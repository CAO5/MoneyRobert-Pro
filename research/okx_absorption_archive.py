"""Replay independently captured OKX swap Parquet with official settled funding rates."""
import argparse
import json
import math
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from datetime import datetime, timezone
from hashlib import sha256
from heapq import merge
from itertools import groupby
from pathlib import Path
from urllib.parse import urlencode
from urllib.request import urlopen

from absorption_event_review import DELAY, HORIZONS, PRIMARY, HOUR, NS
from flow_history_download import read_downloads
from flow_history_replay import parquet_rows, verify_downloads
from okx_absorption_review import DAY, DELAY_SENSITIVITY, MS, SPECS, SWAP_COSTS, Swap, describe, utc_day


KINDS = ('orderbook','trades','mark_price')
FUNDING_ENDPOINT = 'https://www.okx.com/api/v5/public/funding-rate-history'
BOOK_FIELDS = ('received_time','event_time','event_type','final_update_id','prev_final_update_id',
               'last_update_id','side','price','quantity')
TRADE_FIELDS = ('received_time','event_time','trade_id','price','quantity','is_buyer_maker')
MARK_FIELDS = ('received_time','event_time','mark_price','next_funding_time')


def keys(instrument, hour):
    label = datetime.fromtimestamp(hour,timezone.utc).strftime('okx_futures/%Y-%m-%d/%H')
    return [f'{label}/{instrument}_{kind}.parquet' for kind in KINDS]


def book_rows(file, hour):
    for key, levels in groupby(parquet_rows(file,BOOK_FIELDS),key=lambda row:row[:6]):
        wall, ts, action, after, before, auxiliary = key
        checkpoint = wall == hour*NS and action == 'snapshot' and after == auxiliary
        if action == 'snapshot':
            before = -1
        elif before is None:
            raise ValueError('OKX archive delta has no previous sequence number')
        data = {'seqId':after,'prevSeqId':before,'ts':ts,'bids':[],'asks':[]}
        for *_, side, price, size in levels:
            data[{'bid':'bids','ask':'asks'}[side]].append((price,size))
        yield wall, 'book', (action,data,checkpoint)


def trade_rows(file):
    for wall, ts, identifier, price, quantity, buyer_maker in parquet_rows(file,TRADE_FIELDS):
        if not isinstance(buyer_maker,bool):
            raise ValueError('Missing taker direction')
        yield wall, 'trade', dict(tradeId=identifier,count=1,ts=ts,px=price,sz=quantity,
                                 side='sell' if buyer_maker else 'buy')


def mark_rows(file):
    rows = sorted(parquet_rows(file,MARK_FIELDS),key=lambda row:row[0])
    for wall, ts, price, next_funding in rows:
        yield wall, 'mark', (ts,price,next_funding)


def read_rates(path, instrument, start, end):
    raw = path.read_bytes()
    rates = decode_rates(raw,instrument,start,end)
    return rates,sha256(raw).hexdigest()


def decode_rates(raw, instrument, start, end):
    body = json.loads(raw)
    if body['code'] != '0':
        raise ValueError(f'Invalid OKX settlement response for {instrument}')
    rates = {}
    for row in body['data']:
        stamp, rate = int(row['fundingTime']), float(row['realizedRate'])
        if row['instId'] != instrument or not math.isfinite(rate) or (stamp in rates and rates[stamp] != rate):
            raise ValueError(f'Invalid or conflicting funding record for {instrument}')
        rates[stamp] = rate
    if not rates or min(rates) >= start*MS or max(rates) < end*MS:
        raise ValueError(f'OKX settled-funding history does not bracket {instrument} evaluation')
    return rates


def fetch_rates(root, start, end):
    root.mkdir(parents=True,exist_ok=True)
    for instrument in SPECS:
        path = root/f'{instrument}-www.okx.com-settled-funding.json'
        if path.is_file():
            read_rates(path,instrument,start,end)
            continue
        cursor = (end+DAY)*MS
        records = {}
        for _ in range(20):
            query = urlencode(dict(instId=instrument,limit=100,after=cursor))
            with urlopen(f'{FUNDING_ENDPOINT}?{query}',timeout=15) as source:
                page = json.load(source)
            if page.get('code') != '0' or not page.get('data'):
                raise ValueError(f'Missing official funding history for {instrument} before {cursor}')
            stamps = []
            for row in page['data']:
                stamp = int(row['fundingTime'])
                if stamp in records and records[stamp] != row:
                    raise ValueError(f'Inconsistent official funding history for {instrument}')
                records[stamp] = row
                stamps.append(stamp)
            if min(stamps) >= cursor:
                raise ValueError(f'Funding pagination failed to advance for {instrument}')
            cursor = min(stamps)
            if cursor < start*MS:
                break
            time.sleep(.3)
        raw = (json.dumps(dict(code='0',data=list(records.values())),indent=2)+'\n').encode('utf-8')
        decode_rates(raw,instrument,start,end)
        path.write_bytes(raw)


def feed_hour(panel, paths, hour, start, end):
    ordered = merge(book_rows(paths[0],hour),trade_rows(paths[1]),mark_rows(paths[2]),key=lambda row:row[0])
    latest = hour*NS
    for wall, kind, content in ordered:
        if not latest <= wall < (hour+HOUR)*NS:
            raise ValueError(f'Receipts out of order/outside hour: {paths[0].parent}')
        latest = wall
        if kind == 'book':
            action, row, checkpoint = content
            if checkpoint:
                if panel.sequence == row['seqId']:
                    continue
                if wall > start*NS or panel.sequence is not None:
                    panel.bad(hour,'archive_checkpoint_reanchor')
                panel.sequence = panel.top = None
                panel.book(wall,wall,action,row,checkpoint=True)
            else:
                panel.book(wall,wall,action,row)
        elif kind == 'trade':
            panel.trades(wall,wall,(content,))
        else:
            ts, price, next_funding = content
            if price is not None:
                panel.mark_message({'ts':ts,'markPx':price})
            if next_funding is not None:
                panel.funding_message({'ts':ts,'fundingTime':next_funding,'settState':'forecast'})
    panel.finish_hour(hour,start,end,True)


def replay_symbol(root, instrument, start, split, end):
    panel = Swap(instrument)
    rate_file = root/f'{instrument}-www.okx.com-settled-funding.json'
    panel.rate, funding_digest = read_rates(rate_file,instrument,start,end)
    panel.funding.update(panel.rate)
    manifest = read_downloads(root)
    digests = []
    for hour in range(start,end,HOUR):
        paths = keys(instrument,hour)
        verify_downloads(root,paths)
        digests.extend((name,manifest[name]['sha256']) for name in paths)
        feed_hour(panel,[root/'raw'/name for name in paths],hour,start,end)
        if (hour+HOUR)%DAY == 0:
            print(instrument,datetime.fromtimestamp(hour,timezone.utc).date(),'finished',flush=True)
    panel.funding_times = sorted(panel.funding)
    panel.mark_times = sorted(panel.mark)
    report = describe(panel,start,split,end,compare_controls=True,delay_sensitivity=DELAY_SENSITIVITY)
    report.update(source_digests=digests,settled_funding_sha256=funding_digest)
    return instrument,report


def verify_symbol_hours(root, symbol, start, end, manifest):
    days = {}
    for day in range(start,end,DAY):
        coverage = Counter()
        for hour in range(day,day+DAY,HOUR):
            files = keys(symbol,hour)
            if any(manifest.get(name,{}).get('status') != 'ok' or not (root/'raw'/name).is_file()
                   for name in files):
                coverage['hours_missing_files'] += 1
                continue
            verify_downloads(root,files)
            paths = [root/'raw'/name for name in files]
            panel = Swap(symbol)
            feed_hour(panel,paths,hour,hour,hour+HOUR)
            coverage['hours_hash_verified'] += 1
            coverage['fresh_seconds'] += panel.coverage[day//DAY]
            coverage['book_sequence_breaks'] += panel.audit['book_sequence']
            coverage['trade_sequence_breaks'] += panel.audit['trade_sequence']
            coverage['missing_trade_ids'] += panel.audit['missing_trade_ids']
            coverage['overlapping_trade_messages'] += panel.audit['overlapping_trade_messages']
            coverage['hours_with_90_percent_fresh'] += panel.coverage[day//DAY] >= HOUR*.90
        label = datetime.fromtimestamp(day,timezone.utc).date().isoformat()
        days[label] = dict(coverage)
        print(symbol,label,'coverage audited',coverage['hours_hash_verified'],flush=True)
    return symbol,days


def verify_complete_hours(root, start, end, manifest):
    with ProcessPoolExecutor(len(SPECS)) as pool:
        jobs = [pool.submit(verify_symbol_hours,root,symbol,start,end,manifest) for symbol in SPECS]
        instruments = dict(job.result() for job in jobs)
    return dict(source='CryptoHFTData OKX archive',diagnostic_only=True,
                note='Complete hours are reconstructed independently with fresh clock warmup. Missing hours are not scored.',
                start_utc_second=start,end_utc_second=end,symbols=instruments)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('root',type=Path)
    cli.add_argument('--start',type=utc_day,required=True)
    cli.add_argument('--end',type=utc_day,required=True)
    cli.add_argument('--calibration-days',type=int,default=1)
    group = cli.add_mutually_exclusive_group(required=True)
    group.add_argument('--output',type=Path)
    group.add_argument('--audit-complete-hours',type=Path,
                       metavar='REPORT',help='Verify available hours without scoring or skipping gaps in the actual replay')
    group.add_argument('--fetch-settled-funding',action='store_true',
                       help='Fetch official settlement records without replaying')
    args = cli.parse_args()
    start, end = args.start,args.end
    if end <= start:
        cli.error('The end date must follow the start date')
    if args.audit_complete_hours:
        report = verify_complete_hours(args.root,start,end,read_downloads(args.root))
        args.audit_complete_hours.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
        return
    split = start+args.calibration_days*DAY
    if args.calibration_days < 1 or split >= end:
        cli.error('At least one complete UTC day each for calibration and evaluation')
    if args.fetch_settled_funding:
        fetch_rates(args.root,start,end)
        return
    manifest = read_downloads(args.root)
    missing = [name for hour in range(start,end,HOUR) for instrument in SPECS for name in keys(instrument,hour)
               if manifest.get(name,{}).get('status') != 'ok' or not (args.root/'raw'/name).is_file()]
    if missing:
        cli.error(f'{len(missing)} required files missing, first: {missing[0]}')
    with ProcessPoolExecutor(len(SPECS)) as pool:
        jobs = [pool.submit(replay_symbol,args.root,symbol,start,split,end) for symbol in SPECS]
        symbols = dict(job.result() for job in jobs)
    output = dict(source='CryptoHFTData OKX archive + OKX actual funding history',
                  start_utc_second=start,calibration_end_utc_second=split,evaluation_start_utc_second=split,
                  end_utc_second=end,primary_seconds=PRIMARY,all_horizons_seconds=HORIZONS,
                  delay_seconds=DELAY,delay_diagnostics_seconds=DELAY_SENSITIVITY,
                  costs=SWAP_COSTS,contract_specs=SPECS,symbols=symbols)
    args.output.write_text(json.dumps(output,indent=2,allow_nan=False)+'\n',encoding='utf-8')


if __name__ == '__main__':
    main()
