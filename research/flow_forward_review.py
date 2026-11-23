"""Select a forward evaluation endpoint using only elapsed days and closed-trade count."""
import argparse
import csv
import hashlib
import json
from datetime import date, timedelta
from pathlib import Path

from flow_forward import DAY, EPOCH, NS, simulate


VERSION = Path(__file__).with_name('flow_forward_version.json')
MINIMUM_DAYS = 30
MINIMUM_TRADES = 100


def frozen_start(version):
    registration = json.loads(version.read_text(encoding='utf-8-sig'))
    workspace = version.parent.parent
    for name, expected in registration['files'].items():
        with (workspace / name).open('rb') as file:
            if hashlib.file_digest(file, 'sha256').hexdigest() != expected:
                raise ValueError(f'Frozen file has changed: {name}')
    return date.fromisoformat(registration['start_utc_day'])


def sealed_endpoint(archive):
    with (archive / 'manifest.csv').open(newline='', encoding='utf-8') as manifest:
        segments = tuple(csv.DictReader(manifest))
    if not segments:
        return None
    # A file renamed by the recorder can precede its manifest entry. Use only cataloged files.
    last = segments[-1]
    path = archive / f'{len(segments) - 1:06}.jsonl.gz'
    if last['file'] != path.name or path.stat().st_size != int(last['bytes']):
        raise ValueError('The last sealed segment disagrees with the manifest')
    with path.open('rb') as file:
        if hashlib.file_digest(file, 'sha256').hexdigest() != last['sha256']:
            raise ValueError('The last sealed segment failed its checksum')
    return EPOCH + timedelta(days=int(last['last_utc_ns']) // (DAY * NS))


def review(archive, start, horizon):
    available_days = max(0, (horizon - start).days) if horizon is not None else 0
    progress = {'start_utc_day': start.isoformat(), 'sealed_days': available_days}
    if available_days < MINIMUM_DAYS:
        return {**progress, 'state': 'awaiting_days'}

    first_second = (start - EPOCH).days * DAY

    def replay(days):
        return simulate(archive, first_second, first_second + days * DAY)

    def closed_trades(result):
        return result['results']['portfolio/0.001']['trades']

    latest = replay(available_days)
    count = closed_trades(latest)
    if count < MINIMUM_TRADES:
        return {**progress, 'state': 'awaiting_trades', 'completed_trades': count}

    # Extending the run retains earlier fills and enables more decisions near the old endpoint.
    low, high = MINIMUM_DAYS, available_days
    selected = latest
    while low < high:
        middle = (low + high) // 2
        candidate = replay(middle)
        if closed_trades(candidate) >= MINIMUM_TRADES:
            high, selected = middle, candidate
        else:
            low = middle + 1
    endpoint = start + timedelta(days=high)
    return {**progress, 'state': 'reviewed', 'end_utc_exclusive': endpoint.isoformat(), 'review': selected}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    args = parser.parse_args()
    print(json.dumps(review(args.archive, frozen_start(VERSION), sealed_endpoint(args.archive)), indent=2))


if __name__ == '__main__':
    main()
