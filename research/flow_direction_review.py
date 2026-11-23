"""Diagnose the cost and horizon of spot trades on the existing 30-day sample."""
import argparse
import json
import math
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path
from statistics import mean

from flow_forward import DAY, EPOCH, read_days
from flow_portfolio import STEPS, SYMBOLS


HORIZONS = (60, 600, 3600)
BASE_FEE = .001


def executable_path(quotes, symbol, start, horizons):
    signal = quotes.get((symbol, start))
    entry = quotes.get((symbol, start + 1))
    if not signal or not entry:
        return
    step = STEPS[symbol]
    units = math.floor(24.5 / signal[2] / step) * step
    notional = units * entry[2]
    if not 20 <= notional <= 25 or entry[4] < units:
        return
    returns = []
    for hold in horizons:
        exit_quote = quotes.get((symbol, start + 1 + hold))
        if not exit_quote or exit_quote[3] < units:
            return
        returns.append(exit_quote[1] / entry[2] - 1)
    return returns


def prior_flow(day, close):
    directions = []
    for symbol in ('BTCUSDT', 'ETHUSDT'):
        sold, bought, gaps = day.turnover[symbol]
        if any(gaps[close-600:close]):
            return
        last = close-601
        for i in range(close-600, close):
            if i in day.market_observed[symbol]:
                last = i
            elif i - last >= 60:
                return
        for end in (close-300, close):
            delta = math.fsum(bought[end-300:end]) - math.fsum(sold[end-300:end])
            directions.append(delta)
    if min(directions) > 0:
        return 'buy'
    if max(directions) < 0:
        return 'sell'
    return 'mixed'


def review(tape, baseline):
    first, stop = [(date.fromisoformat(baseline[key]) - EPOCH).days * DAY
                   for key in ('start_utc_day', 'end_utc_exclusive')]
    budgets = {}
    for key, account in baseline['results'].items():
        if not key.endswith('/0.001'):
            continue
        ledger = account['ledger']
        buys = math.fsum(row[5] / (1 + BASE_FEE) for row in ledger)
        sells = math.fsum(row[6] / (1 - BASE_FEE) for row in ledger)
        gross, fees = sells - buys, (sells + buys) * BASE_FEE
        assert math.isclose(gross - fees, account['net_usdt'], abs_tol=1e-8)
        budgets[key] = dict(gross_usdt=gross, fees_usdt=fees, gross_bps_on_buys=1e4*gross/buys,
                            signed_breakeven_fee_bps=1e4*gross/(buys+sells))

    trades = baseline['results']['portfolio/0.001']['ledger']
    grid = range(first, stop - 1 - max(HORIZONS), 3600)
    clock_seconds = {time % DAY for row in trades for time in (row[1], row[1]+1, row[1]+601)}
    wanted = {t for s in grid for t in (s, s+1, *(s+1+h for h in HORIZONS))}
    wanted.update(d+t for d in range(first, stop, DAY) for t in clock_seconds)
    quotes = {}
    flow_group = {}
    by_day = {}
    for t in wanted:
        by_day.setdefault(t // DAY, []).append(t)
    for day in read_days(tape, first, stop, {'clock_jumps': 0, 'filters': {}}):
        for close in range(3600, DAY+1, 3600):
            flow_group[day.start + close] = prior_flow(day, close)
        for t in by_day[day.start // DAY]:
            for symbol in SYMBOLS:
                quotes[symbol, t] = day.quotes[symbol][t-day.start]

    observed = []
    controls = []
    control_counts = []
    for symbol, signal, entry, exit_time, _, debit, credit, _ in trades:
        assert (entry, exit_time) == (signal+1, signal+601)
        actual, = executable_path(quotes, symbol, signal, (600,))
        assert math.isclose(1+actual, (credit/(1-BASE_FEE))/(debit/(1+BASE_FEE)), abs_tol=1e-10)
        others = [value[0] for d in range(first, stop, DAY) if d // DAY != signal // DAY
                  if (value := executable_path(quotes, symbol, d + signal % DAY, (600,)))]
        observed.append(actual)
        controls.append(mean(others))
        control_counts.append(len(others))

    windows = Counter()
    wins = Counter()
    doge_attempts = Counter()
    doge_paths = defaultdict(list)
    for t in grid:
        group = flow_group.get(t)
        if group:
            doge_attempts[group] += 1
            doge = executable_path(quotes, 'DOGEUSDT', t, (3600,))
            if doge:
                doge_paths[group].append((t, doge[0]))
        for symbol in SYMBOLS:
            path = executable_path(quotes, symbol, t, HORIZONS)
            if path is None:
                continue
            windows[symbol] += 1
            for h, gross_return in zip(HORIZONS, path):
                for fee in (BASE_FEE, 2*BASE_FEE):
                    wins[h, fee] += (1 + gross_return)*(1-fee) > 1+fee
    flow_summary = {}
    for group, paths in doge_paths.items():
        values = [value for _,value in paths]
        nets = [(1+v)*(1-BASE_FEE)/(1+BASE_FEE)-1 for v in values]
        flow_summary[group] = dict(attempts=doge_attempts[group], priced=len(paths),
            mean_net_bps=1e4*mean(nets), wins=sum(net>0 for net in nets),
            stress_mean_net_bps=1e4*mean((1+v)*(1-2*BASE_FEE)/(1+2*BASE_FEE)-1 for v in values))
    return dict(budgets=budgets, doge_one_hour_by_prior_btc_eth_flow=flow_summary,
                paired_10m=dict(trades=len(trades), controls_per_trade=control_counts,
                mean_trade_gross_bps=1e4*mean(observed), mean_control_gross_bps=1e4*mean(controls),
                trades_above_matched_mean=sum(a>b for a,b in zip(observed,controls))),
                hourly_grid=dict(scheduled_per_symbol=len(grid), comparable_by_symbol=windows,
                observations=sum(windows.values()), covered_cost_counts={
                    f'{h}s/{fee}':wins[h,fee] for h in HORIZONS for fee in (BASE_FEE, 2*BASE_FEE)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tape', type=Path)
    args = parser.parse_args()
    path = Path(__file__).with_name('flow_history_30d_results.json')
    print(json.dumps(review(args.tape, json.loads(path.read_text())), indent=2))
