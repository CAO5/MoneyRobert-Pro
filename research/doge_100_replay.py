"""Run preregistered DOGE screens 65–100 in parallel on public archives."""
import argparse
import csv
import math
import time
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

from doge_liquidity_signals import depth_signals, quote_paths, trade_signals
from doge_market_signals import big_assets, derivative_signals, venue_signals
from doge_replay_inputs import Signal
from hf_pressure import CACHE
from independent_round import book
from participant_traces import replay


ROOT = Path(__file__).parent
FEES = (.001, .002)
RESULT_COLUMNS = ('number', 'group', 'phase', 'day', 'fee', 'status', 'eligible', 'signals',
                  'attempts', 'misses', 'trades', 'wins', 'net_usdt', 'max_dd_pct',
                  'unpriced_seconds', 'exit_delay_seconds', 'detail')
TRADE_COLUMNS = ('number', 'group', 'phase', 'day', 'fee', 'signal_second', 'planned_hold',
                 'indicator_a', 'indicator_b', 'indicator_c', 'entry_second', 'exit_second',
                 'units', 'debit_usdt', 'credit_usdt', 'net_usdt')


def month_starts(year, months):
    return tuple(f'{year}-{month:02d}-01' for month in months)


LOCAL_MONTHS = (1, 3, 5, 7, 9, 11)
LOCAL_DAYS = month_starts(2024, LOCAL_MONTHS) + month_starts(2026, LOCAL_MONTHS)
DEPTH_DAYS = (month_starts(2023, (3, 6, 12)) + month_starts(2024, (3, 8, 12)) +
              month_starts(2025, (4, 8, 12)) + month_starts(2026, (4, 8, 10)))
ASSET_DAYS = month_starts(2024, (1, 3, 8, 9)) + month_starts(2026, (1, 3, 9, 11))


@dataclass(frozen=True)
class Batch:
    name: str
    numbers: range
    days: tuple[str, ...]
    early_days: int
    discover: Callable

    def phase(self, day):
        return 'early' if day in self.days[:self.early_days] else 'late'


BATCHES = (
    Batch('quote_path', range(65, 71), LOCAL_DAYS, 6, quote_paths),
    Batch('visible_depth', range(71, 77), DEPTH_DAYS, 6, depth_signals),
    Batch('trades', range(77, 83), LOCAL_DAYS, 6, trade_signals),
    Batch('other_assets', range(83, 87), ASSET_DAYS, 4, big_assets),
    Batch('derivatives', range(87, 95), month_starts(2026, range(2, 9)), 4, derivative_signals),
    Batch('other_venues', range(95, 101), month_starts(2025, range(1, 10)), 4, venue_signals),
)


def failed(number, batch, day, fee, error, signal=None):
    counts = (signal.eligible, len(signal.observations)) if signal else ('', '')
    return (number, batch.name, batch.phase(day), day, fee, 'incomplete',
            *counts, *([''] * 8), str(error))


def run_day(day, batch_indices):
    summary, journal = [], []
    try:
        home = book(day, 'DOGEUSDT')
    except (OSError, EOFError, ValueError) as error:
        for index in batch_indices:
            batch = BATCHES[index]
            summary.extend(failed(number, batch, day, fee, error) for number in batch.numbers for fee in FEES)
        return summary, journal

    for index in batch_indices:
        batch = BATCHES[index]
        try:
            signals = batch.discover(day, home)
        except (OSError, EOFError, ValueError) as error:
            summary.extend(failed(number, batch, day, fee, error) for number in batch.numbers for fee in FEES)
            continue
        for number in batch.numbers:
            signal: Signal = signals[number]
            for fee in FEES:
                key = number, batch.name, batch.phase(day), day, fee
                try:
                    metrics, ledger = replay(home, signal.observations, fee)
                except ValueError as error:
                    summary.append(failed(number, batch, day, fee, error, signal))
                    continue
                _, raw_signals, *fills = metrics
                status = 'replayed' if signal.eligible else 'no_observations'
                summary.append((*key, status, signal.eligible, raw_signals, *fills, ''))
                journal.extend((*key, *row) for row in ledger)
    return summary, journal


def write_csv(filename, header, records):
    with (ROOT / filename).open('w', encoding='utf-8', newline='') as output:
        writer = csv.writer(output)
        writer.writerow(header)
        writer.writerows(records)


def check_ledger(results, trades):
    totals = defaultdict(list)
    for trade in trades:
        totals[trade[:5]].append(trade[-1])
        signal, entered, exited = trade[5], trade[10], trade[11]
        if not signal < entered <= exited or not math.isclose(trade[-2]-trade[-3], trade[-1], abs_tol=1e-8):
            raise ValueError(f'Invalid execution order or debit/credit for {trade[:5]}')
    for row in results:
        key, status = row[:5], row[5]
        if status == 'incomplete':
            continue
        count, wins, net = row[10:13]
        records = totals.pop(key, [])
        if count != len(records) or wins != sum(pnl > 0 for pnl in records) or not math.isclose(
                net, math.fsum(records), abs_tol=1e-8):
            raise ValueError(f'Result does not reconcile to fills for {key}')
    if totals:
        raise ValueError('Journal has fills without a matching daily result')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--workers', type=int, default=8, choices=range(1, 13))
    args = parser.parse_args()
    CACHE.mkdir(exist_ok=True)
    started = time.perf_counter()
    calendar = defaultdict(list)
    for index, batch in enumerate(BATCHES):
        for day in batch.days:
            calendar[day].append(index)
    results, trades = [], []
    with ProcessPoolExecutor(args.workers) as pool:
        jobs = {pool.submit(run_day, day, indices): day for day, indices in sorted(calendar.items())}
        for completed, job in enumerate(as_completed(jobs), 1):
            summary, journal = job.result()
            results.extend(summary)
            trades.extend(journal)
            print(f'{completed}/{len(jobs)} {jobs[job]} {len(summary)} rows, {len(journal)} trades', flush=True)
    results.sort(key=lambda row: (row[0], row[3], row[4]))
    trades.sort(key=lambda row: (row[0], row[3], row[4], row[5]))
    check_ledger(results, trades)
    write_csv('doge_100_results.csv', RESULT_COLUMNS, results)
    write_csv('doge_100_trades.csv', TRADE_COLUMNS, trades)
    complete = sum(row[5] == 'replayed' for row in results)
    measured = len({row[0] for row in results if row[5] == 'replayed'})
    print(f'{measured} registered rules, {complete}/{len(results)} result rows, {len(trades)} trades, '
          f'{args.workers} workers, {time.perf_counter()-started:.2f} seconds', flush=True)


if __name__ == '__main__':
    main()
