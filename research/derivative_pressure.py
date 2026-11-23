"""Screen spot reactions to inverse-perpetual premiums and contract inventory shocks."""
import csv
import io
import math
from collections import defaultdict, deque
from datetime import datetime, timezone
from statistics import median

from hf_pressure import rows
from position_divergence import archive


DAYS = tuple(f'2025-{month:02}-01' for month in range(2, 8))
RECEIPT_DAYS = tuple(f'2026-{month:02}-01' for month in range(2, 8))
INVENTORY_RULES = ('unwind_drop', 'buildup_drop', 'unwind_recovery')
INVERSE_RULES = ('inverse_rich', 'inverse_cheap', 'raw_usd_gap')
RECEIPT_RULES = ('joint_inventory_drop', 'joint_inventory_buildup', 'bybit_inventory_drop')
HOLD = 600


def midpoint(quote):
    return (quote[1] + quote[2]) / 2


def observed_interest(day):
    origin = int(datetime.fromisoformat(day).replace(tzinfo=timezone.utc).timestamp())
    file = f'futures/um/daily/metrics/DOGEUSDT/DOGEUSDT-metrics-{day}.zip'
    result = {}
    for row in csv.DictReader(io.StringIO(archive(file))):
        if row['symbol'] != 'DOGEUSDT':
            raise ValueError('Unexpected metrics symbol')
        sec = int(datetime.fromisoformat(row['create_time']).replace(tzinfo=timezone.utc).timestamp()) - origin
        interest = float(row['sum_open_interest'])
        if sec in result or not math.isfinite(interest) or interest <= 0 or sec % 300 or not 0 <= sec < 86400:
            raise ValueError('Invalid open interest observation')
        result[sec] = interest
    return result


def inventory_events(quotes, published):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sample in sorted(published):
        if sample - 300 not in published or sample - 300 < 0 or sample + 360 + HOLD + 70 >= len(quotes):
            continue
        signal = sample + 360
        before, near, late = (quotes[second - 1] for second in (sample - 300, sample, signal + 1))
        if not all((before, near, late)):
            continue
        counts['eligible'] += 1
        change = math.log(published[sample] / published[sample - 300])
        market = math.log(midpoint(near) / midpoint(before))
        recovery = math.log(midpoint(late) / midpoint(near))
        if market > -.0025 or abs(change) < .003:
            continue
        counts['shocks'] += 1
        observed = (signal, HOLD, change, market, recovery)
        if change < 0:
            events['unwind_drop'].append(observed)
            if recovery >= .001 and market + recovery <= 0:
                events['unwind_recovery'].append(observed)
        else:
            events['buildup_drop'].append(observed)
    return events, counts


def inverse_events(spot, linear, inverse, conversion):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    levels = deque(maxlen=60)
    for sec in range(59, len(spot) - HOLD - 70, 10):
        local, tether, coin, fx = spot[sec], linear[sec], inverse[sec], conversion[sec]
        if not all((local, tether, coin, fx)):
            levels.append(None)
            continue
        fx_mid = midpoint(fx)
        level = math.log(midpoint(coin) * fx_mid / midpoint(tether))
        recent = [value for value in levels if value is not None]
        if len(recent) >= 45:
            counts['eligible'] += 1
            center = median(recent)
            paid_premium = math.log(coin[1] * fx[1] / tether[2]) - center
            paid_discount = math.log(coin[2] * fx[2] / tether[1]) - center
            local_gap = math.log(midpoint(tether) / midpoint(local))
            threshold = .0015
            if paid_premium >= threshold and abs(local_gap) <= .0015:
                counts['shocks'] += 1
                events['inverse_rich'].append((sec, HOLD, paid_premium, local_gap, fx_mid))
            elif paid_discount <= -threshold and abs(local_gap) <= .0015:
                counts['shocks'] += 1
                events['inverse_cheap'].append((sec, HOLD, paid_discount, local_gap, fx_mid))
            naive = math.log(midpoint(coin) / midpoint(tether))
            if abs(naive) >= threshold and max(paid_premium, -paid_discount) < threshold:
                events['raw_usd_gap'].append((sec, HOLD, naive, local_gap, fx_mid))
        levels.append(level)
    return events, counts


def received_interest(day):
    start = int(datetime.fromisoformat(day).replace(tzinfo=timezone.utc).timestamp())
    timeline = [None] * 86400
    prior = -1
    for row in rows(day, 'DOGEUSDT', 'derivative_ticker', 'bybit'):
        stamp = int(row[3])
        sec = stamp // 1_000_000 - start
        if stamp < prior:
            raise ValueError('Unordered receipt times')
        prior = stamp
        if not 0 <= sec < 86400 or not row[7]:
            continue
        interest = float(row[7])
        if not math.isfinite(interest) or interest <= 0:
            raise ValueError('Invalid open interest')
        timeline[sec] = (stamp, interest)
    latest = None
    for sec, record in enumerate(timeline):
        if record:
            latest = record
        if latest and (start + sec + 1) * 1_000_000 - latest[0] <= 15_000_000:
            timeline[sec] = latest[1]
        else:
            timeline[sec] = None
    return timeline


def receipt_events(quotes, bybit, archive_interest):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for latest in sorted(archive_interest):
        sec = latest + 360
        if latest < 300 or sec + HOLD + 70 >= len(quotes):
            continue
        if latest - 300 not in archive_interest:
            continue
        previous, current = bybit[latest - 300], bybit[latest]
        left, right = quotes[latest - 301], quotes[latest - 1]
        if not all((previous, current, left, right, quotes[sec])):
            continue
        counts['eligible'] += 1
        move = math.log(midpoint(right) / midpoint(left))
        if move > -.0025:
            continue
        counts['shocks'] += 1
        bybit_change = math.log(current / previous)
        binance = math.log(archive_interest[latest] / archive_interest[latest - 300])
        record = (sec, HOLD, bybit_change, binance, move)
        if bybit_change <= -.003 and binance <= -.003:
            counts['paired'] += 1
            events['joint_inventory_drop'].append(record)
        elif min(bybit_change, binance) >= .003:
            counts['paired'] += 1
            events['joint_inventory_buildup'].append(record)
        elif bybit_change <= -.003:
            events['bybit_inventory_drop'].append(record)
    return events, counts
