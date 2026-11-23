"""Measure calendar-week outcomes for the 2024+ rotation and passive controls."""
import argparse
import hashlib
import json
import math
import random
from concurrent.futures import ThreadPoolExecutor
from datetime import timedelta
from functools import partial
from pathlib import Path
from statistics import mean, median

from slow_spot_replay import BAR, STEPS, SYMBOLS, epoch, utc
from weekly_profit_2024_review import DATA_START, END, FIRST, load_2024
from weekly_profit_policy import COSTS, WEEK
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_delayed_account import Bounds, bitstamp_fill, first_minute
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM
from weekly_rotation_reentry_delay import cached_reader


ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / 'weekly_profit_2024_results.json'
INITIAL = 100.0
EPSILON = 1e-9
DELAY = 60
BLOCKS = (4, 13)
RESAMPLES = 5_000
BINANCE_API = 'https://api.binance.com/api/v3/klines'


def near(left, right):
    return math.isclose(left, right, rel_tol=0, abs_tol=EPSILON)


def week_row(start, opening, closing, bounds, *, deployed=None, hours=None):
    return dict(week=utc(start), opening_equity=opening, closing_equity=closing,
                net=closing-opening, drawdown_amount=bounds.drawdown_amount,
                drawdown_pct=100*bounds.drawdown, entry_notional=deployed, exposed_hours=hours)


def window_risk(equity):
    peak, peak_index = equity[0], 0
    longest_recovery, completed_recoveries = 0, 0
    for index, value in enumerate(equity[1:], 1):
        if value + EPSILON >= peak:
            duration = index - peak_index
            if duration > 1:
                longest_recovery = max(longest_recovery, duration)
                completed_recoveries += 1
            peak = max(peak, value)
            peak_index = index
    return dict(longest_completed_recovery_weeks=longest_recovery,
                completed_recoveries=completed_recoveries,
                open_recovery_weeks=len(equity)-1-peak_index)


def summarize(rows, bound):
    values = [row['net'] for row in rows]
    equity = [rows[0]['opening_equity'], *[r['closing_equity'] for r in rows]]
    net = equity[-1] - equity[0]
    if not near(net, math.fsum(values)):
        raise ValueError('Weekly outcomes do not reconcile with final equity')
    streak = longest_streak = 0
    for value in values:
        streak = streak + 1 if value < -EPSILON else 0
        longest_streak = max(longest_streak, streak)
    positive = sorted((row for row in rows if row['net'] > EPSILON), key=lambda r: -r['net'])
    top = positive[:3]
    after_top = net - math.fsum(row['net'] for row in top)
    return dict(weeks=len(rows), net=net, mean_week_net=mean(values), median_week_net=median(values),
                best_week=max(rows, key=lambda row: row['net'])['week'],
                worst_week=min(rows, key=lambda row: row['net'])['week'], worst_week_net=min(values),
                up_weeks=len(positive), down_weeks=sum(x < -EPSILON for x in values),
                flat_weeks=sum(abs(x) <= EPSILON for x in values),
                weekly_profit_above_drawdown=sum(r['net'] > r['drawdown_amount'] + EPSILON for r in rows),
                longest_losing_streak=longest_streak, drawdown_amount=bound.drawdown_amount,
                drawdown_pct=100*bound.drawdown,
                net_without_top_3=after_top, top_3=[dict(week=r['week'], net=r['net']) for r in top],
                **window_risk(equity))


def drawdown_trade(trade, cash, week_bound, account_bound, bars, pre_fill, factor):
    symbol, entry, close = (trade[key] for key in ('symbol', 'entry', 'exit'))
    units = trade['quantity']
    held_cash = cash - trade['debit']
    for stamp in range(entry-DELAY, close-DELAY, BAR):
        bar = bars[symbol][stamp]
        for bound in (week_bound, account_bound):
            bound.mark(held_cash+units*bar.high*factor, held_cash+units*bar.low*factor)
    low, high = pre_fill(symbol, close-DELAY)
    for bound in (week_bound, account_bound):
        bound.mark(held_cash+units*high*factor, held_cash+units*low*factor)
        bound.mark(cash+trade['pnl'], cash+trade['pnl'])


def rotation_weeks(ledger, starts, bars, pre_fill, cost):
    fee, slip = cost
    factor = (1-fee)*(1-slip)
    by_entry = {}
    for trade in ledger:
        entry, close = trade['entry'], trade['exit']
        start = entry-DELAY
        if (start not in starts or start in by_entry or not entry < close <= entry+WEEK
                or (close-DELAY) % BAR or not near(trade['credit']-trade['debit'], trade['pnl'])):
            raise ValueError('Trade is inconsistent with weekly settlement assumptions')
        by_entry[start] = trade
    cash = INITIAL
    total = Bounds()
    rows = []
    for start in starts:
        week = Bounds()
        week.peak = cash
        trade = by_entry.get(start)
        opening = cash
        active_hours = 0
        if trade:
            drawdown_trade(trade, cash, week, total, bars, pre_fill, factor)
            cash += trade['pnl']
            active_hours = (trade['exit']-trade['entry'])/3600
        rows.append(week_row(start, opening, cash, week,
                             deployed=trade['debit']/(1+fee) if trade else 0, hours=active_hours))
    score = summarize(rows, total)
    score['active_weeks'] = len(ledger)
    score['avg_notional_when_invested'] = mean(t['debit']/(1+fee) for t in ledger) if ledger else None
    score['avg_notional_weighted_by_holding_time'] = mean(row['entry_notional']*row['exposed_hours']/168 for row in rows)
    return dict(score=score, weeks=rows)


def buy_and_hold(symbol, starts, end, bars, price, pre_fill, cost):
    fee, slip = cost
    points = [*starts, end]
    boundary_prices = [price((symbol, start+DELAY)) for start in points]
    buy_price = boundary_prices[0]*(1+slip)
    step = STEPS[symbol]
    units = math.floor(NOMINAL/buy_price/step)*step
    if units*buy_price < ORDER_MINIMUM:
        raise ValueError('Passive control cannot meet the same entry minimum')
    cash = INITIAL - units*buy_price*(1+fee)
    factor = (1-fee)*(1-slip)
    total = Bounds()
    rows = []
    opening = INITIAL
    for index, (start, stop) in enumerate(zip(points, points[1:])):
        week = Bounds()
        week.peak = opening
        closing = cash + units*boundary_prices[index+1]*factor
        for stamp in range(start, stop, BAR):
            bar = bars[symbol][stamp]
            for bound in (week, total):
                bound.mark(cash+units*bar.high*factor, cash+units*bar.low*factor)
        low, high = pre_fill(symbol, stop)
        for bound in (week, total):
            bound.mark(cash+units*high*factor, cash+units*low*factor)
            bound.mark(closing, closing)
        rows.append(week_row(start, opening, closing, week))
        opening = closing
    score = summarize(rows, total)
    score['initial_notional'] = units*buy_price
    score['final_sell_below_minimum'] = units*boundary_prices[-1]*(1-slip) < ORDER_MINIMUM
    return dict(score=score, weeks=rows)


def same_entries_hold(ledger, starts, bars, price, pre_fill, cost):
    fee, slip = cost
    held = []
    unfillable = 0
    for trade in ledger:
        exit_time = trade['entry']+WEEK
        proceeds = trade['quantity']*price((trade['symbol'], exit_time))*(1-slip)
        unfillable += proceeds < ORDER_MINIMUM
        credit = proceeds*(1-fee)
        held.append(dict(trade, exit=exit_time, credit=credit, pnl=credit-trade['debit']))
    return dict(**rotation_weeks(held, starts, bars, pre_fill, cost), weeks_below_exit_minimum=unfillable)


def block_intervals(values):
    """Descriptive percentile intervals for the mean based on circular blocks."""
    n = len(values)
    intervals = {}
    for block in BLOCKS:
        rng = random.Random(20241120)
        size, remainder = divmod(n, block)
        doubled = values * 2
        sums = [math.fsum(doubled[i:i+block]) for i in range(n)]
        tails = [math.fsum(doubled[i:i+remainder]) for i in range(n)]
        draws = sorted((sum(sums[rng.randrange(n)] for _ in range(size)) + tails[rng.randrange(n)])/n
                       for _ in range(RESAMPLES))
        intervals[block] = [draws[int((RESAMPLES-1)*p)] for p in (.025, .975)]
    return intervals


def evaluate(venue, archived):
    series, source = load_2024(venue)
    if source['bars_sha256'] != archived['source']['bars_sha256']:
        raise ValueError(f'Scoped candles changed for {venue}')
    bars = {symbol: {b.time:b for b in data} for symbol, data in series.items()}
    first, end = (epoch(day.isoformat()) for day in (FIRST, END))
    starts = list(range(first, end, WEEK))
    price = cached_reader(venue, partial(minute_open, endpoint=BINANCE_API) if venue == 'binance' else bitstamp_fill)
    pre_fill = first_minute(venue, binance_endpoint=BINANCE_API)
    points = [(symbol, point) for symbol in SYMBOLS for point in (*starts, end)]
    with ThreadPoolExecutor(6) as pool:
        opening = list(pool.map(lambda item: price((item[0], item[1]+DELAY)), points))
        ranges = list(pool.map(lambda item: pre_fill(*item), points))
    reference = list(zip(points, opening, ranges))
    source['monday_minutes_sha256'] = hashlib.sha256(json.dumps(reference,separators=(',', ':')).encode('ascii')).hexdigest()

    outcomes = {}
    archived_lanes = archived['primary'][f'{FIRST.isoformat()}/{END.isoformat()}']
    no_risk = Bounds()
    cash = [week_row(start, INITIAL, INITIAL, no_risk, deployed=0, hours=0) for start in starts]
    cash_case = dict(score=summarize(cash, no_risk), weeks=cash)
    for lane, cost in COSTS.items():
        benchmarks = {symbol: buy_and_hold(symbol, starts, end, bars, price, pre_fill, cost) for symbol in SYMBOLS}
        cases = dict(cash=cash_case, **benchmarks)
        for name, recorded in archived_lanes.items():
            result = recorded[lane]
            if result['open_at_end'] or result['missed_entries'] or result['missing_4h_bars_held']:
                raise ValueError('Weekly attribution requires complete simulated fills')
            ledger = result['ledger']
            case = rotation_weeks(ledger, starts, bars, pre_fill, cost)
            score = case['score']
            for metric in ('net', 'drawdown_amount', 'drawdown_pct'):
                if not near(score[metric], result[metric]):
                    raise ValueError(f'{venue} {lane} {name} {metric} disagrees with scoped replay')
            hold = same_entries_hold(ledger, starts, bars, price, pre_fill, cost)
            pairs = {label: [r['net'] for r in control['weeks']] for label, control in cases.items()}
            pairs['same_entries_hold'] = [r['net'] for r in hold['weeks']]
            comparisons = {}
            for label, control_values in pairs.items():
                differences = [r['net']-other for r, other in zip(case['weeks'], control_values)]
                comparisons[label] = dict(net_difference=math.fsum(differences), mean_week_difference=mean(differences),
                                          mean_week_diagnostic_interval=block_intervals(differences))
            outcomes.setdefault(lane, {})[name] = dict(**case, same_entries_hold=hold, paired=comparisons)
        outcomes[lane]['benchmarks'] = cases
    return dict(currency=archived['currency'], source=source, costs=outcomes)


def report():
    raw = SOURCE.read_bytes()
    baseline = json.loads(raw)['venues']
    with ThreadPoolExecutor(2) as pool:
        venues = dict(zip(baseline, pool.map(lambda v: evaluate(v, baseline[v]), baseline)))
    return dict(data_start=DATA_START, first_week=FIRST.isoformat(), last_week_end=END.isoformat(),
                week_attribution='UTC Monday dates, previous-week expiry credited at Monday 00:01 before new entries',
                bootstrap=dict(method='circular percentile intervals, descriptive on reused history',
                               block_weeks=BLOCKS, resamples=RESAMPLES),
                source_sha256=hashlib.sha256(raw).hexdigest(), venues=venues)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    payload = json.dumps(report(), indent=2, allow_nan=False) + '\n'
    if args.output:
        args.output.write_text(payload, encoding='utf-8')
    else:
        print(payload, end='')
