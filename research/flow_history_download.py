"""Fetch and verify public Binance Spot and OKX swap Parquet inputs for historical replay."""
import argparse
import csv
import hashlib
import re
import shutil
import ssl
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import date, datetime, time as midnight_time, timedelta
from pathlib import Path
from threading import Lock
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

from flow_portfolio import SYMBOLS


API = 'https://api.cryptohftdata.com/v1/download'
LEGACY_API = 'https://api.cryptohftdata.com/download'
OKX_SWAPS = ('BTC-USDT-SWAP','ETH-USDT-SWAP','DOGE-USDT-SWAP')
FREE_SPACE = 10 * 1024**3
REQUEST_LOCK = Lock()
REQUEST_SPACING = 1.15
BODY_TIMEOUT = 180
CHUNK_BYTES = 256*1024
next_request = 0.


def wait_for_slot(spacing):
    global next_request
    with REQUEST_LOCK:
        time.sleep(max(0., next_request - time.monotonic()))
        next_request = time.monotonic() + spacing


def parquet_digest(path):
    if not path.is_file() or path.stat().st_size < 12:
        return None
    with path.open('rb') as content:
        if content.read(4) != b'PAR1':
            return None
        content.seek(-4, 2)
        if content.read(4) != b'PAR1':
            return None
        content.seek(0)
        return hashlib.file_digest(content, 'sha256').hexdigest()


def legacy_extent(url, spacing):
    wait_for_slot(spacing)
    with urlopen(Request(url,headers={'Range':'bytes=0-3'}),timeout=15) as source:
        span = re.fullmatch(r'bytes 0-3/(\d+)',source.headers.get('Content-Range',''))
        if source.status != 206 or span is None or int(span[1]) < 12 or source.read(4) != b'PAR1':
            raise OSError('Provider alternate URL did not return a Parquet byte range')
        return f'bytes=0-{int(span[1])-1}'


def copy_body(source, dest, expected, clock=time.monotonic):
    deadline = clock()+BODY_TIMEOUT
    written = 0
    while True:
        if clock() >= deadline:
            raise TimeoutError('Total download-body timeout')
        block = source.read1(CHUNK_BYTES)
        if not block:
            break
        written += len(block)
        if expected is not None and written > expected:
            raise OSError('Response exceeded the expected length')
        dest.write(block)
    if expected is not None and written != expected:
        raise OSError('Response length differs from Content-Length')


def fetch(root, key, spacing=REQUEST_SPACING):
    path = root / 'raw' / key
    digest = parquet_digest(path)
    if digest:
        return key, 'ok', path.stat().st_size, digest, ''
    path.parent.mkdir(parents=True, exist_ok=True)
    partial = path.with_suffix('.part')
    query = f'file={quote(key,safe="/")}'
    legacy = False
    failure = ''
    for attempt in range(3):
        if shutil.disk_usage(root).free < FREE_SPACE:
            failure = 'Insufficient free disk space'
            break
        try:
            endpoint = LEGACY_API if legacy else API
            url = f'{endpoint}?{query}'
            extent = legacy_extent(url,spacing) if legacy else 'bytes=0-'
            wait_for_slot(spacing)
            request = Request(url,headers={'Range':extent})
            with urlopen(request, timeout=45) as source, partial.open('wb') as dest:
                expected = source.headers.get('Content-Length')
                if source.status == 206:
                    span = re.fullmatch(r'bytes 0-(\d+)/(\d+)', source.headers.get('Content-Range', ''))
                    if span is None or int(span[1]) + 1 != int(span[2]):
                        raise OSError('Response range does not cover the entire file')
                    expected = span[2]
                copy_body(source,dest,int(expected) if expected is not None else None)
            digest = parquet_digest(partial)
            if digest is None:
                raise OSError('Response is missing the Parquet header or footer')
            partial.replace(path)
            return key, 'ok', path.stat().st_size, digest, ''
        except HTTPError as error:
            failure = f'HTTP {error.code}'
            if error.code in (400, 401, 403, 404):
                break
            if error.code == 429:
                retry_after = error.headers.get('Retry-After', '')
                time.sleep(min(60, int(retry_after)) if retry_after.isdecimal() else 60)
        except OSError as error:
            failure = str(error)
            if key.startswith('okx_futures/') and isinstance(error,URLError) and isinstance(error.reason,ssl.SSLEOFError):
                legacy = not legacy
            if attempt < 2:
                time.sleep(2**attempt)
    return key, 'error', 0, '', failure


def inputs(start, end, market='binance_spot'):
    hour = datetime.combine(start, midnight_time.min)
    stop = datetime.combine(end, midnight_time.min)
    symbols = OKX_SWAPS if market == 'okx_futures' else SYMBOLS
    kinds = ('orderbook','trades','mark_price') if market == 'okx_futures' else ('orderbook','trades')
    while hour < stop or (market == 'binance_spot' and hour == stop):
        for symbol in symbols:
            for kind in kinds:
                yield f'{market}/{hour:%Y-%m-%d/%H}/{symbol}_{kind}.parquet'
        hour += timedelta(hours=1)


def read_downloads(root):
    primary = root / 'downloads.csv'
    retries = root / 'download-retries.csv'
    records = {}
    for manifest in (primary, retries):
        if manifest == retries and not manifest.is_file():
            continue
        with manifest.open(newline='', encoding='utf-8') as source:
            for row in csv.DictReader(source):
                if row.get('error') is None:  # A running writer may still be writing the last row.
                    continue
                key = row['source_path']
                if key not in records or records[key]['status'] != 'ok' or row['status'] == 'ok':
                    records[key] = row
    return records


def download(root, keys, audit, workers, spacing):
    new_audit = not audit.exists()
    failures = 0
    with audit.open('a', newline='', encoding='utf-8') as report, ThreadPoolExecutor(workers) as pool:
        writer = csv.writer(report)
        if new_audit:
            writer.writerow(('source_path', 'status', 'bytes', 'sha256', 'error'))
        jobs = [pool.submit(fetch,root,key,spacing) for key in keys]
        for count, future in enumerate(as_completed(jobs),1):
            row = future.result()
            writer.writerow(row)
            report.flush()
            failures += row[1] != 'ok'
            if count % 30 == 0 or count == len(keys) or row[1] != 'ok':
                print(f'{count}/{len(keys)} resolved, {failures} errors',row[0],row[4],flush=True)
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--start', type=date.fromisoformat, required=True)
    parser.add_argument('--end', type=date.fromisoformat, required=True, help='Exclusive replay date in UTC.')
    parser.add_argument('--market',choices=('binance_spot','okx_futures'),default='binance_spot')
    parser.add_argument('--retry-errors', action='store_true', help='Retry recorded failures in a separate manifest.')
    args = parser.parse_args()
    if args.end <= args.start:
        parser.error('The end date must follow the start.')
    args.root.mkdir(parents=True, exist_ok=True)
    keys = tuple(inputs(args.start,args.end,args.market))
    if not args.retry_errors:
        if download(args.root,keys,args.root/'downloads.csv',6,REQUEST_SPACING):
            raise SystemExit(1)
        return
    workers, spacing = (3,REQUEST_SPACING) if args.market == 'okx_futures' else (1,15.)
    audit = args.root/'download-retries.csv'
    retries = 4 if args.market == 'okx_futures' else 1
    for round_number in range(retries):
        recorded = read_downloads(args.root)
        pending = tuple(key for key in keys if key in recorded and recorded[key]['status'] != 'ok'
                        and (round_number == 0 or recorded[key]['error'] not in ('HTTP 400','HTTP 401','HTTP 403','HTTP 404')))
        if not pending:
            if any(key in recorded and recorded[key]['status'] != 'ok' for key in keys):
                raise SystemExit(1)
            print('No outstanding download errors', flush=True)
            return
        download(args.root,pending,audit,workers,spacing)
    recorded = read_downloads(args.root)
    if any(key in recorded and recorded[key]['status'] != 'ok' for key in keys):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
