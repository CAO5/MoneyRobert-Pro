"""Compare two fixed sector filters and one exit on the existing DOGE flow signal."""
import argparse
import csv
import io
import math
from concurrent.futures import ProcessPoolExecutor
from datetime import date
from pathlib import Path
from statistics import median

from flow_path import DAYS, HOLD, flow_path
from hf_pressure import CACHE
from independent_round import book
from participant_traces import replay
from position_divergence import archive


ROOT = Path(__file__).parent
SYMBOLS = ('DOGEUSDT', 'PEPEUSDT', 'SHIBUSDT', 'FLOKIUSDT')
RECHECK = ('2023-01-01', '2023-05-01', '2023-06-01', '2024-02-01', '2024-03-01', '2024-07-01',
           '2025-02-01', '2025-03-01', '2025-04-01', '2025-09-01', '2026-03-01', '2026-04-01')
MORE = ('2024-01-01', '2024-04-01', '2024-08-01', '2024-11-01',
        '2025-01-01', '2025-06-01', '2025-08-01', '2025-12-01',
        '2026-01-01', '2026-05-01', '2026-08-01', '2026-11-01')
PHASES = {day: phase for phase, days in (('first', DAYS), ('recheck', RECHECK), ('additional', MORE))
          for day in days}
FEES = (.001, .002)


def closes(day, symbol):
    path = f'spot/daily/klines/{symbol}/1m/{symbol}-1m-{day}.zip'
    raw = archive(path)
    origin_ms = (date.fromisoformat(day) - date(1970, 1, 1)).days * 86_400_000
    divisor = 1000 if day >= '2025-01-01' else 1
    minutes = {}
    for row in csv.reader(io.StringIO(raw)):
        minute, remainder = divmod(int(row[0]) // divisor - origin_ms, 60_000)
        price = float(row[4])
        if remainder or not 0 <= minute < 1440 or minute in minutes or not math.isfinite(price) or price <= 0:
            raise ValueError(f'Invalid minute in {path}')
        minutes[minute] = price
    return minutes


def prior_returns(tapes, signal):
    # The last minute must be closed at least five seconds before the shock.
    minute = (signal - 60 - 5) // 60 - 1
    required = tuple(range(minute - 5, minute + 1))
    if not all(all(point in tape for point in required) for tape in tapes):
        return None
    return tuple(tape[minute] / tape[minute - 5] - 1 for tape in tapes)


def study(day):
    quotes = book(day, 'DOGEUSDT')
    events, _ = flow_path(day, quotes)
    original = events['balanced_drop']
    stop_bids = {}
    eligible, strict, relative = [], [], []
    observations = []
    context = 'prelisting' if day < '2023-06-01' else 'no_base_signal'
    tapes = None
    if original and context != 'prelisting':
        try:
            tapes = tuple(closes(day, symbol) for symbol in SYMBOLS)
            context = 'received_archives'
        except (OSError, ValueError) as error:
            context = f'unavailable: {error}'
    for event in original:
        sec = event[0]
        bids = [quote[1] for quote in quotes[sec-59:sec+1] if quote]
        if len(bids) >= 54:
            stop_bids[sec] = min(bids) * .999
        changes = prior_returns(tapes, sec) if tapes is not None else None
        strong = broad = False
        if changes is not None:
            doge, *peers = changes
            strong = doge > .001 and max(peers) < -.002
            broad = sum(change < 0 for change in peers) >= 2 and doge - median(peers) >= .002
            eligible.append(event)
            if strong:
                strict.append(event)
            if broad:
                relative.append(event)
        observations.append((PHASES[day], day, sec, context, *(changes or ('',) * 4),
                             int(strong), int(broad), stop_bids.get(sec, '')))
    summary, journal = [], []
    strategies = (('original', original), ('original_matched', eligible), ('strict', strict),
                  ('relative', relative), ('stop', original))
    for rule, events in strategies:
        for fee in FEES:
            options = {'stop_bids': stop_bids} if rule == 'stop' else {}
            metrics, ledger = replay(quotes, events, fee, **options)
            early = sum(row[6] < row[5] + HOLD for row in ledger)
            key = (PHASES[day], day, rule, fee)
            summary.append((*key, context, len(original), len(eligible), len(stop_bids), early, *metrics[1:]))
            journal.extend((*key, *row, stop_bids.get(row[0], '') if rule == 'stop' else '') for row in ledger)
    return summary, journal, observations


def write(name, header, rows):
    with (ROOT / name).open('w', newline='', encoding='utf-8') as stream:
        writer = csv.writer(stream)
        writer.writerow(header)
        for records in rows:
            writer.writerows(records)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--workers', type=int, default=8, choices=range(1, 13))
    args = parser.parse_args()
    CACHE.mkdir(exist_ok=True)
    with ProcessPoolExecutor(args.workers) as pool:
        blocks = list(pool.map(study, PHASES))
    summary, trades, signals = zip(*blocks)
    key = ('phase', 'day', 'rule', 'fee')
    write('flow_sector_results.csv',
          (*key, 'context', 'base_signals', 'sector_covered_signals', 'stop_defined_signals', 'early_exits',
           'signals', 'attempts', 'misses', 'trades', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds',
           'delayed_exit_seconds'), summary)
    write('flow_sector_trades.csv',
          (*key, 'signal_sec', 'hold', 'shock', 'net_flow_share', 'turnover', 'entry_sec', 'exit_sec',
           'qty', 'debit_usdt', 'credit_usdt', 'net_usdt', 'stop_bid'), trades)
    write('flow_sector_signals.csv',
          ('phase', 'day', 'signal_sec', 'context', 'doge_prior', 'pepe_prior', 'shib_prior',
           'floki_prior', 'strict_pass', 'relative_pass', 'stop_bid'), signals)
    print(f'{len(blocks)} dates, {sum(map(len, summary))} result rows, '
          f'{sum(map(len, trades))} trades, {args.workers} workers')


if __name__ == '__main__':
    main()
