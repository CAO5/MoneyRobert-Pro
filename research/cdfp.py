"""Record forward-only CDFP v0.1 research samples from public OKX trades."""

import argparse
import asyncio
import json
import math
import sqlite3
import time
from bisect import bisect_left, insort
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path

import aiohttp

from okx_behavior_tape import GIB, collect


ASSETS = ('BTC', 'ETH', 'SOL')
MARKETS = tuple(asset + '-USDT' + suffix for asset in ASSETS for suffix in ('', '-SWAP'))
VERSION = 'cdfp-v0.1-candidate'
BIN = 10_000
WINDOW = 6 * BIN
DAY = 86_400_000
GRACE = 5_000
QUOTE_WAIT = BIN
HORIZONS = (1, 5, 15)
MIN_HISTORY = DAY // BIN * 9 // 10
INSTRUMENTS_URL = 'https://www.okx.com/api/v5/public/instruments'

SCHEMA = """
CREATE TABLE samples (
    asset TEXT, factor_version TEXT, factor_time INTEGER, available_time INTEGER NOT NULL,
    cdfp_value REAL, issued_cdfp_value REAL, spot_imbalance REAL, perp_imbalance REAL,
    same_direction_bins INTEGER,
    volume_quality TEXT NOT NULL, market_regime TEXT NOT NULL, quality_status TEXT NOT NULL,
    momentum REAL, abs_return_1m REAL, realized_vol_15m REAL,
    thresholds TEXT NOT NULL, bins TEXT NOT NULL, gap_detected_time INTEGER,
    price_time INTEGER, price_event_time INTEGER, start_price REAL,
    PRIMARY KEY (asset, factor_version, factor_time)
);
CREATE TABLE labels (
    asset TEXT, factor_version TEXT, factor_time INTEGER, minutes INTEGER,
    target_time INTEGER, price_time INTEGER, price_event_time INTEGER,
    log_return REAL, completed_time INTEGER, status TEXT NOT NULL,
    PRIMARY KEY (asset, factor_version, factor_time, minutes),
    FOREIGN KEY (asset, factor_version, factor_time)
        REFERENCES samples (asset, factor_version, factor_time)
);
"""


def utc_ms():
    return time.time_ns() // 1_000_000


def bin_start(ms):
    return ms // BIN * BIN


def imbalance(buy, sell):
    return (buy - sell) / (buy + sell) if buy + sell else 0.0


def common(spot, perp):
    return math.copysign(min(abs(spot), abs(perp)), spot) if spot * perp > 0 else 0.0


async def contract_specs():
    async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=15)) as session:
        async with session.get(INSTRUMENTS_URL, params={'instType': 'SWAP'}) as response:
            response.raise_for_status()
            result = await response.json()
    if result['code'] != '0':
        raise ValueError(f"OKX instruments returned code {result['code']}")
    return {row['instId']: row for row in result['data'] if row['instId'] in MARKETS}


def contract_multipliers(specs):
    multipliers = dict.fromkeys(MARKETS, 1.0)
    for market in MARKETS[1::2]:
        spec = specs[market]
        if (spec['ctType'], spec['ctValCcy'], spec['settleCcy'], spec['state']) != (
            'linear', market.split('-')[0], 'USDT', 'live'
        ):
            raise ValueError(f'Unsupported OKX contract specification for {market}')
        multiplier = float(spec['ctVal']) * float(spec['ctMult'])
        if not math.isfinite(multiplier) or multiplier <= 0:
            raise ValueError(f'Invalid OKX contract size for {market}')
        multipliers[market] = multiplier
    return multipliers


class History:
    def __init__(self):
        self.entries = deque()
        self.sorted = []

    def percentile(self, end, quantile):
        while self.entries and self.entries[0][0] < end - DAY:
            _, expired = self.entries.popleft()
            self.sorted.pop(bisect_left(self.sorted, expired))
        if len(self.sorted) < MIN_HISTORY:
            return None
        return self.sorted[math.ceil(len(self.sorted) * quantile) - 1]

    def add(self, time_ms, value):
        if value is not None:
            self.entries.append((time_ms, value))
            insort(self.sorted, value)

    def invalidate(self, first_end, last_end):
        kept = deque()
        for end, value in self.entries:
            if first_end <= end <= last_end:
                self.sorted.pop(bisect_left(self.sorted, value))
            else:
                kept.append((end, value))
        self.entries = kept


@dataclass
class Bucket:
    buy: float = 0.0
    sell: float = 0.0
    close: tuple | None = None
    healthy: bool = False

    @property
    def volume(self):
        return self.buy + self.sell

    def export(self):
        return {'buy_usdt': self.buy, 'sell_usdt': self.sell,
                'imbalance': imbalance(self.buy, self.sell) if self.healthy else None}


@dataclass
class Market:
    ack: int | None = None
    last_frame: int | None = None
    last_trade: tuple | None = None
    buckets: dict = field(default_factory=dict)
    bad: set = field(default_factory=set)
    volume: History = field(default_factory=History)


@dataclass
class Prediction:
    key: tuple
    available: int
    price: float | None = None
    origin: int | None = None
    next_label: int = 0

    @property
    def due(self):
        if self.origin is None:
            return self.available
        return self.origin + HORIZONS[self.next_label] * 60_000


class Study:
    def __init__(self, db, multipliers, started, clock=utc_ms):
        self.db = db
        self.clock = clock
        self.multipliers = multipliers
        self.end = bin_start(started) + BIN
        self.started = self.end
        self.markets = {name: Market() for name in MARKETS}
        self.prices = {asset: deque(maxlen=91) for asset in ASSETS}
        self.histories = {asset: (History(), History()) for asset in ASSETS}
        self.pending = {asset: [] for asset in ASSETS}
        self.next_due = dict.fromkeys(ASSETS, math.inf)
        self.clock_origin = None
        self.last_utc = started

    def check_clock(self, wall, mono):
        offset = wall - mono
        if wall // 1_000_000 < self.last_utc or (
            self.clock_origin is not None and abs(offset - self.clock_origin) > 500_000_000
        ):
            raise RuntimeError('UTC clock shifted during CDFP capture')
        if self.clock_origin is None:
            self.clock_origin = offset
        self.last_utc = wall // 1_000_000

    def observe(self, kind, message, wall, mono):
        self.check_clock(wall, mono)
        now = wall // 1_000_000
        if kind in ('connect', 'disconnect'):
            for name in message['instruments']:
                market = self.markets[name]
                market.ack = market.last_trade = market.last_frame = None
        elif kind == 'pong':
            for name in message['instruments']:
                self.markets[name].last_frame = now
        elif kind == 'control' or (kind == 'trades-all' and 'event' in message):
            instrument = message.get('arg', {}).get('instId')
            if instrument in self.markets:
                market = self.markets[instrument]
                if message.get('event') == 'subscribe':
                    market.ack = market.last_frame = now
                elif message.get('event') in ('unsubscribe', 'error'):
                    market.ack = market.last_trade = market.last_frame = None
        elif kind == 'trades-all':
            name = message['arg']['instId']
            self.markets[name].last_frame = now
            self.trade(name, message.get('data', ()), now)

    def invalidate(self, name, start, end, now):
        market = self.markets[name]
        if start == end and start in market.bad:
            return
        for stamp in range(max(start, self.end - WINDOW), end + BIN, BIN):
            market.bad.add(stamp)
            if stamp in market.buckets:
                market.buckets[stamp].healthy = False
        if start < self.end - BIN:
            market.volume.invalidate(start + BIN, min(end + BIN, self.end - BIN))
            if not name.endswith('-SWAP'):
                asset = name.split('-')[0]
                self.prices[asset] = deque(
                    ((stamp, None if start < stamp <= end + BIN else price)
                     for stamp, price in self.prices[asset]), maxlen=91
                )
                jumps, volatility = self.histories[asset]
                jumps.invalidate(start + BIN, end + BIN + WINDOW)
                volatility.invalidate(start + BIN, end + BIN + 90 * BIN)
            with self.db:
                self.db.execute(
                    """UPDATE samples SET cdfp_value=NULL, spot_imbalance=NULL, perp_imbalance=NULL,
                       same_direction_bins=NULL, quality_status='GAP', volume_quality='UNKNOWN',
                       gap_detected_time=coalesce(gap_detected_time, ?)
                       WHERE asset=? AND factor_version=? AND factor_time>? AND factor_time<=?""",
                    (now, name.split('-')[0], VERSION, start, end + WINDOW),
                )

    def trade(self, name, data, now):
        market = self.markets[name]
        if market.ack is None:
            return
        try:
            row, = data
            stamp = int(row['ts'])
            ident = int(row['tradeId'])
            price = float(row['px'])
            size = float(row['sz'])
            amount = price * size * self.multipliers[name]
            side = row['side']
            if (row['instId'] != name or ident <= 0 or stamp <= 0 or stamp > now + GRACE
                or side not in ('buy', 'sell') or row['source'] not in ('0', '1')
                or not all(math.isfinite(value) and value > 0 for value in (price, size, amount))):
                raise ValueError('Invalid trade fields')
        except (KeyError, TypeError, ValueError, OverflowError):
            start = bin_start(market.last_trade[1]) if market.last_trade else bin_start(now)
            self.invalidate(name, start, bin_start(now), now)
            return
        start = bin_start(stamp)
        previous = market.last_trade
        if previous and ident == previous[0]:
            return
        if previous and ident != previous[0] + 1:
            earlier = bin_start(previous[1])
            self.invalidate(name, min(start, earlier), max(start, earlier), now)
            if ident < previous[0]:
                return
        market.last_trade = (ident, stamp)
        if start < self.end - BIN or now > start + BIN + GRACE:
            self.invalidate(name, start, start, now)
            return
        bucket = market.buckets.setdefault(start, Bucket())
        if side == 'buy':
            bucket.buy += amount
        else:
            bucket.sell += amount
        if not name.endswith('-SWAP') and start not in market.bad:
            quote = (stamp, ident, price)
            bucket.close = max(bucket.close or quote, quote)
            if now >= stamp:
                self.resolve(name.split('-')[0], now, stamp, price)

    def price_state(self, asset, end, quote):
        prices = self.prices[asset]
        prices.append((end, quote))
        momentum = None
        if len(prices) > 6 and prices[-7][1] and quote:
            momentum = math.log(quote / prices[-7][1])
        values = [value for _, value in prices]
        rv = None
        if len(values) == 91 and values[0] is not None and quote is not None:
            moves = [math.log(right / left) ** 2 for left, right in zip(values, values[1:])
                     if left is not None and right is not None]
            if len(moves) >= 81:
                rv = math.sqrt(math.fsum(moves))
        jump = abs(momentum) if momentum is not None else None
        jumps, volatility = self.histories[asset]
        p99, p95 = jumps.percentile(end, .99), volatility.percentile(end, .95)
        regime = 'WARMUP'
        if end >= self.started + DAY and None not in (jump, rv, p99, p95):
            regime = 'NORMAL' if jump < p99 and rv < p95 else 'EXTREME'
        jumps.add(end, jump)
        volatility.add(end, rv)
        return regime, momentum, jump, rv, p99, p95

    def seal_bins(self, end):
        cutoff = end - WINDOW
        for market in self.markets.values():
            for old in tuple(market.buckets):
                if old < cutoff:
                    del market.buckets[old]
            market.bad = {start for start in market.bad if start >= cutoff}
            start = end - BIN
            bucket = market.buckets.setdefault(start, Bucket())
            bucket.healthy = (market.ack is not None and market.ack <= start
                              and market.last_frame is not None and market.last_frame >= end
                              and start not in market.bad)

    def sample(self, asset, end):
        spot, perp = (self.markets[asset + suffix] for suffix in ('-USDT', '-USDT-SWAP'))
        pairs = [(spot.buckets.get(start, Bucket()), perp.buckets.get(start, Bucket()))
                 for start in range(end - WINDOW, end, BIN)]
        healthy = all(left.healthy and right.healthy for left, right in pairs)
        latest_spot, latest_perp = pairs[-1]
        quote = latest_spot.close
        regime, momentum, jump, rv, p99, p95 = self.price_state(
            asset, end, quote[2] if quote and latest_spot.healthy else None
        )
        spot_min, perp_min = (market.volume.percentile(end, .10) for market in (spot, perp))
        volume_quality = 'UNKNOWN'
        score = spot_flow = perp_flow = count = None
        if healthy:
            flows = [(imbalance(left.buy, left.sell), imbalance(right.buy, right.sell))
                     for left, right in pairs]
            spot_flow, perp_flow = (math.fsum(side) / 6 for side in zip(*flows))
            contributions = [common(left, right) for left, right in flows]
            score = math.fsum(contributions) / 6
            count = sum(value != 0 for value in contributions)
            if end >= self.started + DAY and None not in (spot_min, perp_min):
                low = any(left.volume < spot_min or right.volume < perp_min for left, right in pairs)
                volume_quality = 'LOW' if low else 'OK'
        for market, latest in ((spot, latest_spot), (perp, latest_perp)):
            if latest.healthy and latest.volume > 0:
                market.volume.add(end, latest.volume)
        bins = [{'end': end - (5 - index) * BIN, 'spot': left.export(), 'perp': right.export()}
                for index, (left, right) in enumerate(pairs)]
        quality = 'OK' if healthy else ('WARMUP' if end < self.started + WINDOW - BIN else 'GAP')
        thresholds = {'abs_return_p99': p99, 'rv_p95': p95,
                      'spot_volume_p10': spot_min, 'perp_volume_p10': perp_min}
        return (asset, VERSION, end, score, spot_flow, perp_flow, count, volume_quality, regime,
                quality, momentum, jump, rv, json.dumps(thresholds), json.dumps(bins))

    def tick(self, now):
        for asset in ASSETS:
            self.resolve(asset, now)
        while now >= self.end + GRACE:
            end = self.end
            self.seal_bins(end)
            samples = [self.sample(asset, end) for asset in ASSETS]
            available = max(now, self.clock())
            with self.db:
                for asset, version, stamp, *fields in samples:
                    key = (asset, version, stamp)
                    self.db.execute(
                        """INSERT INTO samples (
                           asset, factor_version, factor_time, available_time, cdfp_value,
                           issued_cdfp_value, spot_imbalance, perp_imbalance, same_direction_bins, volume_quality,
                           market_regime, quality_status, momentum, abs_return_1m, realized_vol_15m,
                           thresholds, bins) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                        (*key, available, fields[0], *fields),
                    )
                    status = 'PENDING' if fields[0] is not None else 'FACTOR_UNAVAILABLE'
                    self.db.executemany(
                        """INSERT INTO labels (asset, factor_version, factor_time, minutes,
                           completed_time, status) VALUES (?, ?, ?, ?, ?, ?)""",
                        [(*key, minutes, None if status == 'PENDING' else available, status)
                         for minutes in HORIZONS],
                    )
                    if status == 'PENDING':
                        self.pending[asset].append(Prediction(key, available))
                        self.next_due[asset] = min(self.next_due[asset], available)
            self.end += BIN

    def resolve(self, asset, now, event=None, price=None):
        # Most trade updates arrive between deadlines and need no database work.
        if (event if event is not None else now - QUOTE_WAIT) < self.next_due[asset]:
            return
        remaining = []
        with self.db:
            for item in self.pending[asset]:
                due = item.due
                if item.origin is None:
                    if now > due + QUOTE_WAIT:
                        self.finish_labels(item, now, 'NO_START_PRICE')
                        continue
                    if event is not None and due <= event <= now:
                        item.origin, item.price = now, price
                        self.db.execute(
                            """UPDATE samples SET price_time=?, price_event_time=?, start_price=?
                               WHERE asset=? AND factor_version=? AND factor_time=?""",
                            (now, event, price, *item.key),
                        )
                        self.db.executemany(
                            """UPDATE labels SET target_time=? WHERE asset=? AND factor_version=?
                               AND factor_time=? AND minutes=?""",
                            [(now + minutes * 60_000, *item.key, minutes) for minutes in HORIZONS],
                        )
                else:
                    while item.next_label < len(HORIZONS):
                        due = item.due
                        if now > due + QUOTE_WAIT:
                            self.complete(item, now, 'NO_END_PRICE')
                        elif event is not None and due <= event <= now:
                            self.complete(item, now, 'COMPLETE', event, math.log(price / item.price))
                        else:
                            break
                if item.next_label < len(HORIZONS):
                    remaining.append(item)
        self.pending[asset] = remaining
        self.next_due[asset] = min((item.due for item in remaining), default=math.inf)

    def complete(self, item, now, status, event=None, result=None):
        self.db.execute(
            """UPDATE labels SET price_time=?, price_event_time=?, log_return=?, completed_time=?,
               status=? WHERE asset=? AND factor_version=? AND factor_time=? AND minutes=?""",
            (now if event is not None else None, event, result, max(now, self.clock()), status,
             *item.key, HORIZONS[item.next_label]),
        )
        item.next_label += 1

    def finish_labels(self, item, now, status):
        while item.next_label < len(HORIZONS):
            self.complete(item, now, status)

    def close(self):
        now = self.clock()
        with self.db:
            for items in self.pending.values():
                for item in items:
                    self.finish_labels(item, now, 'CAPTURE_ENDED')


async def run(root, seconds, cap):
    specs = await contract_specs()
    multipliers = contract_multipliers(specs)
    root.mkdir(parents=True, exist_ok=False)
    (root / 'instruments.json').write_text(json.dumps(specs, indent=2), encoding='utf-8')
    db = sqlite3.connect(root / 'cdfp.sqlite')
    try:
        db.execute('PRAGMA journal_mode=WAL')
        db.execute('PRAGMA foreign_keys=ON')
        db.executescript(SCHEMA)
        study = Study(db, multipliers, utc_ms())
        capture = asyncio.create_task(collect(
            root / 'raw', seconds, cap, instruments=MARKETS, channels=('trades-all',),
            observer=study.observe, ws_compression=True, split_streams=True, receive_timeout=3,
        ))
        try:
            while True:
                done, _ = await asyncio.wait((capture,), timeout=.25)
                if done:
                    reason = await capture
                    if reason == 'subscription-error':
                        raise RuntimeError('OKX rejected a CDFP subscription. See the raw archive.')
                    break
                study.check_clock(time.time_ns(), time.monotonic_ns())
                study.tick(study.last_utc)
        finally:
            if not capture.done():
                capture.cancel()
                await asyncio.gather(capture, return_exceptions=True)
            study.close()
    finally:
        db.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--seconds', type=int, default=1200)
    parser.add_argument('--max-gib', type=int, default=2)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 7 * 86_400 or not 1 <= args.max_gib <= 8:
        parser.error('Set a duration of up to 7 days and a raw archive cap of 1 to 8 GiB')
    asyncio.run(run(args.directory, args.seconds, args.max_gib * GIB))
