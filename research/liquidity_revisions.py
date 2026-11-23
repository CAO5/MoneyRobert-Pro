"""Spot signals from displayed size withdrawal and revisions to upcoming DOGE funding."""
import math
from collections import defaultdict
from datetime import date

from hf_pressure import rows


DAYS = ('2024-07-01', '2024-10-01', '2024-11-01', '2025-04-01',
        '2025-06-01', '2025-08-01', '2025-10-01', '2025-12-01')
DAY = 86400
MICROS = 1_000_000
HOLD = 600


def origin(day):
    return (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY


def spot_trades(day):
    start = origin(day)
    tape = defaultdict(list)
    gaps = [0] * (DAY + 1)
    prior = None
    for row in rows(day, 'DOGEUSDT', 'trades'):
        received = int(row[3])
        sec = received // MICROS - start
        if not 0 <= sec < DAY:
            continue
        ident = int(row[4])
        if ident == prior:
            continue
        if prior is not None and ident != prior + 1:
            gaps[sec + 1] = 1
        prior = ident
        side = row[5]
        price, quantity = map(float, row[6:8])
        if side not in ('buy', 'sell') or not math.isfinite(price) or not math.isfinite(quantity) or min(price, quantity) <= 0:
            raise ValueError('Invalid DOGE trade')
        tape[sec].append((received, side, price, quantity))
    for index in range(1, len(gaps)):
        gaps[index] += gaps[index - 1]
    return tape, gaps


def withdrawn(quotes, tape, gaps):
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(10, len(quotes) - HOLD - 90, 5):
        old, new = quotes[sec - 5], quotes[sec]
        if not old or not new or old[1:3] != new[1:3] or old[0] == new[0]:
            continue
        if gaps[sec + 1] != gaps[sec - 5]:
            continue
        observed = quotes[sec - 5:sec + 1]
        if any(not item or item[1:3] != old[1:3] for item in observed):
            continue
        bids, asks = old[3:5]
        if min(bids * old[1], asks * old[2]) < 2500:
            continue
        counts['eligible'] += 1
        ask_removed, bid_removed = asks - new[4], bids - new[3]
        if ask_removed < .70 * asks and bid_removed < .70 * bids:
            continue
        counts['shocks'] += 1
        buys = sells = 0.
        for elapsed in range(sec - 5, sec + 1):
            for received, side, price, quantity in tape.get(elapsed, ()):
                if received <= old[0]:
                    continue
                if side == 'buy' and price >= old[2]:
                    buys += quantity
                elif side == 'sell' and price <= old[1]:
                    sells += quantity
        if ask_removed >= .70 * asks and abs(bid_removed) <= .20 * bids:
            if ask_removed - buys >= .60 * asks:
                signals['ask_withdrawn'].append((sec, HOLD, ask_removed / asks, buys / asks, bid_removed / bids))
            elif buys >= .60 * asks:
                signals['ask_consumed'].append((sec, HOLD, ask_removed / asks, buys / asks, bid_removed / bids))
        if bid_removed >= .70 * bids and abs(ask_removed) <= .20 * asks and bid_removed - sells >= .60 * bids:
            signals['bid_withdrawn'].append((sec, HOLD, bid_removed / bids, sells / bids, ask_removed / asks))
    return signals, counts


def funding(day):
    start = origin(day)
    timeline = [None] * DAY
    for row in rows(day, 'DOGEUSDT', 'derivative_ticker', 'binance-futures'):
        if not row[4] or not row[5]:
            continue
        exchange, received, settle = (int(row[column]) for column in (2, 3, 4))
        sec = received // MICROS - start
        if not 0 <= sec < DAY or not 0 < settle - exchange <= 28800 * MICROS:
            continue
        rate = float(row[5])
        if not math.isfinite(rate):
            raise ValueError('Invalid published funding rate')
        item = received, settle // MICROS - start, rate
        if timeline[sec] is None or received >= timeline[sec][0]:
            timeline[sec] = item
    latest = None
    for sec, item in enumerate(timeline):
        if item is not None:
            latest = item
        if latest and (start + sec + 1) * MICROS - latest[0] <= 5 * MICROS and sec < latest[1]:
            timeline[sec] = latest
        else:
            timeline[sec] = None
    return timeline


def repriced(quotes, rates):
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(599, len(quotes) - HOLD - 90, 60):
        old, new = rates[sec - 300], rates[sec]
        if not (old and new and quotes[sec - 300] and quotes[sec]) or old[1] != new[1] or new[1] <= sec + HOLD + 90:
            continue
        counts['eligible'] += 1
        change = new[2] - old[2]
        local = math.log(sum(quotes[sec][1:3]) / sum(quotes[sec - 300][1:3]))
        if abs(change) < .00002:
            continue
        counts['shocks'] += 1
        record = (sec, HOLD, change, new[2], local)
        if change > 0 and local <= .001:
            signals['higher_funding'].append(record)
        if change < 0 and local >= -.001:
            signals['lower_funding'].append(record)
        if old[2] * new[2] < 0:
            signals['sign_flip'].append(record)
    return signals, counts
