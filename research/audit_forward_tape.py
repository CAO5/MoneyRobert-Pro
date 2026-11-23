"""Integrity and timing diagnostics only. Does not display market levels or test alpha."""
import argparse
import csv
import gzip
import hashlib
import json
from collections import Counter
from pathlib import Path


FRESH_NS = 2_000_000_000
BOOK_TOPICS = {'binance': 'dogeusdt@bookTicker', 'bybit': 'orderbook.1.DOGEUSDT'}


class QuoteCoverage:
    def __init__(self):
        self.deadlines = dict.fromkeys(BOOK_TOPICS, 0)
        self.book_ns = Counter()
        self.both_ns = 0
        self.first = self.last = None
        self.offline_since = dict.fromkeys(BOOK_TOPICS)
        self.offline = Counter()
        self.longest_offline = Counter()
        self.offline_ns = Counter()

    def observe(self, when, venue, kind):
        if self.first is None:
            self.first = self.last = when
        for exchange, deadline in self.deadlines.items():
            self.book_ns[exchange] += max(0, min(when, deadline) - self.last)
        self.both_ns += max(0, min(when, *self.deadlines.values()) - self.last)
        self.last = when
        if venue not in BOOK_TOPICS:
            return
        if kind in ('connect', 'disconnect'):
            self.deadlines[venue] = 0
            if kind == 'disconnect' and self.offline_since[venue] is None:
                self.offline_since[venue] = when
                self.offline[venue] += 1
        elif kind == BOOK_TOPICS[venue]:
            self.deadlines[venue] = when + FRESH_NS
            if self.offline_since[venue] is not None:
                duration = when - self.offline_since[venue]
                self.longest_offline[venue] = max(self.longest_offline[venue], duration)
                self.offline_ns[venue] += duration
                self.offline_since[venue] = None

    def summary(self):
        wall_span = self.last - self.first if self.first is not None else 0
        freshness = {venue: round(100 * covered / wall_span, 3) for venue, covered in self.book_ns.items()} if wall_span else {}
        open_gaps = {venue: self.last - start if start is not None else 0 for venue, start in self.offline_since.items()}
        seconds = 1_000_000_000
        return {
            'span_seconds': round(wall_span / seconds, 3),
            'fresh_quote_pct': freshness,
            'simultaneously_fresh_quote_pct': round(100 * self.both_ns / wall_span, 3) if wall_span else None,
            'disconnect_episodes': dict(self.offline),
            'disconnect_to_quote_seconds': {venue: round((self.offline_ns[venue] + open_gaps[venue]) / seconds, 3) for venue in BOOK_TOPICS},
            'longest_disconnect_to_quote_seconds': {venue: round(max(self.longest_offline[venue], open_gaps[venue]) / seconds, 3) for venue in BOOK_TOPICS},
        }


def inspect(root):
    counts = Counter()
    previous = {}
    worst_gap = {}
    clocks = {}
    gaps = regressions = clock_jumps = total = 0
    last_trade = last_mono = last_wall = None
    coverage = QuoteCoverage()
    summary_file = root / 'summary.json'
    summary = json.loads(summary_file.read_text()) if summary_file.is_file() else None
    with (root / 'manifest.csv').open(newline='') as stream:
        records = list(csv.DictReader(stream))
    for index, record in enumerate(records):
        path = root / record['file']
        if path.name != f'{index:06}.jsonl.gz' or path.parent != root or path.stat().st_size != int(record['bytes']):
            raise ValueError('Unexpected segment name or size')
        with path.open('rb') as source:
            if hashlib.file_digest(source, 'sha256').hexdigest() != record['sha256']:
                raise ValueError('Segment checksum failed')
        rows = 0
        with gzip.open(path, 'rt', encoding='ascii') as source:
            for line in source:
                wall, mono, venue, kind, data = json.loads(line)
                if last_mono is not None:
                    if mono < last_mono:
                        raise ValueError('Local monotonic clock moved backwards')
                    clock_jumps += abs((mono - last_mono) - (wall - last_wall)) > 100_000_000
                if not rows and wall != int(record['first_utc_ns']):
                    raise ValueError('Incorrect segment start')
                last_wall, last_mono = wall, mono
                coverage.observe(mono, venue, kind)
                rows += 1
                label = venue + '/' + kind
                counts[label] += 1
                if kind in ('dogeusdt@bookTicker', 'orderbook.1.DOGEUSDT', 'dogeusdt@trade', 'publicTrade.DOGEUSDT'):
                    if label in previous:
                        worst_gap[label] = max(worst_gap.get(label, 0), (mono - previous[label]) / 1e9)
                    previous[label] = mono
                if kind == 'dogeusdt@trade':
                    trade = int(data['data']['t'])
                    if last_trade is not None:
                        gaps += max(0, trade - last_trade - 1)
                        regressions += trade <= last_trade
                    last_trade = trade
                if kind == 'clock':
                    send_wall, send_mono, payload = data
                    server = (int(payload['serverTime']) * 1_000_000 if venue == 'binance' else int(payload['result']['timeNano']))
                    clocks[venue] = {'rtt_ms': round((mono - send_mono) / 1e6, 2),
                                     'server_minus_receive_ms': round((server - wall) / 1e6, 2),
                                     'server_minus_send_ms': round((server - send_wall) / 1e6, 2)}
        if not rows or rows != int(record['rows']) or last_wall != int(record['last_utc_ns']):
            raise ValueError('Incorrect segment length or end')
        total += int(record['bytes'])
    if summary and (summary['counts'] != counts or summary['bytes'] != total or summary['files'] != len(records)):
        raise ValueError('Manifest and terminal summary differ')
    return dict(segments=len(records), bytes=total, counts=counts, terminal=summary and summary['reason'],
                partial=len(list(root.glob('*.partial'))), binance_missing_trade_ids=gaps,
                binance_repeated_or_reversed_ids=regressions, clock_jumps_over_100ms=clock_jumps,
                max_message_gap_sec=worst_gap, last_clock_measurements=clocks,
                quote_arrival_coverage=coverage.summary())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    print(json.dumps(inspect(parser.parse_args().root), indent=2))
