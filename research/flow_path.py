"""Compare two-sided flow with one-sided flow, and isolated jumps with distributed moves."""
import math
from collections import defaultdict

from sell_pressure import volume, window


DAYS = ('2021-05-01', '2021-06-01', '2021-07-01', '2021-09-01', '2022-01-01', '2022-02-01',
        '2022-03-01', '2022-10-01', '2023-04-01', '2023-07-01', '2024-12-01', '2026-09-01')
HOLD = 600
FLOW_RULES = ('balanced_drop', 'persistent_sell_drop', 'balanced_rise')
JUMP_RULES = ('down_jump', 'distributed_drop', 'up_jump')


def log_mid(quote):
    return math.log((quote[1] + quote[2]) / 2) if quote else None


def flow_path(day, quotes):
    return flow_events(quotes, *volume(day, 'binance'))


def flow_events(quotes, sells, buys, gaps, *, move_floor=None, tail=HOLD + 70):
    events = defaultdict(list)
    coverage = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(360, len(quotes) - tail, 30):
        before, now = log_mid(quotes[sec - 60]), log_mid(quotes[sec])
        if before is None or now is None or window(gaps, sec, 360):
            continue
        sold, bought = (window(values, sec, 60) for values in (sells, buys))
        normal = math.fsum(window(values, sec - 60, 300) for values in (sells, buys)) / 5.
        turnover = sold + bought
        coverage['eligible'] += 1
        move = now - before
        if turnover < max(5000., normal * 2) or abs(move) < .004:
            continue
        if move_floor is not None:
            threshold = move_floor(quotes, sec)
            if threshold is None or abs(move) < threshold:
                continue
        coverage['shocks'] += 1
        net_share = (bought - sold) / turnover
        halves = []
        for end in (sec - 30, sec):
            sold_part, bought_part = (window(values, end, 30) for values in (sells, buys))
            if sold_part + bought_part:
                halves.append((bought_part - sold_part) / (sold_part + bought_part))
        if len(halves) != 2:
            continue
        observation = (sec, HOLD, move, net_share, turnover)
        if abs(net_share) <= .15 and max(map(abs, halves)) <= .30:
            coverage['paired'] += 1
            events['balanced_drop' if move < 0 else 'balanced_rise'].append(observation)
        elif move < 0 and net_share <= -.40 and max(halves) <= -.20:
            events['persistent_sell_drop'].append(observation)
    return events, coverage


def jump_statistics(sample):
    changes = [later - earlier for earlier, later in zip(sample, sample[1:])]
    variance = math.fsum(change ** 2 for change in changes)
    if not variance:
        return 0., 0., 0.
    bipower = math.pi / 2 * len(changes) / (len(changes) - 1) * math.fsum(
        abs(left * right) for left, right in zip(changes, changes[1:]))
    dominant = max(changes, key=abs)
    return max(0., (variance - bipower) / variance), dominant, dominant ** 2 / variance


def jumps(quotes):
    events = defaultdict(list)
    coverage = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(125, len(quotes) - HOLD - 70, 30):
        sample = [log_mid(quotes[point]) for point in range(sec - 120, sec + 1, 5)]
        if any(value is None for value in sample):
            continue
        coverage['eligible'] += 1
        move = sample[-1] - sample[0]
        if abs(move) < .004:
            continue
        coverage['shocks'] += 1
        jump_share, dominant, dominance = jump_statistics(sample)
        observation = (sec, HOLD, move, jump_share, dominance)
        if jump_share >= .55 and dominance >= .55 and abs(dominant) >= .003 and dominant * move > 0:
            coverage['paired'] += 1
            events['down_jump' if move < 0 else 'up_jump'].append(observation)
        elif move < 0 and jump_share <= .25 and dominance <= .35:
            events['distributed_drop'].append(observation)
    return events, coverage
