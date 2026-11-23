"""Screen sell-pressure exhaustion against immediate and sustained-pressure entries."""
import csv
import math
from collections import defaultdict
from datetime import date
from pathlib import Path

from hf_pressure import CACHE, rows
from independent_round import book
from participant_traces import replay


ROOT = Path(__file__).parent
DAYS = ('2024-08-01', '2024-09-01', '2024-12-01', '2026-09-01', '2026-10-01', '2026-11-01')
RULES = ('at_local_shock', 'at_confirmed_shock', 'wait_after_confirmed', 'home_cools', 'both_cool', 'pressure_persists')
DAY = 86400
SHOCK = 45
WAIT = 60
HOLD = 600
LOOKBACK = 600


def prefix(values):
    result = [0]
    for value in values:
        result.append(result[-1] + value)
    return result


def volume(day, venue, symbol='DOGEUSDT'):
    origin = (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY
    sell, buy, bad = ([0.] * DAY for _ in range(3))
    seen = set()
    last_stamp = -1
    last_id = None
    for row in rows(day, symbol, 'trades', venue):
        stamp = int(row[3])
        sec = stamp // 1_000_000 - origin
        if stamp < last_stamp:
            raise ValueError(f'Arrival time is not ordered for {venue} {day}')
        last_stamp = stamp
        if not 0 <= sec < DAY:
            continue
        trade_id = int(row[4])
        if trade_id in seen:
            continue
        seen.add(trade_id)
        if venue == 'binance' and last_id is not None and trade_id != last_id + 1:
            bad[sec] += 1
        value = float(row[6]) * float(row[7])
        if not math.isfinite(value) or value <= 0 or row[5] not in ('sell', 'buy'):
            raise ValueError(f'Invalid trade for {venue} {day}')
        if row[5] == 'sell':
            sell[sec] += value
        else:
            buy[sec] += value
        last_id = trade_id
    return tuple(prefix(values) for values in (sell, buy, bad))


def window(values, end, duration):
    return values[end + 1] - values[end + 1 - duration]


def signal_state(quotes, volumes, second):
    sell, buy, bad = volumes
    if not (quotes[second] and quotes[second - SHOCK]) or window(bad, second, LOOKBACK + SHOCK):
        return None
    current = window(sell, second, SHOCK)
    activity = current + window(buy, second, SHOCK)
    usual = window(sell, second - SHOCK, LOOKBACK) * SHOCK / LOOKBACK
    if not usual or not activity:
        return None
    mid_now = sum(quotes[second][1:3])
    mid_before = sum(quotes[second - SHOCK][1:3])
    down = mid_now / mid_before - 1
    return current, current / activity, current / usual, down


def outcome(quotes, volumes, event, decision, initial_sell):
    if not quotes[event] or not quotes[decision] or window(volumes[2], decision, WAIT):
        return None
    changed = sum(quotes[decision][1:3]) / sum(quotes[event][1:3]) - 1
    rate = window(volumes[0], decision, WAIT) * SHOCK / (initial_sell * WAIT)
    return rate, changed


def discover(home, away, home_volume, away_volume):
    signals = defaultdict(list)
    matured = defaultdict(list)
    counts = defaultdict(int)
    available_after = 0
    for sec in range(LOOKBACK + SHOCK, len(home) - HOLD - 90):
        for event, local, other in matured.pop(sec, ()):
            local_after = outcome(home, home_volume, event, sec, local[0])
            other_after = outcome(away, away_volume, event, sec, other[0]) if other else None
            if local_after is None:
                continue
            counts['matured_home'] += 1
            home_rate, home_change = local_after
            away_rate, away_change = other_after if other_after is not None else (None, None)
            cooled = home_rate <= .45 and home_change >= -.001
            if cooled:
                signals['home_cools'].append((sec, HOLD, home_rate, away_rate, home_change))
            if other_after is None:
                continue
            counts['matured_both'] += 1
            signals['wait_after_confirmed'].append((sec, HOLD, home_rate, away_rate, home_change))
            if cooled and away_rate <= .45 and away_change >= -.001:
                signals['both_cool'].append((sec, HOLD, home_rate, away_rate, home_change))
            if min(home_rate, away_rate) >= .75:
                signals['pressure_persists'].append((sec, HOLD, home_rate, away_rate, home_change))
        if sec % 15 or sec < available_after or sec + WAIT + HOLD + 90 >= len(home):
            continue
        local = signal_state(home, home_volume, sec)
        if local is None:
            continue
        counts['eligible_home'] += 1
        if not (local[0] >= 5000 and local[1] >= .72 and local[2] >= 3 and local[3] <= -.0035):
            continue
        counts['home_shocks'] += 1
        other = signal_state(away, away_volume, sec)
        confirmed = other and other[0] >= 2500 and other[1] >= .65 and other[2] >= 2 and other[3] <= -.002
        if confirmed:
            counts['confirmed_shocks'] += 1
            signals['at_confirmed_shock'].append((sec, HOLD, local[2], other[2], local[3]))
        signals['at_local_shock'].append((sec, HOLD, local[2], other[2] if confirmed else None, local[3]))
        matured[sec + WAIT].append((sec, local, other if confirmed else None))
        available_after = sec + WAIT + HOLD
    return signals, counts


def main():
    CACHE.mkdir(exist_ok=True)
    with (ROOT / 'sell_pressure_results.csv').open('w', newline='') as results, (ROOT / 'sell_pressure_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('day', 'rule', 'fee', 'eligible_home', 'home_shocks', 'confirmed_shocks', 'matured_home', 'matured_both', 'raw_signals', 'signal_seconds', 'attempts', 'misses', 'trades', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('day', 'rule', 'fee', 'signal_second', 'hold', 'local_rate_ratio', 'other_rate_ratio', 'local_price_change', 'entry_second', 'exit_second', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        for day in DAYS:
            home = book(day, 'DOGEUSDT')
            away = book(day, 'DOGEUSDT', 'bybit-spot')
            signals, counts = discover(home, away, volume(day, 'binance'), volume(day, 'bybit-spot'))
            context = tuple(counts[key] for key in ('eligible_home', 'home_shocks', 'confirmed_shocks', 'matured_home', 'matured_both'))
            for rule in RULES:
                for fee in (.001, .002):
                    result, ledger = replay(home, signals[rule], fee)
                    writer.writerow((day, rule, fee, *context, *result))
                    journal.writerows((day, rule, fee, *row) for row in ledger)
            print(day, context, {rule: len(signals[rule]) for rule in RULES}, flush=True)


if __name__ == '__main__':
    main()
