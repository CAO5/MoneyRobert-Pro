"""Volume-weighted trade prices and short reversals conditioned on long-horizon persistence."""
import math
from collections import defaultdict

from flow_path import log_mid
from liquidity_revisions import spot_trades
from sell_pressure import prefix, window


DAYS = tuple('2021-05-01 2021-06-01 2021-07-01 2022-01-01 2022-03-01 2022-04-01 2022-06-01 2022-08-01 2022-11-01 2023-02-01 2023-03-01 2023-10-01 2024-03-01 2024-12-01 2025-07-01 2026-08-01'.split())
HOLD = 600
VWAP_RULES = ('volume_anchor_dip', 'time_anchor_dip', 'volume_anchor_rally')
SCALE_RULES = ('persistent_pullback', 'mean_reverting_pullback', 'persistent_breakdown')


def anchors(day, quotes):
    return volume_events(quotes, *spot_trades(day))


def volume_events(quotes, tape, gaps):
    notionals = [0.] * len(quotes)
    sizes = [0.] * len(quotes)
    active = [0] * len(quotes)
    for sec, trades in tape.items():
        notionals[sec] = math.fsum(price * quantity for _, _, price, quantity in trades)
        sizes[sec] = math.fsum(quantity for _, _, _, quantity in trades)
        active[sec] = bool(trades)
    notional, size, trading = map(prefix, (notionals, sizes, active))
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(600, len(quotes) - HOLD - 70, 60):
        if not quotes[sec] or window(gaps, sec, 600) or window(trading, sec, 600) < 120:
            continue
        samples = [quote for quote in quotes[sec - 590:sec + 1:10] if quote]
        recent = window(notional, sec, 600)
        if len(samples) < 50 or recent < 100_000:
            continue
        counts['eligible'] += 1
        vwap = recent / window(size, sec, 600)
        twap = math.fsum((quote[1] + quote[2]) / 2 for quote in samples) / len(samples)
        below_volume = math.log(quotes[sec][2] / vwap)
        below_time = math.log(quotes[sec][2] / twap)
        event = (sec, HOLD, below_volume, below_time, recent)
        if below_volume <= -.005 and below_time > -.0025:
            counts['shocks'] += 1
            events['volume_anchor_dip'].append(event)
        elif below_time <= -.005 and below_volume > -.0025:
            counts['shocks'] += 1
            events['time_anchor_dip'].append(event)
        elif math.log(quotes[sec][1] / vwap) >= .005 and math.log(quotes[sec][1] / twap) < .0025:
            counts['shocks'] += 1
            events['volume_anchor_rally'].append(event)
    return events, counts


def variance_ratio(prices):
    changes = [after - before for before, after in zip(prices, prices[1:])]
    mean = (prices[-1] - prices[0]) / len(changes)
    variance = math.fsum((change - mean) ** 2 for change in changes) / (len(changes) - 1)
    if variance == 0:
        return None
    horizon = 5
    overlapping = [prices[index] - prices[index - horizon] - horizon * mean for index in range(horizon, len(prices))]
    normalization = horizon * len(overlapping) * (1 - horizon / len(changes))
    return math.fsum(change ** 2 for change in overlapping) / (variance * normalization)


def scale_events(quotes):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    sampled = [log_mid(quote) for quote in quotes[59::60]]
    for minute in range(65, len(sampled) - 12):
        if minute % 5:
            continue
        prior = sampled[minute - 65:minute - 4]
        latest = sampled[minute]
        if latest is None or any(value is None for value in prior):
            continue
        ratio = variance_ratio(prior)
        if ratio is None:
            continue
        counts['eligible'] += 1
        fast = latest - prior[-1]
        slow = prior[-1] - prior[-31]
        if slow < .006 or fast > -.003:
            continue
        counts['shocks'] += 1
        event = (minute * 60 + 59, HOLD, ratio, fast, slow)
        if ratio >= 1.25:
            if -.8 * slow <= fast:
                events['persistent_pullback'].append(event)
            elif fast <= -slow:
                events['persistent_breakdown'].append(event)
        elif ratio <= .85 and -.8 * slow <= fast:
            events['mean_reverting_pullback'].append(event)
    return events, counts
