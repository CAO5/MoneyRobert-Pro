"""Confirmed DEX candle signals with spot-only execution; no wallet or on-chain fills."""
import csv
import hashlib
import json
import math
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import defaultdict, deque
from datetime import date
from pathlib import Path
from statistics import median

from independent_round import book
from participant_traces import replay


ROOT = Path(__file__).parent
CACHE = Path(tempfile.gettempdir()) / 'doge-pool-screen'
DOGE = '0xba2ae424d960c26247dd6c32edc70b295c744c43'
POOLS = {'usdt': ('0x0fa119e6a12e3540c2412f9eda0221ffd16a7934', '0x55d398326f99059ff775485246999027b3197955'),
         'wbnb': ('0xac109c8025f272414fd9e2faa805a583708a017f', '0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c')}
DAYS = tuple(f'2026-{month:02}-01' for month in range(6, 12))
RULES = ('both_rich', 'both_cheap', 'active_rise', 'active_fall')


def midnight(day):
    return (date.fromisoformat(day) - date(1970, 1, 1)).days * 86400


def candles(pool, day):
    CACHE.mkdir(exist_ok=True)
    address, counter = POOLS[pool]
    origin = midnight(day)
    end = origin + 86400 - 1
    result, proofs = {}, []
    while end >= origin:
        dest = CACHE / f'{pool}-{day}-{end}.json'
        if not dest.exists():
            params = urllib.parse.urlencode(dict(aggregate=1, before_timestamp=end, limit=1000,
                                                 currency='token', token=DOGE, include_empty_intervals='false'))
            url = f'https://api.geckoterminal.com/api/v2/networks/bsc/pools/{address}/ohlcv/minute?{params}'
            for attempt in range(3):
                try:
                    with urllib.request.urlopen(url, timeout=40) as response:
                        raw = response.read()
                    break
                except urllib.error.HTTPError as error:
                    if error.code != 429 or attempt == 2:
                        raise
                    retry = error.headers.get('Retry-After', '')
                    time.sleep(max(65 * (attempt + 1), int(retry) if retry.isdigit() else 0))
            time.sleep(12)
        else:
            raw = dest.read_bytes()
        payload = json.loads(raw)
        meta = payload['meta']
        if meta['base']['address'].lower() != DOGE or meta['quote']['address'].lower() != counter:
            raise ValueError('Wrong pool units')
        bars = payload['data']['attributes']['ohlcv_list']
        stamps = [row[0] for row in bars]
        if not stamps or max(stamps) > end or any(new >= old for old, new in zip(stamps, stamps[1:])):
            raise ValueError('Unknown or invalid historical page')
        for stamp, opening, high, low, close, volume in bars:
            if not all(math.isfinite(value) for value in (opening, high, low, close, volume)) or stamp % 60 or not 0 < low <= min(opening, close) <= max(opening, close) <= high or volume < 0:
                raise ValueError('Invalid indexed candle')
            if stamp >= origin and volume > 0:
                result[stamp - origin] = (opening, close, volume)
        if not dest.exists():
            dest.write_bytes(raw)
        proofs.append((dest.name, hashlib.sha256(raw).hexdigest(), len(bars)))
        end = min(stamps) - 1
        if len(bars) < 1000:
            break
    return result, proofs


def features(direct, wrapped, spot, bridge):
    signals = defaultdict(list)
    histories = [deque(), deque()]
    matched = fresh = active = 0
    for minute in range(0, len(spot) - 840, 60):
        last, decision = minute + 59, minute + 179
        pair = (direct.get(minute), wrapped.get(minute))
        for history in histories:
            while history and history[0][0] < minute - 1800:
                history.popleft()
        if all(pair):
            matched += 1
            local, then, conversion = spot[decision], spot[last], bridge[last]
            if local and then and conversion:
                fresh += 1
                direct_close, native_close = pair[0][1], pair[1][1]
                premium = min(direct_close, native_close * conversion[1]) / max(local[2], then[2]) - 1
                discount = max(direct_close, native_close * conversion[2]) / min(local[1], then[1]) - 1
                if premium >= .006 or discount <= -.006:
                    rule = 'both_rich' if premium >= .006 else 'both_cheap'
                    signals[rule].append((decision, 600, premium, discount, 0.))
                if all(len(history) >= 6 for history in histories):
                    active += 1
                    multiple = max(bar[2] / median([size for _, size in history]) for bar, history in zip(pair, histories))
                    changes = [bar[1] / bar[0] - 1 for bar in pair]
                    if multiple >= 4 and (min(changes) >= .0035 or max(changes) <= -.0035):
                        rule = 'active_rise' if min(changes) >= .0035 else 'active_fall'
                        signals[rule].append((decision, 600, *changes, multiple))
        for bar, history in zip(pair, histories):
            if bar:
                history.append((minute, bar[2]))
    return signals, (len(direct), len(wrapped), matched, fresh, active)


def main():
    with (ROOT / 'wrapped_flow_results.csv').open('w', newline='') as results, (ROOT / 'wrapped_flow_trades.csv').open('w', newline='') as trades, (ROOT / 'wrapped_flow_inputs.csv').open('w', newline='') as inputs:
        writer, journal, raw = csv.writer(results), csv.writer(trades), csv.writer(inputs)
        writer.writerow(('day', 'rule', 'fee', 'direct_bars', 'wrapped_bars', 'same_minute', 'fresh_reference', 'with_volume_history', 'raw_signals', 'signal_seconds', 'attempts', 'misses', 'trades', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('day', 'rule', 'fee', 'signal_second', 'hold', 'measure_a', 'measure_b', 'volume_multiple', 'entry_second', 'exit_second', 'qty', 'debit', 'credit', 'net_usdt'))
        raw.writerow(('page', 'sha256', 'rows_including_lookback'))
        for day in DAYS:
            direct, first = candles('usdt', day)
            wrapped, second = candles('wbnb', day)
            raw.writerows(first + second)
            spot = book(day, 'DOGEUSDT')
            signals, coverage = features(direct, wrapped, spot, book(day, 'BNBUSDT'))
            for rule in RULES:
                for fee in (.001, .002):
                    result, ledger = replay(spot, signals[rule], fee)
                    writer.writerow((day, rule, fee, *coverage, *result))
                    journal.writerows((day, rule, fee, *row) for row in ledger)
            print(day, coverage, flush=True)


if __name__ == '__main__':
    main()
