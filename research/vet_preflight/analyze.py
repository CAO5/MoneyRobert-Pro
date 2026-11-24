"""Compute only VET raw-variable distributions from sealed OKX research data."""

import argparse
import csv
import gzip
import hashlib
import json
import math
import sys
from collections import Counter, defaultdict
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from flow_forward import sealed_rows
from flow_history_replay import Depth
from okx_tardis_audit import archive_path, receipt_ns

MARKETS = ('BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP')
STEP = 10_000
GRACE = 5_000
SILENCE = 10_000
QUANTILES = (.50, .90, .99)


def utc(ms):
    return datetime.fromtimestamp(ms / 1000, UTC).isoformat()


def weighted(counts, fraction):
    remaining = max(1, math.ceil(sum(counts.values()) * fraction))
    for value, weight in sorted(counts.items()):
        remaining -= weight
        if remaining <= 0:
            return value
    return None


def percentiles(counts, unit=1):
    return {f'p{int(100*p)}': (value / unit if (value := weighted(counts, p)) is not None else None)
            for p in QUANTILES}


def ratio(num, denom):
    return num / denom if denom else None


def acceleration(current, reference):
    if reference <= 0:
        return None
    return max(0., math.log(current / reference)) if current > 0 else 0.


class Market:
    def __init__(self, tick, start, end):
        self.tick, self.start, self.end = tick, start, end
        self.events = Counter()
        self.spreads = defaultdict(Counter)
        self.basis_points = defaultdict(Counter)
        self.mids = {}
        self.gaps = {'quotes': set(), 'trades': set()}
        self.delays = {'quotes': Counter(), 'trades': Counter()}
        self.counts = Counter()
        self.last_received = self.last_event = None
        self.first_trade = None
        self.interrupted = {}
        self.trade_end = self.trade_stamp = self.quote = self.sequence = None
        self.depth = None

    def bad(self, stream, start, end, reason):
        self.counts[reason] += 1
        lo = max(self.start, min(start, end) // STEP * STEP)
        hi = min(self.end - STEP, max(start, end) // STEP * STEP)
        self.gaps[stream].update(range(lo, hi + STEP, STEP))

    def outage(self, received, reason, streams=('quotes', 'trades')):
        # End the quote at the last known exchange event in the old connection.
        if 'quotes' in streams:
            self.extend_quote(self.last_event)
            self.quote = self.sequence = self.depth = None
        for stream in streams:
            stamp = self.last_event if self.last_event is not None else received
            self.bad(stream, stamp, received, reason + '_' + stream)
            self.interrupted.setdefault(stream, stamp)
        if 'trades' in streams:
            self.trade_end = self.trade_stamp = None

    def touch(self, received):
        if self.last_received is not None:
            delta = received - self.last_received
            if delta < 0 or delta > SILENCE:
                self.outage(received, 'receipt_clock' if delta < 0 else 'transport_silence')
        self.last_received = received

    def usable(self, stream, event, received):
        if not self.start - STEP <= event < self.end:
            return False
        if self.start <= event < self.end:
            self.delays[stream][received - event] += 1
        too_early = received - event < -1000
        late = received > (event // STEP + 1) * STEP + GRACE
        if too_early or late:
            self.counts[f'{stream}_exchange_clock_ahead' if too_early else f'{stream}_beyond_grace'] += 1
            self.bad(stream, event, event, f'{stream}_late_or_clock')
            return False
        return True

    def extend_quote(self, until):
        if self.quote is None or until is None:
            return
        cursor, tick_count, bp, mid = self.quote
        begin = max(cursor, self.start)
        end = min(until, self.end)
        while begin < end:
            slot = begin // STEP * STEP
            final = min(end, slot + STEP)
            self.spreads[slot][tick_count] += final - begin
            self.basis_points[slot][bp] += final - begin
            for boundary in range((begin // 1000 + 1) * 1000, final + 1, 1000):
                self.mids[boundary] = mid
            begin = final
        if until >= cursor:
            self.quote = (until, tick_count, bp, mid)

    def price(self, event, received, bid, ask):
        if event >= self.end:
            self.extend_quote(self.end)
            self.last_event = max(self.end, self.last_event or self.end)
            return
        if self.quote is not None and event < self.quote[0]:
            self.bad('quotes', event, self.quote[0], 'quote_exchange_time_reversal')
            self.quote = None
            return
        if not self.usable('quotes', event, received) or not 0 < bid < ask:
            if not 0 < bid < ask:
                self.bad('quotes', event, event, 'invalid_best_price')
            self.quote = None
            return
        mid = (bid + ask) / 2
        ticks = (ask - bid) / self.tick
        if any(abs(value - round(value)) > 1e-6 for value in (ticks, bid / self.tick, ask / self.tick)):
            self.bad('quotes', event, event, 'off_tick_grid')
            self.quote = None
            return
        self.extend_quote(event)
        if 'quotes' in self.interrupted:
            self.bad('quotes', self.interrupted.pop('quotes'), event, 'quote_recovery')
        self.quote = (event, round(ticks), round((ask - bid) / mid * 10_000_000), mid)
        self.last_event = max(event, self.last_event or event)

    def book(self, row, received, action, channel):
        event = int(row['ts'])
        if channel == 'bbo-tbt':
            if not row['bids'] or not row['asks']:
                self.bad('quotes', event, event, 'empty_bbo')
                self.quote = None
                return
            self.price(event, received, float(row['bids'][0][0]), float(row['asks'][0][0]))
            return
        previous, current = int(row['prevSeqId']), int(row['seqId'])
        if action == 'snapshot' and previous == -1:
            if self.sequence is not None:
                self.outage(received, 'book_reanchor', ('quotes',))
            self.depth = (Depth(-1), Depth(1))
        elif action != 'update' or self.sequence is None or previous != self.sequence:
            if self.sequence is not None:
                self.outage(received, 'book_sequence_gap', ('quotes',))
            else:
                self.bad('quotes', event, event, 'unanchored_book')
            return
        self.sequence = current
        for side, field in zip(self.depth, ('bids', 'asks')):
            for px, qty, *_ in row[field]:
                side.set(float(px), float(qty))
        self.price(event, received, *(side.best()[0] for side in self.depth))

    def trades(self, rows, received):
        if len(rows) != 1:
            self.bad('trades', received, received, 'unexpected_trade_envelope')
            return
        row, = rows
        event = int(row['ts'])
        if event >= self.end:
            self.last_event = max(self.end, self.last_event or self.end)
            return
        trade_id, count = int(row['tradeId']), int(row['count'])
        if self.start <= event < self.end:
            self.counts['trade_messages'] += 1
            self.counts['trade_count_sum'] += count
            self.counts['messages_with_count_gt_1'] += count > 1
        if count < 1:
            self.bad('trades', event, event, 'invalid_count')
            return
        if self.trade_end is not None and trade_id - count != self.trade_end:
            diff = trade_id - count - self.trade_end
            self.bad('trades', self.trade_stamp, event, 'trade_id_discontinuity')
            self.counts['missing_trade_ids'] += max(0, diff)
            if diff < 0:
                return
        if self.trade_stamp is not None and event < self.trade_stamp:
            self.bad('trades', event, self.trade_stamp, 'trade_exchange_time_reversal')
        self.trade_end, self.trade_stamp = trade_id, event
        if self.usable('trades', event, received):
            if 'trades' in self.interrupted:
                self.bad('trades', self.interrupted.pop('trades'), event, 'trade_recovery')
            if self.first_trade is None:
                self.first_trade = event
            self.events[event // STEP * STEP] += 1
            self.last_event = max(event, self.last_event or event)

    def ingest(self, packet, channel, received):
        self.touch(received)
        event = packet.get('event')
        if event in ('subscribe', 'unsubscribe'):
            stream = 'trades' if channel == 'trades' else 'quotes'
            self.outage(received, 'subscription_' + event, (stream,))
        elif 'data' in packet:
            if channel == 'trades':
                self.trades(packet['data'], received)
            else:
                for row in packet['data']:
                    self.book(row, received, packet.get('action'), channel)

    def summary(self):
        self.extend_quote(self.last_event)
        slots = list(range(self.start, self.end, STEP))
        quote_bad = {slot for slot in slots if slot in self.gaps['quotes']
                     or sum(self.spreads.get(slot, {}).values()) != STEP}
        trade_bad = self.gaps['trades'] | {slot for slot in slots
                                        if self.last_event is None or slot + STEP > self.last_event
                                        or self.first_trade is None or slot < self.first_trade
                                        or ('trades' in self.interrupted and slot >=
                                            self.interrupted['trades'] // STEP * STEP)}
        all_spreads, all_bp = Counter(), Counter()
        for slot in slots:
            if slot not in quote_bad:
                all_spreads.update(self.spreads[slot])
                all_bp.update(self.basis_points[slot])
        rv = self.volatility(quote_bad)
        observations = []
        a_values, l_values = [], []
        recent_events = Counter()
        reference = Counter()
        for index in range(66, len(slots) + 1):
            stamp = self.start + index * STEP
            trade_slots = slots[index - 66:index]
            quote_slots = slots[index - 63:index]
            a = l = None
            if not any(slot in trade_bad for slot in trade_slots):
                n = sum(self.events[slot] for slot in trade_slots[-6:])
                a = acceleration(n, sum(self.events[slot] for slot in trade_slots[:-6]) / 10)
                recent_events[n] += 1
                if a is not None:
                    a_values.append(a)
            if not any(slot in quote_bad for slot in quote_slots):
                for slot in quote_slots[:-3]:
                    reference.update(self.spreads[slot])
                current = self.spreads[quote_slots[-3]] + self.spreads[quote_slots[-2]] + self.spreads[quote_slots[-1]]
                l = acceleration(weighted(current, .5), weighted(reference, .5))
                reference.clear()
                l_values.append(l)
            if a is not None and l is not None:
                observations.append((a, l, a * l, stamp))
        values = Counter(v for _, _, v, _ in observations if v > 0)
        positives = sum(values.values())
        joint_a = sum(a > 0 for a, _, _, _ in observations)
        joint_l = sum(l > 0 for _, l, _, _ in observations)
        elevated = weighted(values, .90)
        vol = {}
        for minutes, points in rv.items():
            baseline = Counter(points[stamp] for _, _, _, stamp in observations if stamp in points)
            cutoff = weighted(baseline, .90)
            def compare(predicate):
                selected = Counter(points[stamp] for _, _, v, stamp in observations
                                   if predicate(v) and stamp in points)
                n = sum(selected.values())
                return {'n': n, 'percentiles_bp': percentiles(selected),
                        'above_baseline_p90_fraction': ratio(
                            sum(count for value, count in selected.items() if cutoff is not None and value > cutoff), n)}
            vol[str(minutes)] = {'baseline_n': sum(baseline.values()),
                                 'baseline_percentiles_bp': percentiles(baseline),
                                 'vet_positive': compare(lambda v: v > 0),
                                 'vet_top_nonzero_decile': compare(lambda v: elevated is not None and v >= elevated)}
        trade_messages = self.counts['trade_messages']
        return {
            'tick_size': self.tick, 'expected_10s_bins': len(slots),
            'quote_missing_10s_bins': len(quote_bad),
            'trade_missing_10s_bins': sum(slot in trade_bad for slot in slots),
            'spread_tick_duration_ms': dict(sorted(all_spreads.items())),
            'one_tick_fraction': ratio(all_spreads[1], sum(all_spreads.values())),
            'spread_bp': percentiles(all_bp, 1000),
            'a_valid': len(a_values), 'l_valid': len(l_values), 'joint_valid': len(observations),
            'trade_events_per_60s': percentiles(recent_events),
            'a_positive_fraction': ratio(sum(a > 0 for a in a_values), len(a_values)),
            'l_positive_fraction': ratio(sum(l > 0 for l in l_values), len(l_values)),
            'vet_positive_fraction': ratio(positives, len(observations)),
            'joint_a_positive_fraction': ratio(joint_a, len(observations)),
            'joint_l_positive_fraction': ratio(joint_l, len(observations)),
            'vet_nonzero_n': positives, 'vet_nonzero': percentiles(values),
            'trade_messages': trade_messages, 'trade_count_sum': self.counts['trade_count_sum'],
            'aggregated_message_fraction': ratio(self.counts['messages_with_count_gt_1'], trade_messages),
            'mean_fills_per_message': ratio(self.counts['trade_count_sum'], trade_messages),
            'delay_ms': {name: {**percentiles(values), 'min': min(values, default=None),
                                'max': max(values, default=None), 'n': sum(values.values()),
                                'negative_delay_n': sum(count for delay, count in values.items() if delay < 0)}
                         for name, values in self.delays.items()},
            'quality_counters': dict(self.counts), 'past_realized_volatility': vol,
        }

    def volatility(self, quote_bad):
        variance, invalid = [0.], [0]
        times = range(self.start + 1000, self.end + 1, 1000)
        for stamp in times:
            before, after = self.mids.get(stamp - 1000), self.mids.get(stamp)
            valid = before is not None and after is not None and all(
                (t - 1) // STEP * STEP not in quote_bad for t in (stamp - 1000, stamp))
            variance.append(variance[-1] + ((math.log(after) - math.log(before)) ** 2 if valid else 0))
            invalid.append(invalid[-1] + (not valid))
        result = {}
        for minutes in (1, 5, 30):
            span = minutes * 60
            result[minutes] = {self.start + n * 1000: 10_000 * math.sqrt(variance[n] - variance[n - span])
                               for n in range(span, len(variance), 10) if invalid[n] == invalid[n - span]}
        return result


def history(root, sol_root, start, end, ticks):
    sources = [(root, set(MARKETS[:2]))]
    if sol_root and sol_root.is_dir():
        sources.append((sol_root, {MARKETS[2]}))
    output, source_digests = {}, {}
    for directory, assets in sources:
        files = [archive_path(directory, minute) for minute in range(0, (end - start) // 60_000, 10)]
        if not all(path.is_file() for path in files):
            continue
        markets = {name: Market(ticks[name], start, end) for name in sorted(assets)}
        last = start
        source_digests[str(directory)] = {}
        for index, path in enumerate(files):
            with path.open('rb') as raw:
                source_digests[str(directory)][path.name] = hashlib.file_digest(raw, 'sha256').hexdigest()
            with gzip.open(path, 'rb') as tape:
                for line in tape:
                    if not line.strip():
                        for market in markets.values():
                            market.outage(last, 'provider_disconnect')
                        continue
                    received, payload = line.split(b' ', 1)
                    received = receipt_ns(received) // 1_000_000
                    lower = start + index * 600_000
                    if not lower <= received < lower + 600_000 or received < last:
                        raise ValueError(f'Unordered receiver timestamp in {path}')
                    last = received
                    packet = json.loads(payload)
                    argument = packet.get('arg', {})
                    name, channel = argument.get('instId'), argument.get('channel')
                    if name in markets and channel in ('books', 'trades'):
                        markets[name].ingest(packet, channel, received)
        output.update({name: market.summary() for name, market in markets.items()})
    return {'source': 'Tardis OKX native books + trades', 'receiver': 'Tardis UTC receiver clock',
            'utc_start': utc(start), 'utc_end': utc(end), 'window_seconds': (end - start) / 1000,
            'slice_sha256': source_digests, 'markets': output}


def live(directory, limit=None):
    raw = directory / 'raw'
    with (raw / 'manifest.csv').open(newline='', encoding='utf-8') as catalog:
        shards = list(csv.DictReader(catalog))
    if not shards:
        return {'state': 'AWAITING_SEALED_SEGMENT', 'markets': {}}
    start = (int(shards[0]['first_utc_ns']) // 1_000_000 // STEP + 1) * STEP
    last = int(shards[-1]['last_utc_ns']) // 1_000_000
    end = (last - GRACE) // STEP * STEP
    if limit:
        end = min(end, start + limit)
    specs = json.loads((directory / 'instruments.json').read_text(encoding='utf-8'))
    markets = {name: Market(float(specs[name]['tickSz']), start, end) for name in MARKETS}
    previous = None
    for wall, mono, venue, kind, payload in sealed_rows(raw):
        received = wall // 1_000_000
        if received > end + GRACE:
            break
        scope = payload.get('instruments', ()) if isinstance(payload, dict) else ()
        if previous is not None and (mono < previous[1] or abs(wall - previous[0] - mono + previous[1]) > 100_000_000):
            for market in markets.values():
                market.outage(received, 'local_clock_step')
        previous = (wall, mono)
        if kind in ('connect', 'disconnect', 'sequence_gap', 'pong'):
            for name in scope:
                if kind != 'pong':
                    markets[name].outage(received, kind)
                markets[name].touch(received)
        elif venue == 'okx' and kind in ('bbo-tbt', 'trades'):
            packet = json.loads(payload)
            name = packet.get('arg', {}).get('instId')
            if name in markets:
                markets[name].ingest(packet, kind, received)
    summary_path = raw / 'summary.json'
    stop_reason = json.loads(summary_path.read_text(encoding='utf-8'))['reason'] if summary_path.is_file() else None
    return {'source': 'OKX direct bbo-tbt + trades', 'receiver': 'local UTC and monotonic clock',
            'utc_start': utc(start), 'utc_end': utc(max(start, end)),
            'window_seconds': max(0, end - start) / 1000, 'raw_archive': str(directory),
            'capture_stop_reason': stop_reason,
            'sealed_segments_at_read': len(shards), 'markets': {name: market.summary() for name, market in markets.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--historical', type=Path)
    parser.add_argument('--sol-historical', type=Path)
    parser.add_argument('--historical-hours', type=int, default=2, choices=(2, 24))
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    sources = {}
    if args.historical:
        specs = json.loads((args.archive / 'instruments.json').read_text(encoding='utf-8'))
        ticks = {name: float(specs[name]['tickSz']) for name in MARKETS}
        start = int(datetime(2026, 11, 1, tzinfo=UTC).timestamp() * 1000)
        sources['historical'] = history(args.historical, args.sol_historical, start,
                                        start + args.historical_hours * 3_600_000, ticks)
    if args.archive:
        sources['direct_first_two_hours'] = live(args.archive, 7_200_000)
        if sources['direct_first_two_hours'].get('window_seconds') == 7200:
            sources['direct_all_sealed'] = live(args.archive)
    result = {'factor': 'VET v0.1 raw data preflight', 'factor_step_seconds': 10,
              'receipt_grace_seconds': 5, 'historical_tick_unit_reference': 'live REST snapshot; historical grid checked',
              'sources': sources}
    temporary = args.output.with_suffix('.tmp')
    temporary.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    temporary.replace(args.output)


if __name__ == '__main__':
    main()
