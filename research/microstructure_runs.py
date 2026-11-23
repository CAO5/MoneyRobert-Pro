"""Compare top-of-book order flow and the timing of aggressive trade runs."""
import math
from collections import defaultdict
from datetime import date

from hf_pressure import rows
from liquidity_revisions import spot_trades
from sell_pressure import prefix, window


DAYS = ('2021-04-01', '2021-08-01', '2021-10-01', '2021-12-01',
        '2022-02-01', '2022-05-01', '2022-09-01', '2022-12-01',
        '2023-09-01', '2023-12-01', '2024-08-01', '2024-11-01',
        '2025-01-01', '2025-08-01', '2026-02-01', '2026-09-01')
SECOND = 1_000_000
DAY = 86_400
HOLD = 600
BOOK_RULES = ('bid_pressure_lag', 'ask_pressure_lag', 'bid_pressure_confirmed')
RUN_RULES = ('buy_run', 'sell_run', 'spread_out_buys')


def book_delta(before, after):
    old_bid, old_ask, old_bid_qty, old_ask_qty = before
    bid, ask, bid_qty, ask_qty = after
    bid_change = (bid_qty if bid >= old_bid else 0.) - (old_bid_qty if bid <= old_bid else 0.)
    ask_change = (ask_qty if ask <= old_ask else 0.) - (old_ask_qty if ask >= old_ask else 0.)
    return bid_change - ask_change


def book_flow(day):
    origin = (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY
    signed, gross = [0.] * DAY, [0.] * DAY
    updates = [0] * DAY
    previous = None
    last = -1
    for row in rows(day, 'DOGEUSDT', 'quotes'):
        stamp = int(row[3])
        sec = stamp // SECOND - origin
        if stamp < last:
            raise ValueError('Unordered quote receipt times')
        ask_qty, ask, bid, bid_qty = map(float, row[4:8])
        current = (bid, ask, bid_qty, ask_qty)
        if not 0 <= sec < DAY or not all(math.isfinite(value) for value in current) or not 0 < bid < ask or min(bid_qty, ask_qty) <= 0:
            previous = None
        else:
            if previous and stamp - last <= 2 * SECOND and current != previous:
                change = book_delta(previous, current)
                signed[sec] += change
                gross[sec] += abs(change)
                updates[sec] += 1
            previous = current
        last = stamp
    return tuple(prefix(series) for series in (signed, gross, updates))


def mid_move(quotes, sec, length):
    before, after = quotes[sec - length], quotes[sec]
    return math.log(sum(after[1:3]) / sum(before[1:3])) if before and after else None


def book_events(quotes, series):
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    imbalance, activity, updates = series
    for sec in range(30, len(quotes) - HOLD - 70, 30):
        move = mid_move(quotes, sec, 30)
        if move is None or window(updates, sec, 30) < 30:
            continue
        counts['eligible'] += 1
        flow = window(imbalance, sec, 30)
        gross = window(activity, sec, 30)
        baseline = min(quotes[sec - 30][3:5])
        if gross <= 0 or abs(flow) < max(gross * .35, 12 * baseline):
            continue
        counts['shocks'] += 1
        row = (sec, HOLD, flow / gross, flow / baseline, move)
        if flow > 0:
            signals['bid_pressure_confirmed' if move > .0015 else 'bid_pressure_lag'].append(row)
        elif move >= -.0015:
            signals['ask_pressure_lag'].append(row)
    return signals, counts


def run_events(quotes, tape, gaps):
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(30, len(quotes) - HOLD - 70, 30):
        move = mid_move(quotes, sec, 30)
        if move is None or gaps[sec + 1] != gaps[sec - 29]:
            continue
        amounts = defaultdict(float)
        active_seconds = defaultdict(set)
        for tick in range(sec - 29, sec + 1):
            for _, side, price, quantity in tape.get(tick, ()):
                amounts[side] += price * quantity
                active_seconds[side].add(tick)
        turnover = sum(amounts.values())
        if turnover < 20_000:
            continue
        counts['eligible'] += 1
        strongest = max(amounts, key=amounts.get)
        share = amounts[strongest] / turnover
        if share < .8 or abs(move) > .002:
            continue
        counts['shocks'] += 1
        final_run = 0
        for tick in range(sec, sec - 30, -1):
            opposite = 'buy' if strongest == 'sell' else 'sell'
            if tick not in active_seconds[strongest] or tick in active_seconds[opposite]:
                break
            final_run += 1
        row = (sec, HOLD, share, final_run, move)
        if final_run >= 8:
            counts['paired'] += 1
            signals['buy_run' if strongest == 'buy' else 'sell_run'].append(row)
        elif final_run <= 2 and strongest == 'buy':
            signals['spread_out_buys'].append(row)
    return signals, counts


def order_flow(day, quotes):
    return book_events(quotes, book_flow(day))


def aggressor_runs(day, quotes):
    return run_events(quotes, *spot_trades(day))
