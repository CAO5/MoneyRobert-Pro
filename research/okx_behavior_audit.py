"""Verify sealed OKX archives before attempting a replay."""
import argparse
import json
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

from flow_forward import sealed_rows
from okx_behavior_tape import CHANNELS, INSTRUMENTS


NS = 1_000_000_000


def audit(root):
    counts = {instrument: Counter() for instrument in INSTRUMENTS}
    clocks, controls, disconnects, sequence, trade_end = Counter(), Counter(), Counter(), {}, {}
    first_wall = previous_wall = previous_mono = None
    rows = 0
    for wall, mono, venue, channel, content in sealed_rows(root):
        if venue != 'okx':
            raise ValueError(f'Unexpected venue: {venue}')
        if previous_wall is None:
            first_wall = wall
        else:
            clocks['wall_reversals'] += wall < previous_wall
            clocks['monotonic_reversals'] += mono < previous_mono
        previous_wall, previous_mono = wall, mono
        rows += 1
        if channel in ('connect', 'disconnect', 'sequence_gap'):
            for instrument in content.get('instruments',INSTRUMENTS):
                if instrument not in counts:
                    raise ValueError(f'Unexpected instrument scope: {instrument}')
                sequence.pop(instrument,None)
                trade_end.pop(instrument,None)
            controls[channel] += 1
            if channel == 'disconnect':
                reason = content.get('type','unknown')
                if reason == 'stale-book':
                    reason = f'{reason}:{content.get("detail","unknown")}'
                disconnects[reason] += 1
        if channel not in CHANNELS:
            continue
        packet = json.loads(content)
        instrument = packet.get('arg', {}).get('instId')
        if instrument not in counts:
            raise ValueError(f'Unexpected instrument: {instrument}')
        data = packet.get('data', [])
        if not data:
            continue
        counts[instrument][channel] += len(data)
        if channel == 'trades':
            for trade in data:
                last, count = int(trade['tradeId']),int(trade['count'])
                if count <= 0:
                    raise ValueError('Invalid aggregate trade count')
                previous = trade_end.get(instrument)
                if previous is not None:
                    difference = last-count-previous
                    if difference > 0:
                        counts[instrument]['trade_gap_messages'] += 1
                        counts[instrument]['missing_trade_ids'] += difference
                    elif difference < 0:
                        counts[instrument]['overlapping_trade_messages'] += 1
                trade_end[instrument] = max(previous,last) if previous is not None else last
        if channel != 'books':
            continue
        if len(data) != 1:
            raise ValueError(f'Unexpected book message size: {len(data)}')
        update = data[0]
        before, after = int(update['prevSeqId']), int(update['seqId'])
        action = packet.get('action')
        if action == 'snapshot' and before == -1:
            counts[instrument]['snapshots'] += 1
        elif action != 'update' or sequence.get(instrument) != before:
            counts[instrument]['invalid_book_links'] += 1
            sequence.pop(instrument, None)
            continue
        sequence[instrument] = after
    if not rows:
        raise ValueError('No verified sealed messages')
    for instrument in counts:
        for field in ('trade_gap_messages','missing_trade_ids','overlapping_trade_messages','invalid_book_links'):
            counts[instrument].setdefault(field,0)
    return dict(rows=rows, first_utc=utc(first_wall), last_utc=utc(previous_wall),
                elapsed_seconds=(previous_wall-first_wall)/NS, controls=dict(controls),
                disconnect_reasons=dict(disconnects),
                clocks=dict(clocks), instruments=counts)


def utc(wall):
    return datetime.fromtimestamp(wall/NS, timezone.utc).isoformat(timespec='seconds')


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('directory', type=Path)
    args = cli.parse_args()
    print(json.dumps(audit(args.directory), indent=2))
