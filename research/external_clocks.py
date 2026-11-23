"""BTC quote-grid and NYSE clock controls. Signal inputs and execution stay separate."""
import csv
import io
import math
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor
from datetime import date, timedelta
from pathlib import Path

from position_divergence import archive


GRID_DAYS = ('2024-07-01', '2024-08-01', '2024-09-01', '2024-10-01',
             '2025-07-01', '2025-08-01', '2025-10-01', '2025-12-01')
CLOCK_DAYS = ('2024-02-01', '2024-03-01', '2024-04-01', '2024-05-01', '2024-07-01',
              '2024-08-01', '2024-10-01', '2024-11-01', '2025-04-01', '2025-05-01',
              '2025-07-01', '2025-08-01', '2025-09-01', '2025-10-01', '2025-12-01',
              '2026-04-01', '2026-05-01', '2026-06-01', '2026-07-01', '2026-09-01', '2026-10-01')
TICK = .00000001
HOLD = 600
CLOCK_RULES = ('open_buy', 'close_buy', 'open_after_drop', 'close_after_drop',
               'placebo_open_buy', 'placebo_close_buy', 'placebo_open_after_drop', 'placebo_close_after_drop')
HOLIDAYS = {
    2024: set('01-01 01-15 02-19 03-29 05-27 06-19 07-04 09-02 11-28 12-25'.split()),
    2025: set('01-01 01-09 01-20 02-17 04-18 05-26 06-19 07-04 09-01 11-27 12-25'.split()),
    2026: set('01-01 01-19 02-16 04-03 05-25 06-19 07-03 09-07 11-26 12-25'.split()),
}
EARLY_CLOSE = {
    2024: {'07-03', '11-29', '12-24'},
    2025: {'07-03', '11-28', '12-24'},
    2026: {'11-27', '12-24'},
}


def mid(quote):
    return (quote[1] + quote[2]) / 2


def quote_grid(home, bitcoin, cross):
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    next_check = 30
    for sec in range(30, len(home) - HOLD - 90):
        if sec < next_check:
            continue
        earlier, current = sec - 5, sec
        if not all(series[at] for series in (home, bitcoin, cross) for at in (earlier, current)):
            continue
        before, after = cross[earlier], cross[current]
        if abs(before[2] - before[1] - TICK) > 1e-14 or abs(after[2] - after[1] - TICK) > 1e-14:
            continue
        counts['eligible'] += 1
        gap = (after[1] - before[1]) / TICK
        sign = 1 if math.isclose(gap, 1., abs_tol=1e-6) else -1 if math.isclose(gap, -1., abs_tol=1e-6) else 0
        if not sign:
            continue
        btc_move = math.log(mid(bitcoin[current]) / mid(bitcoin[earlier]))
        doge_move = math.log(mid(home[current]) / mid(home[earlier]))
        if abs(btc_move) > .0005:
            continue
        relative = doge_move - btc_move
        counts['shocks'] += 1
        event = (sec, HOLD, gap, btc_move, relative)
        if sign > 0:
            events['up_confirmed' if relative > .001 else 'up_cash_lags'].append(event)
        elif relative >= -.001:
            events['down_cash_lags'].append(event)
        next_check = sec + 30
    return events, counts


def first_sunday(year, month):
    beginning = date(year, month, 1)
    return beginning + timedelta(days=(6 - beginning.weekday()) % 7)


def anchors(day):
    value = date.fromisoformat(day)
    suffix = day[5:]
    if value.weekday() >= 5 or suffix in HOLIDAYS[value.year] or suffix in EARLY_CLOSE[value.year]:
        return {}
    summer = first_sunday(value.year, 3) + timedelta(days=7) <= value < first_sunday(value.year, 11)
    offset = 4 if summer else 5
    opening = (9 + offset) * 60 + 30
    closing = (16 + offset) * 60
    return {'open': opening, 'close': closing, 'placebo_open': opening - 60, 'placebo_close': closing - 60}


def clock_events(day, quote_at):
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for label, minute in anchors(day).items():
        signal = (minute + 5) * 60 - 1
        current, earlier = quote_at(signal), quote_at(signal - 300)
        if current is None or earlier is None:
            continue
        change = math.log(current / earlier)
        counts['eligible'] += 1
        record = (signal, HOLD, change, minute, 0.)
        signals[label + '_buy'].append(record)
        if change <= -.0025:
            counts['shocks'] += 1
            signals[label + '_after_drop'].append(record)
    return signals, counts


def equities_clock(day, home):
    def at(second):
        return mid(home[second]) if home[second] else None
    return clock_events(day, at)


def minute_windows(month):
    raw = archive(f'spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-{month}.zip')
    days = defaultdict(dict)
    for row in csv.reader(io.StringIO(raw)):
        timestamp = int(row[0])
        stamp = timestamp // (1_000_000 if timestamp >= 10 ** 15 else 1000)
        day = date(1970, 1, 1) + timedelta(days=stamp // 86400)
        second = stamp % 86400
        opening, high, low, closing = map(float, row[1:5])
        if second % 60 or second in days[day] or not 0 < low <= min(opening, closing) <= max(opening, closing) <= high:
            raise ValueError(f'Invalid archived minute {month} {stamp}')
        days[day][second] = (opening, high, low, closing)
    return days


def clock_day_proxy(day, bars):
    def numbered_close(second):
        bar = bars.get(second // 60 * 60)
        return bar[3] if bar else None
    found, counts = clock_events(day, numbered_close)
    output = []
    for rule in CLOCK_RULES:
        for signal, hold, move, _, _ in found[rule]:
            entry, close = signal + 61, signal + 61 + hold
            required = [bars.get(sec) for sec in range(entry, close + 60, 60)]
            if not all(required):
                continue
            qty = math.floor(24.5 / numbered_close(signal))
            if not 20 <= qty * required[0][0] <= 25:
                continue
            output.append((day, rule, entry, close, move, qty, required[0][0], required[-1][0],
                           min(bar[2] for bar in required[:-1])))
    return output, counts['eligible']


def scan_month(month):
    journal = []
    missing = 0
    for day, bars in sorted(minute_windows(month).items()):
        if not anchors(day.isoformat()):
            continue
        found, eligible = clock_day_proxy(day.isoformat(), bars)
        missing += 4 - eligible
        journal.extend(found)
    return journal, missing


def scan_rules(journal, rule, fee):
    cash, peak, drawdown = 100., 100., 0.
    completed = []
    for row in journal:
        if row[1] != rule or drawdown >= .08:
            continue
        day, _, entry, close, _, qty, entry_price, exit_price, low = row
        debit = qty * entry_price * (1 + fee)
        if debit > cash:
            raise ValueError('Insufficient cash in minute exclusion test')
        proceeds = qty * exit_price * (1 - fee)
        drawdown = max(drawdown, 1 - (cash - debit + qty * low * (1 - fee)) / peak)
        net = proceeds - debit
        cash += net
        peak = max(peak, cash)
        drawdown = max(drawdown, 1 - cash / peak)
        completed.append((day, rule, fee, entry, close, qty, debit, proceeds, net))
    return len(completed), sum(row[-1] > 0 for row in completed), cash - 100., drawdown * 100, completed


def screen_calendar():
    root = Path(__file__).parent
    months = [f'{year}-{month:02}' for year in (2024, 2025) for month in range(1, 13)]
    with ProcessPoolExecutor(3) as workers:
        monthly = list(workers.map(scan_month, months))
    with (root / 'equity_clock_minute_results.csv').open('w', newline='') as results, (root / 'equity_clock_minute_trades.csv').open('w', newline='') as ledger:
        writer, journal = csv.writer(results), csv.writer(ledger)
        writer.writerow(('period', 'rule', 'fee', 'raw_signals_with_full_bars', 'missing_anchor_inputs', 'trades', 'wins', 'net_usdt', 'drawdown_proxy_pct'))
        journal.writerow(('period', 'day', 'rule', 'fee', 'entry_sec', 'exit_sec', 'qty', 'debit', 'credit', 'net_usdt'))
        for year, block in zip((2024, 2025), (monthly[:12], monthly[12:])):
            events = [event for day_rows, _ in block for event in day_rows]
            missing = sum(count for _, count in block)
            for rule in CLOCK_RULES:
                for fee in (.001, .002):
                    trades, wins, net, drawdown, notes = scan_rules(events, rule, fee)
                    writer.writerow((year, rule, fee, sum(row[1] == rule for row in events), missing, trades, wins, net, drawdown))
                    journal.writerows((year, *note) for note in notes)
            print(year, len(events), missing, flush=True)


if __name__ == '__main__':
    screen_calendar()
