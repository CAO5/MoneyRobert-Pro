"""Read-only VET depth candidate replay using the existing OKX native tape."""

import argparse
import gzip
import hashlib
import json
import math
from bisect import bisect_left, bisect_right, insort
from collections import Counter, defaultdict
from datetime import UTC, datetime
from pathlib import Path

from analyze import GRACE, MARKETS, STEP, Market, acceleration, percentiles, ratio, utc, weighted
from okx_tardis_audit import archive_path, receipt_ns

BAND = 2 / 10_000
LEVELS = 400
DEPTH_FIELDS = ('bid_usdt', 'ask_usdt', 'total_usdt')


class Side:
    def __init__(self, sign):
        self.sign = sign
        self.keys = []
        self.sizes = {}

    def apply(self, levels):
        for raw_price, raw_size, *_ in levels:
            price, size = float(raw_price), float(raw_size)
            if not (math.isfinite(price) and price > 0 and math.isfinite(size) and size >= 0):
                raise ValueError('Invalid book level')
            key = price * self.sign
            if size == 0:
                if key in self.sizes:
                    del self.sizes[key]
                    self.keys.pop(bisect_left(self.keys, key))
            else:
                if key not in self.sizes:
                    insort(self.keys, key)
                self.sizes[key] = size
        for key in self.keys[LEVELS:]:
            del self.sizes[key]
        del self.keys[LEVELS:]

    @property
    def best(self):
        return self.keys[0] * self.sign if self.keys else 0.

    def band(self, edge, multiplier):
        edge = math.nextafter(edge * self.sign, math.inf)
        outside = self.keys and self.keys[-1] >= math.nextafter(edge, -math.inf)
        worst = self.keys[-1] * self.sign if self.keys else None
        inside = self.keys[:bisect_right(self.keys, edge)]
        notional = math.fsum(key * self.sign * self.sizes[key] * multiplier for key in inside) if outside else None
        return notional, worst, len(self.keys)


def medians(histograms):
    return dict(zip(DEPTH_FIELDS, (weighted(values, .5) for values in histograms)))


class DepthMarket(Market):
    def __init__(self, name, spec, start, end):
        asset = name.split('-')[0]
        if (spec['ctType'], spec['ctValCcy'], spec['settleCcy']) != ('linear', asset, 'USDT'):
            raise ValueError(f'Expected a base-denominated linear contract for {name}')
        super().__init__(float(spec['tickSz']), start, end)
        self.name = name
        self.multiplier = float(spec['ctVal']) * float(spec['ctMult'])
        if not (math.isfinite(self.multiplier) and self.multiplier > 0):
            raise ValueError(f'Invalid contract multiplier for {name}')
        self.book_state = None
        self.depth_weights = defaultdict(lambda: [Counter(), Counter(), Counter()])
        self.band_coverage = defaultdict(Counter)
        self.last_in_bin = {}
        self.quote_faults = defaultdict(set)

    def bad(self, stream, start, end, reason):
        super().bad(stream, start, end, reason)
        if stream == 'quotes':
            lo = max(self.start, min(start, end) // STEP * STEP)
            hi = min(self.end - STEP, max(start, end) // STEP * STEP)
            for slot in range(lo, hi + STEP, STEP):
                self.quote_faults[slot].add(reason)

    def outage(self, received, reason, streams=('quotes', 'trades')):
        super().outage(received, reason, streams)
        if 'quotes' in streams:
            self.book_state = None

    def extend_quote(self, until):
        if self.quote is not None and self.book_state is not None and until is not None:
            cursor = max(self.quote[0], self.start)
            end = min(until, self.end)
            state = self.book_state
            while cursor < end:
                slot = cursor // STEP * STEP
                stop = min(end, slot + STEP)
                self.last_in_bin[slot] = state
                self.band_coverage[slot][state['coverage']] += stop - cursor
                if state['coverage'] == 'BOTH':
                    for target, field in zip(self.depth_weights[slot], DEPTH_FIELDS):
                        target[state[field]] += stop - cursor
                cursor = stop
        super().extend_quote(until)

    def book(self, row, received, action, channel):
        event = int(row['ts'])
        previous, current = int(row['prevSeqId']), int(row['seqId'])
        self.counts['book_messages'] += 1
        if action == 'snapshot' and previous == -1:
            if self.sequence is not None:
                self.outage(received, 'book_reanchor', ('quotes',))
            self.depth = (Side(-1), Side(1))
            self.counts['book_snapshots'] += 1
        elif action != 'update' or self.sequence is None or previous != self.sequence:
            if self.sequence is not None:
                self.outage(received, 'book_sequence_gap', ('quotes',))
            else:
                self.bad('quotes', event, event, 'unanchored_book')
            return
        elif current == previous:
            if row['bids'] or row['asks']:
                self.outage(received, 'same_sequence_changed_levels', ('quotes',))
                return
            self.counts['book_heartbeats'] += 1
        elif current < previous:
            # OKX documents linked sequence resets during maintenance.
            self.counts['linked_sequence_resets'] += 1
        self.sequence = current
        bid, ask = self.depth
        try:
            bid.apply(row['bids'])
            ask.apply(row['asks'])
        except (TypeError, ValueError, OverflowError):
            self.outage(received, 'invalid_book_level', ('quotes',))
            return
        best_bid, best_ask = bid.best, ask.best
        self.price(event, received, best_bid, best_ask)
        if self.quote is None or event >= self.end:
            self.book_state = None
            return
        mid = (best_bid + best_ask) / 2
        bid_depth, deepest_bid, bid_levels = bid.band(mid * (1 - BAND), self.multiplier)
        ask_depth, deepest_ask, ask_levels = ask.band(mid * (1 + BAND), self.multiplier)
        if bid_depth is not None and ask_depth is not None:
            coverage = 'BOTH'
            total = bid_depth + ask_depth
        else:
            coverage = 'BID_SHORT' if ask_depth is not None else 'ASK_SHORT' if bid_depth is not None else 'BOTH_SHORT'
            total = None
            self.counts['snapshots_with_insufficient_2bp'] += 1
        self.book_state = {
            'quote_event_ms': event, 'quote_received_ms': received, 'seq_id': current,
            'prev_seq_id': previous, 'sequence_anchored': True, 'coverage': coverage,
            'best_bid': best_bid, 'best_ask': best_ask, 'mid': mid,
            'deepest_bid': deepest_bid, 'deepest_ask': deepest_ask,
            'bid_levels': bid_levels, 'ask_levels': ask_levels,
            'bid_usdt': bid_depth, 'ask_usdt': ask_depth, 'total_usdt': total,
        }

    def assess(self, out):
        self.extend_quote(self.last_event)
        slots = list(range(self.start, self.end, STEP))
        quote_bad = {slot for slot in slots if slot in self.gaps['quotes']
                     or sum(self.band_coverage.get(slot, {}).values()) != STEP}
        coverage_bad = {slot for slot in slots if slot not in quote_bad
                        and self.band_coverage[slot]['BOTH'] != STEP}
        depth_bad = quote_bad | coverage_bad
        trade_bad = self.gaps['trades'] | {slot for slot in slots if
                   self.last_event is None or slot + STEP > self.last_event
                   or self.first_trade is None or slot < self.first_trade
                   or ('trades' in self.interrupted and slot >= self.interrupted['trades'] // STEP * STEP)}
        duration_coverage = Counter()
        depth = [Counter(), Counter(), Counter()]
        for slot in slots:
            if slot not in quote_bad:
                duration_coverage.update(self.band_coverage[slot])
            if slot not in depth_bad:
                for aggregate, weights in zip(depth, self.depth_weights[slot]):
                    aggregate.update(weights)

        empty = dict.fromkeys(DEPTH_FIELDS)
        zero_windows = 0
        bilateral_positive = 0
        scores = []
        recent, base, contractions, a_scores = Counter(), Counter(), Counter(), Counter()
        for index, slot in enumerate(slots, 1):
            end = slot + STEP
            a = l = candidate = None
            recent_depth = base_depth = empty
            current_depth_slots = slots[max(0, index - 3):index]
            base_depth_slots = slots[max(0, index - 63):max(0, index - 3)]
            recent_usable = len(current_depth_slots) == 3 and not any(s in depth_bad for s in current_depth_slots)
            base_usable = len(base_depth_slots) == 60 and not any(s in depth_bad for s in base_depth_slots)
            if recent_usable:
                recent_depth = self.window_depth(current_depth_slots)
            if base_usable:
                base_depth = self.window_depth(base_depth_slots)
            if index >= 66 and not any(s in trade_bad for s in slots[index - 66:index]):
                count = sum(self.events[s] for s in slots[index - 6:index])
                a = acceleration(count, sum(self.events[s] for s in slots[index - 66:index - 6]) / 10)
                if a is not None:
                    a_scores[a] += 1
            if index >= 66 and recent_usable and base_usable:
                d_recent, d_base = recent_depth['total_usdt'], base_depth['total_usdt']
                if min(d_recent, d_base) > 0:
                    l = max(0., math.log(d_base / d_recent))
                    if l > 0:
                        bilateral_positive += all(recent_depth[side] < base_depth[side] for side in DEPTH_FIELDS[:2])
                    recent[d_recent] += 1
                    base[d_base] += 1
                    contractions[l] += 1
                else:
                    zero_windows += 1
            if a is not None and l is not None:
                candidate = a * l
                scores.append((end, a, l, candidate))
            out.write(json.dumps({
                'market': self.name, 'factor_time_ms': end, 'receipt_cutoff_ms': end + GRACE,
                'last_usable_quote': self.last_in_bin.get(slot), 'quote_sequence_clean_10s': slot not in quote_bad,
                'quote_faults_10s': sorted(self.quote_faults.get(slot, ())),
                'coverage_duration_ms_10s': self.band_coverage.get(slot, {}),
                'band_usable_10s': slot not in depth_bad, 'trades_usable_10s': slot not in trade_bad,
                'aggregate_trade_messages_10s': self.events[slot] if slot not in trade_bad else None,
                'recent_depth_median_usdt': recent_depth, 'prior_depth_median_usdt': base_depth,
                'recent_depth_window_usable': recent_usable, 'prior_depth_window_usable': base_usable,
                'A': a, 'L_depth': l, 'VET_v0.2_candidate': candidate,
            }, ensure_ascii=True, allow_nan=False) + '\n')

        nonzero = Counter(score for _, _, _, score in scores if score > 0)
        conditional = self.conditioned_vol(self.volatility(quote_bad), scores, weighted(nonzero, .9))
        positive_l = Counter({value: n for value, n in contractions.items() if value > 0})
        by_hour = []
        for start in range(self.start, self.end, 3_600_000):
            hour = [(l, score) for stamp, _, l, score in scores if start < stamp <= start + 3_600_000]
            by_hour.append({'utc_start': utc(start), 'joint_valid': len(hour),
                            'l_positive_fraction': ratio(sum(l > 0 for l, _ in hour), len(hour)),
                            'candidate_positive_fraction': ratio(sum(score > 0 for _, score in hour), len(hour))})
        return {
            'tick_size': self.tick, 'base_units_per_contract': self.multiplier, 'expected_bins_10s': len(slots),
            'quote_invalid_or_missing_bins_10s': len(quote_bad),
            'insufficient_band_only_bins_10s': len(coverage_bad), 'depth_invalid_bins_10s': len(depth_bad),
            'trade_invalid_or_missing_bins_10s': sum(slot in trade_bad for slot in slots),
            'band_coverage_duration_ms_given_healthy_quote': dict(duration_coverage),
            'fully_covered_time_fraction_given_healthy_quote': ratio(duration_coverage['BOTH'], sum(duration_coverage.values())),
            'time_weighted_depth_usdt': dict(zip(DEPTH_FIELDS, (percentiles(values) for values in depth))),
            'recent_30s_median_total_usdt': percentiles(recent), 'prior_10m_median_total_usdt': percentiles(base),
            'nonpositive_total_depth_windows': zero_windows, 'a_valid': sum(a_scores.values()),
            'a_positive_fraction': ratio(sum(n for a, n in a_scores.items() if a > 0), sum(a_scores.values())),
            'l_valid': sum(contractions.values()), 'l_positive_n': sum(positive_l.values()),
            'l_positive_fraction': ratio(sum(positive_l.values()), sum(contractions.values())),
            'l_depth': percentiles(contractions), 'l_depth_nonzero': percentiles(positive_l),
            'fraction_of_reference_depth_lost_when_positive': {
                key: 1 - math.exp(-value) if value is not None else None for key, value in percentiles(positive_l).items()
            },
            'joint_valid': len(scores), 'candidate_positive_n': sum(nonzero.values()),
            'candidate_positive_fraction': ratio(sum(nonzero.values()), len(scores)),
            'candidate_nonzero': percentiles(nonzero),
            'both_side_window_medians_contract_fraction_given_l_positive': ratio(bilateral_positive, sum(positive_l.values())),
            'quality_counters': dict(self.counts), 'hourly_joint': by_hour,
            'past_realized_volatility_ending_before_contraction_window': conditional,
        }

    def window_depth(self, slots):
        weights = [Counter(), Counter(), Counter()]
        for slot in slots:
            for aggregate, values in zip(weights, self.depth_weights[slot]):
                aggregate.update(values)
        return medians(weights)

    @staticmethod
    def conditioned_vol(volatility, scores, high_cut):
        result = {}
        for minutes, vol in volatility.items():
            values = [(stamp, l, candidate, vol[stamp - 30_000]) for stamp, _, l, candidate in scores
                      if stamp - 30_000 in vol]
            baseline = Counter(rv for _, _, _, rv in values)
            tail = weighted(baseline, .9)

            def describe(predicate):
                selected = [(stamp, rv) for stamp, l, candidate, rv in values if predicate(l, candidate)]
                comparable = [(rv, vol[stamp - 30_000 - minutes * 60_000]) for stamp, rv in selected
                              if stamp - 30_000 - minutes * 60_000 in vol]
                return {'n': len(selected), 'volatility_bp': percentiles(Counter(rv for _, rv in selected)),
                        'above_joint_baseline_p90_fraction': ratio(sum(rv > tail for _, rv in selected), len(selected)),
                        'paired_comparison_n': len(comparable),
                        'above_previous_equal_window_fraction': ratio(sum(rv > earlier for rv, earlier in comparable),
                                                                       len(comparable))}
            result[str(minutes)] = {
                'joint_baseline': describe(lambda l, candidate: True),
                'depth_shrinks': describe(lambda l, candidate: l > 0),
                'candidate_positive': describe(lambda l, candidate: candidate > 0),
                'candidate_top_positive_decile': describe(lambda l, candidate: high_cut is not None and candidate >= high_cut),
            }
        return result


def replay(directory, markets, start, end):
    digests = {}
    last = start
    for minute in range(0, (end - start) // 60_000, 10):
        path = archive_path(directory, minute)
        with path.open('rb') as raw:
            digests[path.name] = hashlib.file_digest(raw, 'sha256').hexdigest()
        lower = start + minute * 60_000
        with gzip.open(path, 'rb') as tape:
            for line in tape:
                if not line.strip():
                    for market in markets.values():
                        market.outage(last, 'provider_disconnect')
                    continue
                stamp, payload = line.split(b' ', 1)
                received = receipt_ns(stamp) // 1_000_000
                if not lower <= received < lower + 600_000 or received < last:
                    raise ValueError(f'Invalid receipt ordering or bounds in {path}')
                last = received
                packet = json.loads(payload)
                arg = packet.get('arg', {})
                name, channel = arg.get('instId'), arg.get('channel')
                if name in markets and channel in ('books', 'trades'):
                    markets[name].ingest(packet, channel, received)
    return digests


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--historical', type=Path, required=True)
    parser.add_argument('--sol-historical', type=Path, required=True)
    parser.add_argument('--specs', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--diagnostics', type=Path, required=True)
    args = parser.parse_args()
    specs = json.loads(args.specs.read_text(encoding='utf-8'))
    start = int(datetime(2026, 11, 1, tzinfo=UTC).timestamp() * 1000)
    end = start + 7_200_000
    markets = {name: DepthMarket(name, specs[name], start, end) for name in MARKETS}
    hashes = {}
    for directory, names in ((args.historical, MARKETS[:2]), (args.sol_historical, MARKETS[2:])):
        hashes[str(directory)] = replay(directory, {name: markets[name] for name in names}, start, end)
    tmp = args.diagnostics.with_suffix('.tmp')
    with gzip.open(tmp, 'wt', encoding='utf-8', compresslevel=6) as diag:
        reports = {name: market.assess(diag) for name, market in markets.items()}
    tmp.replace(args.diagnostics)
    result = {'study': 'VET v0.2 diagnostic candidate', 'source': 'Tardis native OKX books and trades',
              'utc_start': utc(start), 'utc_end': utc(end), 'window_seconds': (end - start) / 1000,
              'band_bp_each_side': 2, 'grid_seconds': 10, 'receipt_grace_seconds': 5,
              'depth_windows_seconds': {'current': 30, 'prior_nonoverlapping': 600},
              'slice_sha256': hashes, 'specifications_file': str(args.specs), 'markets': reports}
    tmp = args.output.with_suffix('.tmp')
    tmp.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    tmp.replace(args.output)


if __name__ == '__main__':
    main()
