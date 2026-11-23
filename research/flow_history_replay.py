"""Reconstruct historical Binance Spot books from sequenced snapshots and updates."""
import argparse
import csv
import heapq
import json
import math
import os
import shutil
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from contextlib import ExitStack, contextmanager
from datetime import date, datetime, timedelta, timezone
from itertools import groupby
from pathlib import Path

import pyarrow.parquet as pq

from flow_forward import DAY, EPOCH, NS, sealed_rows, simulate
from flow_forward_review import VERSION, frozen_start
from flow_history_download import inputs, parquet_digest, read_downloads
from flow_portfolio import SYMBOLS
from forward_tape import Tape


BOOK_COLS = ('received_time', 'event_type', 'first_update_id', 'final_update_id', 'last_update_id',
             'side', 'price', 'quantity')
TRADE_COLS = ('received_time', 'trade_id', 'price', 'quantity', 'is_buyer_maker')
HOUR = 3600


def parquet_rows(path, columns):
    for batch in pq.ParquetFile(path).iter_batches(columns=columns):
        data = batch.to_pydict()
        yield from zip(*(data[name] for name in columns))


class Depth:
    def __init__(self, sign):
        self.sign = sign
        self.levels = {}
        self.heap = []

    def set(self, price, quantity):
        if not math.isfinite(price) or price <= 0 or not math.isfinite(quantity) or quantity < 0:
            raise ValueError('Invalid order book level')
        if quantity == 0:
            self.levels.pop(price, None)
        else:
            if price not in self.levels:
                heapq.heappush(self.heap, self.sign * price)
            self.levels[price] = quantity

    def best(self):
        if len(self.heap) > 2 * len(self.levels) + 512:
            self.heap = [self.sign * price for price in self.levels]
            heapq.heapify(self.heap)
        while self.heap and self.sign * self.heap[0] not in self.levels:
            heapq.heappop(self.heap)
        if self.heap:
            price = self.sign * self.heap[0]
            return price, self.levels[price]
        return 0., 0.


class Market:
    def __init__(self):
        self.depth = {'bid': Depth(-1), 'ask': Depth(1)}
        self.book_id = self.trade_id = None
        self.top = None
        self.audit = Counter()

    def books(self, path, hour, *, observe=None):
        quotes, gaps = {}, Counter()
        latest = hour * NS
        prior_snapshot = None
        for key, items in groupby(parquet_rows(path, BOOK_COLS), key=lambda row: row[:5]):
            stamp, kind, first, final, snapshot = key
            second = stamp // NS
            if (stamp < latest and second == latest // NS and kind == 'update'
                    and prior_snapshot is not None and first == prior_snapshot + 1):
                # The archive can list buffered deltas immediately after their snapshot.
                # The delta is actionable only once that snapshot is available.
                stamp = latest
                self.audit['buffered_updates_after_snapshot'] += 1
            if stamp < latest or second >= hour + HOUR:
                raise ValueError(f'Receipt clocks are out of order or outside the UTC hour: {path}')
            latest = stamp
            prior_snapshot = snapshot if kind == 'snapshot' else None
            if kind == 'snapshot':
                if self.book_id is not None and snapshot <= self.book_id:
                    continue
                if self.book_id is not None:
                    self.audit['snapshot_reanchors'] += 1
                    gaps[second] += 1
                    if stamp == hour * NS:
                        quotes[second] = (stamp // 1000, 0., 0., 0., 0.)
                self.depth = {'bid': Depth(-1), 'ask': Depth(1)}
                self.book_id = snapshot
                self.audit['snapshots'] += 1
            elif kind == 'update':
                if self.book_id is None or final <= self.book_id:
                    continue
                if first > self.book_id + 1:
                    self.audit['book_sequence_gaps'] += 1
                    gaps[second] += 1
                    self.book_id = self.top = None
                    quotes[second] = (stamp // 1000, 0., 0., 0., 0.)
                    continue
                self.book_id = final
            else:
                raise ValueError(f'Unknown order book event {kind!r}')
            for _, _, _, _, _, side, price, quantity in items:
                self.depth[side].set(float(price), float(quantity))
            bid, bid_qty = self.depth['bid'].best()
            ask, ask_qty = self.depth['ask'].best()
            top = (bid, ask, bid_qty, ask_qty)
            if observe is not None and kind == 'update' and 0 < bid < ask:
                observe(stamp, self.top, top)
            if top == self.top:
                continue
            self.top = top
            if not 0 < bid < ask:
                gaps[second] += 1
                self.audit['crossed_or_empty_books'] += 1
            elif kind == 'snapshot' and stamp == hour * NS:
                # A provider checkpoint reconstructs depth. It has no observed quote timestamp.
                continue
            quotes[second] = (stamp // 1000, *top)
        return quotes, gaps

    def trades(self, path, hour, gaps, *, observe=None):
        totals = {second: [0., 0., count, 0, None, None] for second, count in gaps.items()}
        latest = hour * NS
        for trade_id, entries in groupby(parquet_rows(path, TRADE_COLS), key=lambda row: row[1]):
            stamp, _, price, quantity, buyer_maker = next(entries)
            for duplicate in entries:
                if duplicate[2:] != (price, quantity, buyer_maker):
                    raise ValueError(f'Conflicting copies of trade {trade_id}: {path}')
                stamp = min(stamp, duplicate[0])
                self.audit['duplicate_trade_rows'] += 1
            second = stamp // NS
            if stamp < latest or second >= hour + HOUR:
                raise ValueError(f'Trade receipts are out of order or outside the UTC hour: {path}')
            latest = stamp
            if trade_id == self.trade_id:
                continue
            state = totals.setdefault(second, [0., 0., 0, 0, None, None])
            if self.trade_id is not None and trade_id != self.trade_id + 1:
                state[2] += 1
                self.audit['trade_sequence_gaps'] += 1
                if trade_id < self.trade_id:
                    continue
            value = float(price) * float(quantity)
            if not isinstance(buyer_maker, bool) or not math.isfinite(value) or value <= 0:
                raise ValueError('Invalid received trade')
            state[0 if buyer_maker else 1] += value
            if state[3] == 0:
                state[4] = trade_id
            state[3] += 1
            state[5] = self.trade_id = trade_id
            self.audit['trades'] += 1
            if observe is not None:
                observe(stamp, float(price), float(quantity), buyer_maker)
        return totals


def rebuild_market_hour(directory, hour, symbol, market):
    quotes, gaps = market.books(directory / f'{symbol}_orderbook.parquet', hour)
    flows = market.trades(directory / f'{symbol}_trades.parquet', hour, gaps)
    return quotes, flows


def rebuild_hour(directory, hour, markets):
    quote_updates, flow_updates = {}, {}
    for symbol, market in markets.items():
        quote_updates[symbol], flow_updates[symbol] = rebuild_market_hour(directory, hour, symbol, market)
    return quote_updates, flow_updates


def init_market_worker(market):
    global worker_market
    worker_market = market


def worker_hour(directory, hour, symbol):
    quotes, flows = rebuild_market_hour(directory, hour, symbol, worker_market)
    return quotes, flows, worker_market.audit


@contextmanager
def parallel_hours(markets):
    with ExitStack() as stack:
        workers = {symbol: stack.enter_context(ProcessPoolExecutor(
            max_workers=1, initializer=init_market_worker, initargs=(market,)))
            for symbol, market in markets.items()}

        def rebuild(directory, hour, markets):
            pending = {symbol: worker.submit(worker_hour, directory, hour, symbol)
                       for symbol, worker in workers.items()}
            quotes, flows = {}, {}
            for symbol, request in pending.items():
                quotes[symbol], flows[symbol], markets[symbol].audit = request.result()
            return quotes, flows

        yield rebuild


def hourly_samples(hour, quotes, flows):
    for second in range(hour, hour + HOUR):
        books = {symbol: list(tape[second]) for symbol, tape in quotes.items() if second in tape}
        trades = {symbol: tape[second] for symbol, tape in flows.items() if second in tape}
        if books or trades:
            received = max([second * NS, *(value[0] * 1000 for value in books.values())])
            yield received, [second, books, trades]


@contextmanager
def watch_downloader(pid):
    if pid is None:
        yield None
        return
    if os.name == 'nt':
        import ctypes
        from ctypes import wintypes

        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
        kernel.WaitForSingleObject.restype = wintypes.DWORD
        kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
        handle = kernel.OpenProcess(0x00100000, False, pid)  # SYNCHRONIZE
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())

        def alive():
            state = kernel.WaitForSingleObject(handle, 0)
            if state not in (0, 258):
                raise ctypes.WinError(ctypes.get_last_error())
            return state == 258

        try:
            yield alive
        finally:
            kernel.CloseHandle(handle)
    else:
        def alive():
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return False
            return True
        yield alive


def verify_downloads(root, paths, downloader_alive=None):
    announced = set()
    while True:
        running = downloader_alive is not None and downloader_alive()
        manifest = read_downloads(root)
        pending = []
        for key in paths:
            record = manifest.get(key)
            if record is None:
                pending.append((key, 'not yet downloaded'))
            elif record['status'] != 'ok':
                pending.append((key, record['error']))
                if running and key not in announced:
                    print(f'Waiting for a verified retry: {key} ({record["error"]})', flush=True)
                    announced.add(key)
        if not pending:
            break
        if not running:
            key, reason = pending[0]
            raise ValueError(f'Incomplete source archive: {key} ({reason})')
        time.sleep(3)

    for key in paths:
        path = root / 'raw' / key
        if parquet_digest(path) != manifest[key]['sha256']:
            raise ValueError(f'Historical source digest mismatch: {key}')


def checked_prefix(prefix, root, first, stop, markets, hour_inputs, downloader_alive):
    with (prefix / 'manifest.csv').open(newline='', encoding='utf-8') as source:
        entries = tuple(csv.DictReader(source))
    if not entries or int(entries[0]['first_utc_ns']) != first * NS:
        raise ValueError('Saved replay does not start at the requested boundary')
    warm = int(entries[-1]['last_utc_ns']) // (NS * HOUR) * HOUR
    resume = warm + HOUR
    if not first < resume < stop:
        raise ValueError('Saved replay does not end inside the requested interval')

    key = datetime.fromtimestamp(warm, timezone.utc).strftime('binance_spot/%Y-%m-%d/%H')
    verify_downloads(root, hour_inputs[key], downloader_alive)
    directory = root / 'raw' / key
    for symbol in SYMBOLS:
        checkpoint = next(parquet_rows(directory / f'{symbol}_orderbook.parquet',
                                       ('received_time', 'event_type', 'last_update_id')), None)
        if checkpoint is None or checkpoint[:2] != (warm * NS, 'snapshot') or checkpoint[2] is None:
            raise ValueError(f'No opening book checkpoint for {symbol} at {key}')
    quotes, flows = rebuild_hour(directory, warm, markets)
    # The opening second can contain carry from the previous hour. Compare the rest.
    saved = [(wall, body) for wall, _, _, kind, body in sealed_rows(prefix)
             if kind == 'portfolio_second' and warm < body[0] < resume]
    rebuilt = [(wall, body) for wall, body in hourly_samples(warm, quotes, flows) if body[0] > warm]
    if not saved or saved != rebuilt:
        raise ValueError('Saved replay and rebuilt transition hour differ')
    return resume, entries


def rebuild(root, start, end, output=None, downloader_pid=None, prefix=None, parallel=False):
    frozen_start(VERSION)
    hour_inputs = {key: tuple(paths) for key, paths in groupby(
        inputs(start, end), key=lambda path: path.rsplit('/', 1)[0])}
    if downloader_pid is None:
        verify_downloads(root, tuple(path for paths in hour_inputs.values() for path in paths))
    first, stop = ((day - EPOCH).days * DAY for day in (start, end))
    output = root if output is None else output
    target = output / 'historical-tape'
    markets = {symbol: Market() for symbol in SYMBOLS}
    begin = first
    with watch_downloader(downloader_pid) as downloader_alive, ExitStack() as stack:
        entries = ()
        if prefix is not None:
            begin, entries = checked_prefix(prefix, root, first, stop, markets, hour_inputs, downloader_alive)
        rebuild_next = stack.enter_context(parallel_hours(markets)) if parallel else rebuild_hour
        tape = Tape(target)
        try:
            for entry in entries:
                name = entry['file']
                shutil.copyfile(prefix / name, target / name)
                tape.writer.writerow((name, entry['rows'], entry['first_utc_ns'], entry['last_utc_ns'],
                                     entry['bytes'], entry['sha256']))
                tape.shard += 1
                tape.sealed += int(entry['bytes'])
            if entries:
                tape.catalog.flush()
                print(f'Reused {len(entries)} verified segments through the hour before '
                      f'{datetime.fromtimestamp(begin, timezone.utc).isoformat()}', flush=True)
            for hour in range(begin, stop, HOUR):
                key = datetime.fromtimestamp(hour, timezone.utc).strftime('binance_spot/%Y-%m-%d/%H')
                if downloader_alive is not None:
                    verify_downloads(root, hour_inputs[key], downloader_alive)
                directory = root / 'raw' / key
                quotes, flows = rebuild_next(directory, hour, markets)
                if hour == first:
                    tape.record('binance', 'archive_boundary', None, first*NS, first*NS)
                for received, body in hourly_samples(hour, quotes, flows):
                    tape.record('binance', 'portfolio_second', body, received, received)
                if (hour - first) % DAY == DAY - HOUR:
                    print(directory.parent.name, 'reconstructed', flush=True)
            if downloader_alive is not None:
                boundary = datetime.fromtimestamp(stop, timezone.utc).strftime('binance_spot/%Y-%m-%d/%H')
                verify_downloads(root, hour_inputs[boundary], downloader_alive)
            tape.record('binance', 'archive_boundary', None, stop*NS, stop*NS)
        finally:
            tape.seal()
            tape.catalog.close()
    result = simulate(target, first, stop)
    result.pop('numerical_gate_passed')
    result['audit']['clock_jumps'] = None
    result['audit']['clock_basis'] = 'Archive receive times only'
    result['audit']['provider'] = 'CryptoHFTData Binance Spot'
    result['audit']['source_events'] = {symbol: dict(market.audit) for symbol, market in markets.items()}
    if prefix is not None:
        result['audit']['resumed_from_utc'] = datetime.fromtimestamp(begin, timezone.utc).isoformat()
        result['audit']['source_events_from_utc'] = datetime.fromtimestamp(begin-HOUR, timezone.utc).isoformat()
    result['scope'] = 'Continuous historical replay'
    result['start_utc_day'] = start.isoformat()
    result['end_utc_exclusive'] = end.isoformat()
    (output / 'historical-results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print('historical-results.json written', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--start', type=date.fromisoformat, required=True)
    parser.add_argument('--end', type=date.fromisoformat, required=True, help='Exclusive UTC date.')
    parser.add_argument('--output', type=Path, help='Output folder for the normalized tape and replay results.')
    parser.add_argument('--download-pid', type=int, help='Consume verified hourly files from this running downloader.')
    parser.add_argument('--resume-from', type=Path, help='Reuse a verified sealed prefix from an incomplete replay.')
    parser.add_argument('--parallel-symbols', action='store_true', help='Rebuild the independent symbol books concurrently.')
    args = parser.parse_args()
    if args.end <= args.start:
        parser.error('The end date must follow the start.')
    if args.download_pid is not None and not 0 < args.download_pid < 2**32:
        parser.error('Invalid download PID.')
    rebuild(args.root, args.start, args.end, args.output, args.download_pid, args.resume_from, args.parallel_symbols)


if __name__ == '__main__':
    main()
