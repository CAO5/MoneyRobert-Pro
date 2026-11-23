"""Check native vendor data on a free month-open date without estimating returns."""
import argparse
import asyncio
import gzip
import json
import math
import time
import zlib
from bisect import bisect_left
from collections import Counter
from datetime import datetime, timezone
from hashlib import file_digest
from io import BytesIO
from pathlib import Path
from types import SimpleNamespace
from urllib.parse import urlencode

import aiohttp

from okx_absorption_review import (DELAY, DELAY_SENSITIVITY, HORIZONS, PRIMARY, DAY, HOUR, MS, NS, SWAP_COSTS,
                                   AccountedFills, Quality, Swap, utc_day)
from okx_behavior_tape import CHANNELS, INSTRUMENTS


URL = 'https://api.tardis.dev/v1/data-feeds/okex-swap'
SLICE_MINUTES = 10
MAX_SLICE_BYTES = 32*1024**2
MAX_DECODED_BYTES = 512*1024**2
RETRYABLE_STATUS = {408,500,502,503,504}
BROKEN_GZIP = (EOFError,gzip.BadGzipFile,zlib.error)
TRANSIENT_ERRORS = (aiohttp.ClientConnectionError,aiohttp.ClientPayloadError,TimeoutError,*BROKEN_GZIP)


def archive_path(root, minute):
    return root/f'{minute:04d}.ndjson.gz'


def verify_gzip(stream):
    if stream.read(3) != b'\x1f\x8b\x08':
        raise gzip.BadGzipFile('Missing gzip header')
    stream.seek(0)
    size = 0
    with gzip.GzipFile(fileobj=stream) as decoded:
        while block := decoded.read(1024**2):
            size += len(block)
            if size > MAX_DECODED_BYTES:
                raise ValueError('Decoded native slice exceeds the audit size cap')


async def request_slice(client, day, filters, minute, size):
    query = urlencode({'from':day,'offset':minute,'sliceSize':size,'filters':filters})
    async with client.get(f'{URL}?{query}') as response:
        response.raise_for_status()
        if response.headers.get('Content-Encoding') != 'gzip' or response.headers.get('x-slice-size') != str(size):
            raise ValueError(f'Unexpected native format or slice size at minute {minute}')
        data = bytearray()
        async for block in response.content.iter_chunked(256*1024):
            data.extend(block)
            if len(data) > MAX_SLICE_BYTES:
                raise ValueError(f'Native slice exceeds the audit size cap at minute {minute}')
    raw = bytes(data)
    verify_gzip(BytesIO(raw))
    return raw


async def download_slice(client, day, filters, minute, size):
    attempts = 1 if size > 1 else 3
    for attempt in range(attempts):
        try:
            return await request_slice(client,day,filters,minute,size)
        except TRANSIENT_ERRORS:
            if size == 1 and attempt+1 == attempts:
                raise
        except aiohttp.ClientResponseError as error:
            if error.status not in RETRYABLE_STATUS or (size == 1 and attempt+1 == attempts):
                raise
        if size == 1:
            await asyncio.sleep(2**attempt)
    midpoint = size//2
    print(f'Splitting offset {minute} ({size} min) after a transient failure',flush=True)
    first = await download_slice(client,day,filters,minute,midpoint)
    second = await download_slice(client,day,filters,minute+midpoint,size-midpoint)
    if len(first)+len(second) > MAX_SLICE_BYTES:
        raise ValueError(f'Native slice exceeds the audit size cap at minute {minute}')
    return first+second


async def fetch(root, day, first_minute, hours, *, keep_going=False):
    root.mkdir(parents=True,exist_ok=True)
    filters = json.dumps([dict(channel=channel,symbols=INSTRUMENTS) for channel in CHANNELS],separators=(',',':'))
    timeout = aiohttp.ClientTimeout(total=45,sock_connect=8,sock_read=15)
    missing = []
    async with aiohttp.ClientSession(timeout=timeout,auto_decompress=False,headers={'Accept-Encoding':'gzip'}) as client:
        for minute in range(first_minute,first_minute+60*hours,SLICE_MINUTES):
            path = archive_path(root,minute)
            if path.exists():
                try:
                    with path.open('rb') as stream:
                        verify_gzip(stream)
                    continue
                except BROKEN_GZIP as error:
                    quarantine = path.with_name(f'{path.name}.{time.time_ns()}.invalid')
                    path.rename(quarantine)
                    print(f'Quarantined offset {minute}: {type(error).__name__}',flush=True)
            try:
                data = await download_slice(client,day,filters,minute,SLICE_MINUTES)
            except TRANSIENT_ERRORS as error:
                if not keep_going:
                    raise
                missing.append(minute)
                print(f'Deferred offset {minute}: {type(error).__name__}',flush=True)
                continue
            except aiohttp.ClientResponseError as error:
                if not keep_going or error.status not in RETRYABLE_STATUS:
                    raise
                missing.append(minute)
                print(f'Deferred offset {minute}: HTTP {error.status}',flush=True)
                continue
            temporary = path.with_suffix('.partial')
            with temporary.open('xb') as stream:
                stream.write(data)
            temporary.rename(path)
            print(f'Sealed offset {minute}: {len(data)} bytes',flush=True)
    if missing:
        raise ValueError(f'These offsets still require download: {missing}')


def receipt_ns(value):
    whole, subsecond = value.removesuffix(b'Z').split(b'.',1)
    if not value.endswith(b'Z') or not 1 <= len(subsecond) <= 9 or not subsecond.isdigit():
        raise ValueError('Invalid native receipt timestamp')
    seconds = int(datetime.fromisoformat(whole.decode('ascii')).replace(tzinfo=timezone.utc).timestamp())
    return seconds*NS+int(subsecond.ljust(9,b'0'))


def verify_settlement(panel, stamp, quality, end):
    event_time = stamp//MS-PRIMARY//2-DELAY
    receipts = (panel.quotes.get(event_time+DELAY),panel.quotes.get(event_time+DELAY+PRIMARY))
    horizon = str(PRIMARY)

    def evaluate():
        return {side:panel.evaluate(SimpleNamespace(time=event_time,side=side),quality,end)[horizon]
                for side in ('bid','ask')}

    actual = evaluate()
    rate_table = panel.rate
    try:
        panel.rate = dict.fromkeys(rate_table,0.)
        without_funding = evaluate()
    finally:
        panel.rate = rate_table
    index = bisect_left(panel.mark_times,stamp)-1
    mark_time = panel.mark_times[index] if index >= 0 else None
    checked = []
    for side,results in actual.items():
        if results is None:
            continue
        for lane,observed in results.items():
            if observed is None:
                continue
            if mark_time is None or stamp-mark_time > 2*MS or not receipts[0].exchange_ms < stamp < receipts[1].exchange_ms:
                raise ValueError('Settlement was scored without a pre-settlement mark or a crossing position')
            slippage = SWAP_COSTS[lane][1]
            opening = receipts[0].prices
            reference = opening[1]*(1+slippage) if side=='bid' else opening[0]*(1-slippage)
            signed_carry = (-1 if side=='bid' else 1)*rate_table[stamp]*panel.mark[mark_time]
            if not math.isclose(observed-without_funding[side][lane],signed_carry/reference*10000,abs_tol=1e-8):
                raise ValueError('Settlement debit or credit disagrees with signed funding')
            checked.append(f'{side}:{lane}')
    return dict(settlement_exchange_ms=stamp,mark_age_ms=stamp-mark_time if mark_time is not None else None,
                verified_carry_lanes=checked)


def execution_rehearsal(panel, start, end):
    quality = Quality(start,end,panel.gaps)
    panel.mark_times = sorted(panel.mark)
    panel.funding_times = sorted(panel.funding)
    grid = list(range(start+max(HORIZONS),end-max(HORIZONS),max(HORIZONS)))
    results = {}
    for delay in (DELAY,*DELAY_SENSITIVITY):
        account = AccountedFills(panel,quality,end,delay)
        for decision in grid:
            for side in ('bid','ask'):
                account(SimpleNamespace(time=decision,side=side))
        results[str(delay)] = account.counts
    return dict(grid_times_per_side=len(grid),counts_by_delay=results,
                settlement_checks=[verify_settlement(panel,stamp,quality,end) for stamp in sorted(panel.rate)
                                   if (start+PRIMARY)*MS <= stamp < (end-PRIMARY)*MS])


def replay_native(root, start, hours, *, keep_execution=False, collect_events=False):
    panels = {name:Swap(name,quality_only=not collect_events,keep_execution=keep_execution) for name in INSTRUMENTS}
    counts = {name:Counter() for name in INSTRUMENTS}
    day = start-start%DAY
    first_minute = (start-day)//60
    hour, end = start,start+hours*HOUR
    previous = first = last = None
    interruptions = 0
    digests = {}
    def finish_through(second):
        nonlocal hour
        while second >= hour+HOUR:
            for panel in panels.values():
                panel.finish_hour(hour,start,end,collect_events)
            hour += HOUR

    for minute in range(first_minute,first_minute+60*hours,SLICE_MINUTES):
        path = archive_path(root,minute)
        with path.open('rb') as source:
            digests[path.name] = file_digest(source,'sha256').hexdigest()
        lower = (day+minute*60)*NS
        upper = lower+60*SLICE_MINUTES*NS
        with gzip.open(path,'rb') as source:
            for line in source:
                if not line.strip():
                    interruptions += 1
                    second = max(lower,previous or lower)//NS
                    finish_through(second)
                    for panel in panels.values():
                        panel.bad(second,'provider_disconnect')
                        panel.reset()
                    continue
                stamp, content = line.split(b' ',1)
                wall = receipt_ns(stamp)
                if not lower <= wall < upper or (previous is not None and wall < previous):
                    raise ValueError(f'Invalid receipt ordering or minute bounds in {path.name}')
                finish_through(wall//NS)
                previous = last = wall
                if first is None:
                    first = wall
                packet = json.loads(content)
                arg = packet.get('arg',{})
                channel, name = arg.get('channel'),arg.get('instId')
                if channel not in CHANNELS or name not in panels:
                    continue
                panel = panels[name]
                if packet.get('event') in ('subscribe','unsubscribe'):
                    event = packet['event']
                    counts[name][f'{channel}_{event}'] += 1
                    if channel in ('books','trades'):
                        panel.bad(wall//NS,f'provider_{channel}_{event}')
                        if channel == 'books':
                            panel.sequence = panel.top = None
                        else:
                            panel.trade_id = None
                    continue
                if packet.get('event') == 'error':
                    raise ValueError('Native archive records a subscription error')
                rows = packet.get('data',[])
                counts[name][channel] += len(rows)
                if channel == 'books':
                    if len(rows) != 1:
                        raise ValueError('Expected one native book envelope')
                    panel.book(wall,wall,packet['action'],rows[0])
                elif channel == 'trades':
                    panel.trades(wall,wall,rows)
                elif keep_execution and channel == 'funding-rate':
                    for row in rows:
                        panel.funding_message(row)
                elif keep_execution and channel == 'mark-price':
                    for row in rows:
                        panel.mark_message(row)
    finish_through(end)
    metadata = dict(diagnostic_only=True,source=URL,receiver='Tardis collection clock, not the local device',
                    start_utc=datetime.fromtimestamp(start,timezone.utc).isoformat(),interval_seconds=end-start,
                    observed_receipt_ns=[first,last],provider_disconnect_markers=interruptions,slice_sha256=digests)
    return panels,counts,metadata


def audit(root, start, hours, *, execution_check=False):
    panels, counts, metadata = replay_native(root,start,hours,keep_execution=execution_check)
    reports = {}
    for name,panel in panels.items():
        rows = panel.hour_coverage
        totals = {key:sum(row[key] for row in rows.values()) for key in ('usable','received','fresh')}
        reports[name] = dict(**totals,trade_discontinuity=any(row['trade_discontinuity'] for row in rows.values()),
                             counts=dict(counts[name]),audit=dict(panel.audit))
        if hours > 1:
            reports[name]['hourly'] = {datetime.fromtimestamp(stamp,timezone.utc).isoformat():row
                                       for stamp,row in rows.items()}
        if execution_check:
            reports[name]['execution_rehearsal'] = execution_rehearsal(panel,start,start+hours*HOUR)
    return dict(**metadata,symbols=reports)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('root',type=Path)
    cli.add_argument('--day',type=utc_day,required=True)
    cli.add_argument('--hour',type=int,default=0)
    cli.add_argument('--hours',type=int,default=1)
    cli.add_argument('--fetch',action='store_true')
    cli.add_argument('--fetch-only',action='store_true',help='Download disjoint slices without replaying')
    cli.add_argument('--execution-check',action='store_true',help='Check fixed-time quote and cost eligibility without strategy returns')
    cli.add_argument('--output',type=Path)
    args = cli.parse_args()
    day = datetime.fromtimestamp(args.day,timezone.utc).date()
    if day.day != 1 or not 0 <= args.hour < args.hour+args.hours <= 24:
        cli.error('Choose complete hours inside a UTC month-open day')
    if args.execution_check and args.hours != 24:
        cli.error('The execution check requires a complete UTC day')
    if args.fetch_only and (not args.fetch or args.execution_check or args.output is not None):
        cli.error('Fetch-only requires --fetch without scoring or an output report')
    if not args.fetch_only and args.output is None:
        cli.error('--output is required for replay')
    if args.fetch:
        asyncio.run(fetch(args.root,day.isoformat(),args.hour*60,args.hours,keep_going=args.fetch_only))
    if args.fetch_only:
        raise SystemExit(0)
    data = audit(args.root,args.day+args.hour*HOUR,args.hours,execution_check=args.execution_check)
    if args.execution_check:
        data.update(promotion_allowed=False,execution_check_note='Fixed-time virtual orders; eligibility and carry verification only. No strategy PnL.')
    args.output.write_text(json.dumps(data,indent=2,allow_nan=False)+'\n',encoding='utf-8')
