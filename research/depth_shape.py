"""Test whether standing near-touch liquidity predicts later executable spot returns."""
import math
from collections import defaultdict
from datetime import date

from hf_pressure import rows


DAYS = ('2023-03-01', '2023-06-01', '2023-12-01', '2024-03-01',
        '2024-08-01', '2024-12-01', '2025-04-01', '2025-08-01',
        '2025-12-01', '2026-04-01', '2026-08-01', '2026-10-01')
IMBALANCE = ('deep_bid_support', 'deep_ask_supply', 'best_bid_support')
CONCENTRATION = ('ask_wall', 'bid_wall', 'balanced_diffuse')
DAY = 86400
HOLD = 600


def snapshot(raw):
    if len(raw) != 20 or not all(raw):
        return None
    values = tuple(map(float, raw))
    if not all(math.isfinite(item) and item > 0 for item in values):
        return None
    asks, bids = values[::4], values[2::4]
    if (bids[0] >= asks[0] or any(left >= right for left, right in zip(asks, asks[1:]))
            or any(left <= right for left, right in zip(bids, bids[1:]))):
        return None
    return (bids, values[3::4], asks, values[1::4])


def snapshots(day):
    origin = (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY
    history = [None] * (DAY // 5 + 1)
    last_stamp = -1
    for row in rows(day, 'DOGEUSDT', 'book_snapshot_5'):
        stamp = int(row[3])
        sec = stamp // 1_000_000 - origin
        if stamp < last_stamp:
            raise ValueError('Unordered snapshot receipt times')
        last_stamp = stamp
        if 0 <= sec < DAY and sec % 5 in (3, 4):
            history[sec // 5 + 1] = (stamp, snapshot(row[4:24]))
    return history


def shape(book):
    bids, bid_qty, asks, ask_qty = book
    bid_total, ask_total = sum(bid_qty), sum(ask_qty)
    bid_deep, ask_deep = sum(bid_qty[1:]), sum(ask_qty[1:])
    top = (bid_qty[0] - ask_qty[0]) / (bid_qty[0] + ask_qty[0])
    deep = (bid_deep - ask_deep) / (bid_deep + ask_deep)
    total = (bid_total - ask_total) / (bid_total + ask_total)
    bid_hhi = math.fsum((quantity / bid_total) ** 2 for quantity in bid_qty)
    ask_hhi = math.fsum((quantity / ask_total) ** 2 for quantity in ask_qty)
    return top, deep, total, bid_hhi, ask_hhi


def signals(quotes, history, family):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for index in range(8, (len(quotes) - HOLD - 70) // 5, 2):
        sec = index * 5 - 1
        start, current = history[index - 6], history[index]
        if not start or not current or not start[1] or not current[1] or not quotes[sec] or not quotes[sec - 30]:
            continue
        bids, _, asks, _ = current[1]
        if (bids[0], asks[0]) != quotes[sec][1:3]:
            continue
        if (start[1][0][0], start[1][2][0]) != quotes[sec - 30][1:3]:
            continue
        span_ask, span_bid = (asks[-1] - asks[0]) / asks[0], (bids[0] - bids[-1]) / bids[0]
        if max(span_ask, span_bid) > .005 or max(span_ask, span_bid) > 2 * min(span_ask, span_bid):
            continue
        counts['eligible'] += 1
        move = math.log(sum(quotes[sec][1:3]) / sum(quotes[sec - 30][1:3]))
        if abs(move) > .001:
            continue
        top, deep, total, bid_hhi, ask_hhi = shape(current[1])
        last_top, last_deep, last_total, last_bid_hhi, last_ask_hhi = shape(start[1])
        rule = None
        if family == 'standing_depth':
            if abs(top) <= .25 and abs(last_top) <= .25 and min(deep, last_deep) >= .55:
                rule = 'deep_bid_support'
            elif abs(top) <= .25 and abs(last_top) <= .25 and max(deep, last_deep) <= -.55:
                rule = 'deep_ask_supply'
            elif min(top, last_top) >= .55 and max(abs(deep), abs(last_deep)) <= .25:
                rule = 'best_bid_support'
            record = (sec, HOLD, top, deep, move)
        else:
            if max(abs(total), abs(last_total)) > .2:
                continue
            if min(ask_hhi, last_ask_hhi) >= .5 and max(bid_hhi, last_bid_hhi) <= .32:
                rule = 'ask_wall'
            elif min(bid_hhi, last_bid_hhi) >= .5 and max(ask_hhi, last_ask_hhi) <= .32:
                rule = 'bid_wall'
            elif max(bid_hhi, ask_hhi, last_bid_hhi, last_ask_hhi) <= .27:
                rule = 'balanced_diffuse'
            record = (sec, HOLD, total, bid_hhi, ask_hhi)
        if rule:
            counts['shocks'] += 1
            events[rule].append(record)
    return events, counts
