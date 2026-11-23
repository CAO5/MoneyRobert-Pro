"""Compute the maximum daily coverage allowed by proven within-file trade gaps."""
import argparse
import json
from datetime import datetime, timezone
from hashlib import sha256
from pathlib import Path

from okx_absorption_review import DAY, HOUR


MIN_FRESH = .90


def assess(snapshot):
    if not snapshot.get('diagnostic_only') or snapshot.get('promotion_allowed') is not False:
        raise ValueError('Supply the audited complete-hour diagnostic')
    start, split, end = (snapshot[key] for key in (
        'start_utc_second','calibration_end_utc_second','end_utc_second'))
    if not start < split < end or any(stamp % DAY for stamp in (start,split,end)):
        raise ValueError('Expected complete UTC calibration and evaluation dates')
    report = {}
    for instrument, panel in snapshot['symbols'].items():
        proven = set()
        for row in panel['rejected_hours']:
            audit = row['audit']
            if audit.get('trade_sequence',0) <= 0 or audit.get('missing_trade_ids',0) <= 0:
                continue
            hour = row['hour_utc_second']
            if hour % HOUR or not start <= hour < end or row['fresh_seconds'] != 0:
                raise ValueError(f'Invalid contaminated-hour evidence: {instrument} at {hour}')
            proven.add(hour)
        days = {}
        for day in range(start,end,DAY):
            gaps = sorted(hour for hour in proven if day <= hour < day+DAY)
            ceiling = DAY-len(gaps)*HOUR
            label = datetime.fromtimestamp(day,timezone.utc).date().isoformat()
            days[label] = dict(role='calibration' if day < split else 'evaluation',
                               contaminated_utc_hours=[(hour-day)//HOUR for hour in gaps],
                               maximum_fresh_seconds=ceiling,maximum_fresh_fraction=ceiling/DAY,
                               can_reach_gate=ceiling>=MIN_FRESH*DAY)
        report[instrument] = dict(days=days,can_reach_gate_all_days=all(row['can_reach_gate'] for row in days.values()))
    return dict(coverage_gate=MIN_FRESH,method='Treat every other second as usable; only proven trade-ID gap hours are excluded.',
                source_fixed_window=[start,split,end],symbols=report)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('diagnostic',type=Path)
    cli.add_argument('output',type=Path)
    args = cli.parse_args()
    raw = args.diagnostic.read_bytes()
    output = assess(json.loads(raw))
    output['diagnostic_sha256'] = sha256(raw).hexdigest()
    args.output.write_text(json.dumps(output,indent=2,allow_nan=False)+'\n',encoding='utf-8')
