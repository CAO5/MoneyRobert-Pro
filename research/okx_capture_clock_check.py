"""Compare receiver clocks and common native book messages across two OKX captures."""
import argparse
import csv
import json
from datetime import datetime, timezone
from decimal import Decimal
from hashlib import file_digest
from pathlib import Path
from statistics import median

from flow_forward import sealed_rows
from okx_absorption_archive import book_rows
from okx_absorption_review import HOUR, NS, utc_day


def percentiles(values):
    if not values:
        return None
    ordered = sorted(values)
    return dict(min=ordered[0],p50=median(ordered),p90=ordered[int(.90*(len(ordered)-1))],
                p99=ordered[int(.99*(len(ordered)-1))],max=ordered[-1])


def message_key(update):
    return int(update['seqId']),int(update['prevSeqId']),int(update['ts'])


def level_changes(update):
    return tuple(tuple(sorted((Decimal(price),Decimal(quantity)) for price,quantity,*_ in update[side]))
                 for side in ('bids','asks'))


def direct_updates(tape, instrument, start_ns, end_ns):
    native = {}
    for wall, monotonic, venue, channel, content in sealed_rows(tape):
        if venue != 'okx':
            raise ValueError('Only OKX public captures can be compared')
        if channel != 'books' or not start_ns <= wall < end_ns:
            continue
        raw = json.loads(content)
        if raw['arg']['instId'] != instrument or raw.get('action') != 'update':
            continue
        for data in raw.get('data',()):
            if data['asks'] or data['bids']:
                key = message_key(data)
                native.setdefault(key,(wall,monotonic,level_changes(data)))
    return native


def seal_bounds(tape):
    with (tape/'manifest.csv').open(newline='',encoding='utf-8') as source:
        entries = list(csv.DictReader(source))
    if not entries:
        raise ValueError(f'No sealed capture: {tape}')
    return int(entries[0]['first_utc_ns']),int(entries[-1]['last_utc_ns'])


def fresh_book_seconds(probe, baseline, start, end):
    """Describe book arrival freshness, without claiming continuity or strategy coverage."""
    first_second,last_second = (start+NS-1)//NS,end//NS

    def ages(updates):
        return [(wall//NS,monotonic//1_000_000-key[2]) for key,(wall,monotonic,_) in updates.items()]

    probe_ages,baseline_ages = ages(probe),ages(baseline)
    fastest = min((age for _,age in probe_ages+baseline_ages),default=None)

    def fresh(rows):
        return {second for second,age in rows if first_second <= second < last_second and age-fastest <= 1000}

    probe_seconds,reference_seconds = fresh(probe_ages),fresh(baseline_ages)
    return dict(complete_seconds=max(0,last_second-first_second),probe_seconds=len(probe_seconds),
                reference_seconds=len(reference_seconds),either_seconds=len(probe_seconds|reference_seconds),
                added_by_probe=len(probe_seconds-reference_seconds),relative_delay_limit_ms=1000,
                note='Shared fastest observed receipt offset; ignores book continuity, trades, and quote executability.')


def compare_same_host(tape, reference, instrument):
    bounds = seal_bounds(tape),seal_bounds(reference)
    start = max(first for first,_ in bounds)
    end = min(last for _,last in bounds)
    if start >= end:
        raise ValueError('Local captures do not have a sealed time overlap')
    probe = direct_updates(tape,instrument,start,end)
    baseline = direct_updates(reference,instrument,start,end)
    matches = set(probe)&set(baseline)
    identical = [key for key in matches if probe[key][2] == baseline[key][2]]
    delays = [(probe[key][1]-baseline[key][1])/1e6 for key in identical]

    def counts(updates):
        return dict(nonempty_updates=len(updates), receipt_seconds=len({row[0]//NS for row in updates.values()}))

    return dict(diagnostic_only=True,instrument=instrument,probe=str(tape.resolve()),reference=str(reference.resolve()),
                overlap_receipt_ns=[start,end],overlap_seconds=(end-start)/NS,
                probe_counts=counts(probe),reference_counts=counts(baseline),exact_header_matches=len(matches),
                payload_mismatches=len(matches)-len(identical),probe_minus_reference_monotonic_ms=percentiles(delays),
                probe_over_one_second_earlier=sum(delay < -1000 for delay in delays),
                reference_over_one_second_earlier=sum(delay > 1000 for delay in delays),
                raw_book_freshness=fresh_book_seconds(probe,baseline,start,end),
                note='Same-machine receipt comparison for identical native updates; not absolute latency or strategy coverage.')


def compare(tape, archive, instrument, hour):
    native = direct_updates(tape,instrument,hour*NS,(hour+HOUR)*NS)
    if not native:
        raise ValueError('No native nonempty updates in the selected hour')
    matched, changed = {}, 0
    mismatches = []
    previous = None
    receipts = []
    for wall, _, (action,row,_) in book_rows(archive,hour):
        if not hour*NS <= wall < (hour+HOUR)*NS:
            raise ValueError('Archive message assigned to the wrong UTC hour')
        if previous is not None and wall < previous:
            raise ValueError('Third-party receipt order reverses')
        previous = wall
        if action != 'update' or not (row['bids'] or row['asks']):
            continue
        changed += 1
        key = message_key(row)
        if key in native:
            matched.setdefault(key,wall)
            if native[key][2] != level_changes(row):
                mismatches.append(key)
    for key,vendor_wall in matched.items():
        local_wall,local_mono,_ = native[key]
        stamp = key[2]*1_000_000
        receipts.append(((local_wall-stamp)/1e6,(vendor_wall-stamp)/1e6,(local_mono-stamp)//1_000_000,
                         (local_wall-vendor_wall)/1e6))
    local_min = min((row[2] for row in receipts),default=0)
    excess = [row[2]-local_min for row in receipts]
    with archive.open('rb') as source:
        digest = file_digest(source,'sha256').hexdigest()
    return dict(instrument=instrument,utc_hour=datetime.fromtimestamp(hour,timezone.utc).isoformat(),
                vendor_archive_sha256=digest,direct_updates=len(native),vendor_updates=changed,
                comparable_exchange_time_ms=[min(key[2] for key in matched),max(key[2] for key in matched)] if matched else None,
                exactly_matched=len(receipts),matched_fraction_of_direct=len(receipts)/len(native),
                matched_payload_mismatches=len(mismatches),payload_mismatch_examples=mismatches[:5],
                delta_wall_clock_ms=percentiles([row[3] for row in receipts]),
                direct_wall_minus_exchange_ms=percentiles([row[0] for row in receipts]),
                vendor_wall_minus_exchange_ms=percentiles([row[1] for row in receipts]),
                direct_added_delay_above_observed_min_ms=percentiles(excess),
                direct_updates_above_one_second_extra_delay=sum(value>1000 for value in excess),
                note='Exact native sequence and exchange timestamp join; diagnostics only. Wall offsets are not latency.')


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('tape',type=Path)
    cli.add_argument('archive',type=Path)
    cli.add_argument('--date',type=utc_day)
    cli.add_argument('--hour',type=int)
    cli.add_argument('--same-host',action='store_true',help='Compare two local sealed captures instead of vendor data')
    cli.add_argument('--instrument',required=True)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    if args.same_host:
        if args.date is not None or args.hour is not None:
            cli.error('Same-host mode uses the sealed overlap, not a specified UTC hour')
        data = compare_same_host(args.tape,args.archive,args.instrument)
    else:
        if args.date is None or args.hour is None or not 0 <= args.hour < 24:
            cli.error('Vendor mode requires a UTC date and hour (0–23)')
        data = compare(args.tape,args.archive,args.instrument,args.date+args.hour*HOUR)
    args.output.write_text(json.dumps(data,indent=2,allow_nan=False)+'\n',encoding='utf-8')
