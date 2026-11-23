"""Read-only public spot tape recorder. Writes only to a new, explicit directory."""
import argparse
import asyncio
import csv
import gzip
import hashlib
import json
import math
import shutil
import time
from contextlib import suppress
from pathlib import Path

import aiohttp


BINANCE_STREAM = 'wss://data-stream.binance.vision/stream?streams='
FEEDS = {
    'binance': BINANCE_STREAM + 'dogeusdt@bookTicker/dogeusdt@trade',
    'bybit': 'wss://stream.bybit.com/v5/public/spot',
}
SUBSCRIBE = {'op': 'subscribe', 'args': ['orderbook.1.DOGEUSDT', 'publicTrade.DOGEUSDT']}
GIB = 1024 ** 3


class Tape:
    def __init__(self, root):
        root.mkdir(parents=True, exist_ok=False)
        self.root = root
        self.catalog = (root / 'manifest.csv').open('x', newline='', encoding='utf8')
        self.writer = csv.writer(self.catalog)
        self.writer.writerow(('file', 'rows', 'first_utc_ns', 'last_utc_ns', 'bytes', 'sha256'))
        self.catalog.flush()
        self.archive = None
        self.shard = 0
        self.sealed = 0
        self.counts = {}

    def record(self, venue, kind, payload, wall=None, monotonic=None):
        wall = time.time_ns() if wall is None else wall
        monotonic = time.monotonic_ns() if monotonic is None else monotonic
        if self.archive is None or monotonic - self.opened >= 300_000_000_000:
            self.seal()
            self.path = self.root / f'{self.shard:06}.jsonl.gz.partial'
            self.archive = gzip.open(self.path, 'xb', compresslevel=2)
            self.first, self.opened, self.rows = wall, monotonic, 0
        encoded = json.dumps([wall, monotonic, venue, kind, payload], separators=(',', ':'), ensure_ascii=True)
        self.archive.write(encoded.encode('ascii') + b'\n')
        self.last = wall
        self.rows += 1
        key = f'{venue}/{kind}'
        self.counts[key] = self.counts.get(key, 0) + 1

    def seal(self):
        if self.archive is None:
            return
        self.archive.close()
        self.archive = None
        with self.path.open('rb') as source:
            digest = hashlib.file_digest(source, 'sha256').hexdigest()
        final = self.path.with_suffix('')
        self.path.rename(final)
        size = final.stat().st_size
        self.writer.writerow((final.name, self.rows, self.first, self.last, size, digest))
        self.catalog.flush()
        self.sealed += size
        self.shard += 1

    def size(self):
        return self.sealed + (self.path.stat().st_size if self.archive else 0)


class PortfolioTape(Tape):
    """Preserve the final quote and all directional turnover in each receipt second."""
    def __init__(self, root):
        super().__init__(root)
        self.bucket = None
        self.last_ids = {}
        self.quotes = {}
        self.flows = {}

    def flush(self):
        if self.bucket is not None:
            super().record('binance', 'portfolio_second', [self.bucket, self.quotes, self.flows],
                           self.bucket_wall, self.bucket_mono)
        self.bucket = None
        self.quotes = {}
        self.flows = {}

    def record(self, venue, kind, payload, wall=None, monotonic=None):
        if not kind.endswith(('@bookTicker', '@trade')):
            self.flush()
            super().record(venue, kind, payload, wall, monotonic)
            return
        wall = time.time_ns() if wall is None else wall
        monotonic = time.monotonic_ns() if monotonic is None else monotonic
        second = wall // 1_000_000_000
        if second != self.bucket:
            self.flush()
            self.bucket = second
        self.bucket_wall, self.bucket_mono = wall, monotonic
        data = payload['data']
        symbol = data['s']
        if kind.endswith('@bookTicker'):
            self.quotes[symbol] = [wall // 1000, *[float(data[field]) for field in ('b', 'a', 'B', 'A')]]
        else:
            trade_id = int(data['t'])
            previous = self.last_ids.get(symbol)
            flow = self.flows.setdefault(symbol, [0., 0., 0, 0, trade_id, trade_id])
            if previous is not None and trade_id <= previous:
                flow[2] += trade_id < previous
                return
            self.last_ids[symbol] = trade_id
            amount = float(data['p']) * float(data['q'])
            flow[2] += previous is not None and trade_id != previous + 1
            if not math.isfinite(amount) or amount <= 0 or not isinstance(data['m'], bool):
                flow[2] += 1
                return
            flow[0 if data['m'] else 1] += amount
            flow[3] += 1
            flow[5] = trade_id


async def keepalive(socket):
    while True:
        await asyncio.sleep(15)
        await socket.send_json({'op': 'ping'})


async def public_metadata(session, tape, binance_symbols, bybit):
    endpoints = (
        ('binance', 'clock', 'https://data-api.binance.vision/api/v3/time'),
        *(('binance', 'filters', 'https://data-api.binance.vision/api/v3/exchangeInfo?symbol=' + symbol)
          for symbol in binance_symbols),
    )
    if bybit:
        endpoints += (
            ('bybit', 'clock', 'https://api.bybit.com/v5/market/time'),
            ('bybit', 'filters', 'https://api.bybit.com/v5/market/instruments-info?category=spot&symbol=DOGEUSDT'),
        )
    while True:
        for venue, kind, url in endpoints:
            sent = (time.time_ns(), time.monotonic_ns())
            try:
                async with session.get(url, timeout=aiohttp.ClientTimeout(total=8)) as response:
                    response.raise_for_status()
                    data = await response.json()
                    if data.get('retCode', 0) != 0:
                        raise ValueError('Venue rejected metadata query')
                    tape.record(venue, kind, [*sent, data])
            except (aiohttp.ClientError, TimeoutError, ValueError) as error:
                tape.record(venue, 'metadata_error', [kind, str(error)[:220]])
        await asyncio.sleep(21600)


async def receive(venue, endpoint, session, tape):
    backoff = 1
    while True:
        ping = None
        try:
            async with session.ws_connect(endpoint, heartbeat=15) as socket:
                tape.record(venue, 'connect', None)
                if venue == 'bybit':
                    await socket.send_json(SUBSCRIBE)
                    ping = asyncio.create_task(keepalive(socket))
                while True:
                    frame = await socket.receive(timeout=30)
                    wall, mono = time.time_ns(), time.monotonic_ns()
                    if frame.type != aiohttp.WSMsgType.TEXT:
                        raise ConnectionError(f'WebSocket frame {frame.type}')
                    data = json.loads(frame.data)
                    kind = data.get('stream') or data.get('topic') or 'control'
                    tape.record(venue, kind, data, wall, mono)
                    if kind != 'control':
                        backoff = 1
                    if data.get('success') is False:
                        raise ConnectionError('Subscription refused')
        except (aiohttp.ClientError, TimeoutError, ConnectionError, json.JSONDecodeError) as error:
            tape.record(venue, 'disconnect', str(error)[:220])
        finally:
            if ping:
                ping.cancel()
                with suppress(asyncio.CancelledError, ConnectionError, aiohttp.ClientError):
                    await ping
        await asyncio.sleep(backoff)
        backoff = min(backoff * 2, 60)


async def collect(root, seconds, cap, portfolio=False, sampled=False):
    symbols = ('DOGEUSDT', 'BTCUSDT', 'ETHUSDT') if portfolio else ('DOGEUSDT',)
    feeds = FEEDS
    if portfolio:
        streams = '/'.join(f'{symbol.lower()}@{kind}' for symbol in symbols for kind in ('bookTicker', 'trade'))
        feeds = {'binance': BINANCE_STREAM + streams}
    tape = PortfolioTape(root) if sampled else Tape(root)
    started, initial = time.time(), time.monotonic()
    reason = 'interrupted'
    try:
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=None, sock_connect=12), trust_env=True) as session:
            tasks = [asyncio.create_task(receive(venue, endpoint, session, tape)) for venue, endpoint in feeds.items()]
            tasks.append(asyncio.create_task(public_metadata(session, tape, symbols, 'bybit' in feeds)))
            try:
                while True:
                    for task in tasks:
                        if task.done():
                            task.result()
                            raise RuntimeError('Reader stopped unexpectedly')
                    if max(time.time() - started, time.monotonic() - initial) >= seconds:
                        reason = 'deadline'
                    elif (root / 'STOP').exists():
                        reason = 'stop-file'
                    elif tape.size() >= cap - 2 * 1024 ** 2 or shutil.disk_usage(root).free < 10 * GIB:
                        reason = 'storage-limit'
                    if reason != 'interrupted':
                        break
                    await asyncio.sleep(1)
            finally:
                for task in tasks:
                    task.cancel()
                await asyncio.gather(*tasks, return_exceptions=True)
    finally:
        if sampled:
            tape.flush()
        tape.seal()
        tape.catalog.close()
        summary = dict(start_utc=started, end_utc=time.time(), reason=reason, files=tape.shard, bytes=tape.sealed, counts=tape.counts)
        (root / 'summary.json').write_text(json.dumps(summary), encoding='utf8')
        print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('--max-gib', type=int, default=8)
    parser.add_argument('--binance-portfolio', action='store_true',
                        help='Record Binance DOGE, BTC and ETH books and trades.')
    parser.add_argument('--sample-seconds', action='store_true',
                        help='Archive per-second quotes, turnover and trade ID diagnostics.')
    args = parser.parse_args()
    if args.sample_seconds and not args.binance_portfolio:
        parser.error('Per-second archives require --binance-portfolio.')
    maximum_days, maximum_gib = (240, 48) if args.binance_portfolio else (32, 8)
    if not (1 <= args.seconds <= maximum_days * 86400 and 1 <= args.max_gib <= maximum_gib):
        parser.error(f'Maximum duration {maximum_days} days, storage guard 1–{maximum_gib} GiB.')
    asyncio.run(collect(args.root, args.seconds, args.max_gib * GIB, args.binance_portfolio, args.sample_seconds))
