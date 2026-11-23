"""Compare a break in BTC downside co-movement and deceleration of DOGE downside variance."""
import math
import statistics
from collections import defaultdict

from flow_path import log_mid


DAYS = tuple('2024-01-01 2024-02-01 2024-03-01 2024-07-01 2024-08-01 2024-09-01 2024-10-01 2024-12-01 2025-01-01 2025-02-01 2025-03-01 2025-04-01 2025-05-01 2025-06-01 2025-07-01 2025-08-01 2025-09-01 2025-10-01 2025-11-01 2025-12-01 2026-01-01 2026-02-01 2026-03-01 2026-04-01 2026-09-01 2026-10-01 2026-11-01'.split())
CO_MOVEMENT = ('downside_break', 'still_coupled', 'upside_break')
SEMIVARIANCE = ('downside_decelerates', 'downside_accelerates', 'upside_decelerates')
HOLD = 600


def changes(quote_path, last, length, stride):
    prices = [log_mid(quote_path[second]) for second in range(last - length, last + 1, stride)]
    if any(item is None for item in prices):
        return None
    return [later - earlier for earlier, later in zip(prices, prices[1:])]


def coupling(left, right):
    if not any(left) or not any(right):
        return None
    power_left, power_right = (math.fsum(value * value for value in series) for series in (left, right))
    dot = math.fsum(first * second for first, second in zip(left, right))
    return dot / math.sqrt(power_left * power_right)


def aligned_events(doge, bitcoin):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(1200, len(doge) - HOLD - 70, 60):
        local, broad = (changes(quotes, sec, 1200, 20) for quotes in (doge, bitcoin))
        if local is None or broad is None:
            continue
        counts['eligible'] += 1
        old_local, now_local = local[:45], local[45:]
        old_broad, now_broad = broad[:45], broad[45:]
        if abs(sum(now_broad)) > .0015 or math.fsum(value * value for value in now_broad) < 1e-7:
            continue
        local_move = sum(now_local)
        if abs(local_move) < .003:
            continue
        down = local_move < 0
        old_pairs = [(home, market) for home, market in zip(old_local, old_broad) if (market < 0 if down else market > 0)]
        now_pairs = [(home, market) for home, market in zip(now_local, now_broad) if (market < 0 if down else market > 0)]
        if len(old_pairs) < 12 or len(now_pairs) < 5:
            continue
        old_score = coupling(*zip(*old_pairs))
        active_score = coupling(*zip(*now_pairs))
        if old_score is None or active_score is None or old_score < .65:
            continue
        counts['shocks'] += 1
        row = (sec, HOLD, old_score, active_score, local_move)
        if active_score < .2:
            events['downside_break' if down else 'upside_break'].append(row)
        elif down and active_score >= .65:
            events['still_coupled'].append(row)
    return events, counts


def semi(sequence, sign):
    return math.fsum(value * value for value in sequence if value * sign > 0)


def variance_events(doge):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(900, len(doge) - HOLD - 70, 60):
        series = changes(doge, sec, 900, 10)
        if series is None:
            continue
        history, first, second = series[:60], series[60:75], series[75:]
        move = sum(first) + sum(second)
        counts['eligible'] += 1
        if abs(move) < .004:
            continue
        sign = -1 if move < 0 else 1
        reference = statistics.median(semi(history[index:index + 15], sign) for index in (0, 15, 30, 45))
        initial, final = semi(first, sign), semi(second, sign)
        opposite = semi(first + second, -sign)
        if max(initial, final) < 3 * max(reference, 1e-8) or initial + final < 3 * opposite:
            continue
        counts['shocks'] += 1
        row = (sec, HOLD, move, initial, final)
        if final <= .4 * initial:
            events['downside_decelerates' if sign < 0 else 'upside_decelerates'].append(row)
        elif sign < 0 and final >= 2.5 * initial:
            events['downside_accelerates'].append(row)
    return events, counts
