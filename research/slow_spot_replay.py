"""Compare three long-only, low-turnover rules on the same four-hour spot bars."""
import csv
import io
import json
import math
import tempfile
import zipfile
from collections import namedtuple
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean, median

from flow_portfolio import STEPS
from spot_pullback import SYMBOLS, month_data


BAR = 14400
Bar = namedtuple('Bar', 'time open high low close')
RULES = ('pullback', 'breakout', 'rotation')


def utc(stamp):
    return datetime.fromtimestamp(stamp, timezone.utc).isoformat()


def load_month(key):
    symbol, month = key
    name = f'{symbol}-4h-{month}'
    path = Path(tempfile.gettempdir()) / 'moneyrobert-spot-v1' / (name + '.zip')
    if not path.exists():
        month_data(key)
    with zipfile.ZipFile(path) as archive, archive.open(name + '.csv') as raw:
        rows = []
        for row in csv.reader(io.TextIOWrapper(raw, encoding='ascii')):
            t = int(row[0]) // (1000000 if len(row[0]) == 16 else 1000)
            bar = Bar(t, *map(float, row[1:5]))
            if not 0 < bar.low <= min(bar.open, bar.close) <= max(bar.open, bar.close) <= bar.high:
                raise ValueError(f'Invalid OHLC in {name}')
            rows.append(bar)
        return rows


def load(warmup='2021-12-01', end='2026-11-01', *, allow_aligned_gaps=False):
    months = [f'{year}-{month:02}' for year in range(int(warmup[:4]), int(end[:4])+1)
              for month in range(1, 13) if warmup[:7] <= f'{year}-{month:02}' < end[:7]]
    keys = [(s, m) for s in SYMBOLS for m in months]
    bars = {s: [] for s in SYMBOLS}
    with ThreadPoolExecutor(8) as pool:
        for (symbol, _), chunk in zip(keys, pool.map(load_month, keys)):
            bars[symbol].extend(chunk)
    expected = list(range(epoch(warmup), epoch(end), BAR))
    observed = {s: [b.time for b in series] for s, series in bars.items()}
    if any(times != expected for times in observed.values()):
        times = observed[SYMBOLS[0]]
        if (not allow_aligned_gaps or any(x != times for x in observed.values())
                or times != sorted(set(times)) or not set(times).issubset(expected)):
            raise ValueError('Archive has a missing, duplicate or misaligned four-hour bar')
        for symbol, series in bars.items():
            by_time = {b.time: b for b in series}
            bars[symbol] = [by_time.get(t, Bar(t, None, None, None, None)) for t in expected]
    return bars


def epoch(day):
    return int(datetime.fromisoformat(day).replace(tzinfo=timezone.utc).timestamp())


def exit_reason(rule, symbol, exits, prior_close, entry_price, age, stop_distance=.03,
                stop_confirmed=True):
    if stop_confirmed and prior_close <= (1-stop_distance)*entry_price:
        return 'position_stop'
    if symbol in exits or (rule == 'pullback' and age >= 12):
        return 'rule'


def rotation_decisions(bars):
    weekly = {}
    for i in range(169, len(bars[SYMBOLS[0]])):
        stamp = bars[SYMBOLS[0]][i].time
        if stamp % 86400 or datetime.fromtimestamp(stamp, timezone.utc).weekday() != 0:
            continue
        if any(v[j].close is None for v in bars.values() for j in (i-1, i-169)):
            continue
        rank = {s: v[i-1].close/v[i-169].close-1 for s,v in bars.items()}
        best = max(rank, key=rank.get)
        picks = [best] if rank[best] > 0 else []
        weekly[i] = (picks, [s for s in SYMBOLS if s not in picks])
    return weekly


def decisions(bars):
    closes = {s: [b.close for b in v] for s, v in bars.items()}
    signals = dict(pullback={}, breakout={}, rotation=rotation_decisions(bars))
    for i in range(169, len(bars[SYMBOLS[0]])):
        stamp = bars[SYMBOLS[0]][i].time
        rank = {s: c[i-1]/c[i-169]-1 for s, c in closes.items()}
        entries, exits = [], []
        for s, c in closes.items():
            avg = mean(c[i-120:i])
            if c[i-1] > avg and c[i-1] < c[i-2] < c[i-3]:
                entries.append(s)
            if c[i-1] <= avg or c[i-1] >= mean(c[i-6:i]):
                exits.append(s)
        signals['pullback'][i] = (sorted(entries, key=rank.get, reverse=True), exits)
        if stamp % 86400:
            continue
        entries, exits = [], []
        for s, c in closes.items():
            if c[i-1] > max(c[i-121:i-1:6]):
                entries.append(s)
            if c[i-1] < min(c[i-61:i-1:6]):
                exits.append(s)
        signals['breakout'][i] = (sorted(entries, key=rank.get, reverse=True), exits)
    return signals


def simulate(bars, signals, rule, start, end, cost, *, entry_limits=None, profit_protection=False,
             stop_confirmations=1, account_pause_drawdown=.08, entry_target=24.5,
             minimum_order_notional=None, add_signals=None, net_profit_target=None, net_profit_lock=None,
             reentry_policy=None):
    first = (epoch(start)-bars[SYMBOLS[0]][0].time)//BAR
    stop = (epoch(end)-bars[SYMBOLS[0]][0].time)//BAR
    cash = close_peak = bar_peak = 100.
    dd_close = dd_bound = units = 0.
    halted = symbol = None
    ledger = []
    fee, slip = cost
    sell_factor = (1-slip)*(1-fee)
    passive = rule in SYMBOLS
    skipped_size = 0
    missing_with_position = 0
    missing_execution = 0
    deferred_exit = None
    entry_minimum = 20. if minimum_order_notional is None else minimum_order_notional
    track_legs = minimum_order_notional is not None
    stopped_assets = {}
    reentry_skips = []
    if any(bars[s][stop-1].close is None for s in SYMBOLS):
        raise ValueError('Replay must end on an observed close')

    def close(price, stamp, why):
        nonlocal cash, units, symbol
        if track_legs and units*price*(1-slip) < minimum_order_notional:
            raise ValueError(f'Exit below the simulated order minimum at {utc(stamp)}')
        credit = units*price*sell_factor
        pnl = credit-debit
        if reentry_policy and pnl < 0 and why in ('position_stop', 'net_profit_lock'):
            stopped_assets[symbol] = stamp, debit/(units*sell_factor)
        trade = dict(symbol=symbol, entry=entry_time, exit=stamp,
                     quantity=units, debit=debit, credit=credit, pnl=pnl, reason=why)
        if track_legs:
            trade['buy_legs'] = legs
        ledger.append(trade)
        cash += credit
        symbol, units = None, 0.

    def buy_terms(wanted, i, target):
        price = bars[wanted][i].open*(1+slip)
        step = STEPS[wanted]
        quantity = math.floor(target/price/step)*step
        return price, quantity

    def blocks_reentry(wanted, i, stamp):
        if wanted not in stopped_assets:
            return False
        stopped, breakeven = stopped_assets[wanted]
        if reentry_policy == 'one_week':
            blocked = stamp <= stopped+7*86400
        elif reentry_policy == 'recover':
            prior_close = bars[wanted][i-1].close
            blocked = prior_close is None or prior_close < breakeven
        else:
            raise ValueError('Unknown reentry policy')
        if not blocked:
            del stopped_assets[wanted]
        return blocked

    for i in range(first, stop):
        stamp = bars[SYMBOLS[0]][i].time
        picks, exits = (([rule] if i == first else [], []) if passive
                        else signals[rule].get(i, ([], [])))
        missing = bars[SYMBOLS[0]][i].open is None
        sold = False
        if symbol and not passive:
            prior = bars[symbol][i-1].close
            stop_closes = stop_closes + 1 if prior is not None and prior <= (1-stop_distance)*entry_price else 0
            stop_confirmed = stop_confirmations is not None and stop_closes >= stop_confirmations
            why = ('account_drawdown' if halted else deferred_exit or
                   exit_reason(rule, symbol, exits, prior, entry_price, i-entry_index, stop_distance,
                               stop_confirmed))
            if prior is not None:
                peak_close = max(peak_close, prior)
            if (not why and profit_protection and prior is not None and peak_close >= 1.12*entry_price
                    and prior <= entry_price+(peak_close-entry_price)*.5):
                why = 'profit_protection'
            if not why and prior is not None and (net_profit_target or net_profit_lock):
                observed = units*prior*sell_factor/debit - 1
                if net_profit_target is not None and observed >= net_profit_target:
                    why = 'net_profit_target'
                elif net_profit_lock:
                    arm, floor = net_profit_lock
                    peak_net = units*peak_close*sell_factor/debit - 1
                    if peak_net >= arm and observed <= floor:
                        why = 'net_profit_lock'
            if why and missing:
                if deferred_exit is None:
                    missing_execution += 1
                    deferred_exit = why
            elif why:
                close(bars[symbol][i].open, stamp, why)
                deferred_exit = None
                bar_peak = max(bar_peak, cash)
                dd_bound = max(dd_bound, 1-cash/bar_peak)
                sold = True
        adding = (symbol and add_signals and not halted and deferred_exit is None
                  and add_signals.get(i) == symbol and len(legs) == 1)
        if missing:
            missing_with_position += bool(symbol)
            missing_execution += bool(adding or (picks and not symbol and not halted))
            continue
        if adding:
            price, amount = buy_terms(symbol, i, 12.25)
            nominal = amount*price
            cost_to_add = nominal*(1+fee)
            if nominal < entry_minimum:
                skipped_size += 1
            else:
                if cost_to_add > cash or debit+cost_to_add > 24.5*(1+fee)+1e-9:
                    raise ValueError('Invalid total position size')
                bar_peak = max(bar_peak, cash+units*bars[symbol][i].open*sell_factor)
                cash -= cost_to_add
                debit += cost_to_add
                entry_price = (entry_price*units+price*amount)/(units+amount)
                units += amount
                peak_close = price
                stop_closes = 0
                legs.append(dict(time=stamp, quantity=amount, debit=cost_to_add))
        entering = picks and not symbol and not halted and (not sold or (rule == 'rotation' and why == 'rule'))
        if entering and blocks_reentry(picks[0],i,stamp):
            reentry_skips.append(dict(symbol=picks[0],time=stamp))
            entering = False
        if entering:
            wanted = picks[0]
            default = (entry_target, .03)
            size, stop_distance = entry_limits.get((i, wanted), default) if entry_limits else default
            entry_price, units = buy_terms(wanted, i, size)
            nominal = units*entry_price
            debit = nominal*(1+fee)
            if nominal < entry_minimum and (entry_limits or track_legs):
                skipped_size += 1
                units = 0.
            elif not (entry_minimum <= nominal <= 25 and debit <= cash):
                raise ValueError('Invalid position size')
            else:
                symbol = wanted
                cash -= debit
                peak_close = entry_price
                stop_closes = 0
                entry_index, entry_time = i, stamp
                legs = [dict(time=stamp, quantity=units, debit=debit)]
        high = low = equity = cash
        if symbol:
            b = bars[symbol][i]
            high, low, equity = [cash+units*p*sell_factor for p in (b.high, b.low, b.close)]
            if i+1 == stop:
                close(b.close, stamp+BAR, 'end')
        close_peak = max(close_peak, equity)
        bar_peak = max(bar_peak, high)
        dd_close = max(dd_close, 1-equity/close_peak)
        dd_bound = max(dd_bound, 1-low/bar_peak)
        if not passive and not halted and dd_close >= account_pause_drawdown:
            halted = utc(stamp+BAR)
    pnl = [t['pnl'] for t in ledger]
    if not math.isclose(sum(pnl), cash-100, abs_tol=1e-8):
        raise ValueError('Ledger reconciliation failed')
    result = dict(net=cash-100, drawdown_close_pct=100*dd_close, drawdown_upper_pct=100*dd_bound,
                wins=sum(x>0 for x in pnl), trades=len(pnl), halted=halted,
                missed_size=skipped_size,
                median_hold_hours=median((t['exit']-t['entry'])/3600 for t in ledger) if pnl else None,
                ledger=ledger)
    if missing_with_position or missing_execution:
        result['gap_audit'] = dict(missing_bars_held=missing_with_position,
                                   affected_orders=missing_execution)
    if reentry_policy:
        result['reentry_skips'] = reentry_skips
    return result


def main():
    bars = load()
    signals = decisions(bars)
    result = {}
    periods = [(f'{year}-01-01', f'{year+1}-01-01') for year in range(2022, 2026)]
    periods += [('2026-01-01','2026-11-01'), ('2022-01-01','2026-11-01')]
    for start, end in periods:
        result[start+'/'+end] = {
            rule: {mode: simulate(bars, signals, rule, start, end, costs)
                   for mode, costs in [('base', (.001,.0005)), ('stress', (.002,.001))]}
            for rule in (*RULES, *SYMBOLS)}
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
