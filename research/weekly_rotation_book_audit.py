"""Check scheduled historical fills using public Binance depth and receipt times."""
import argparse
import hashlib
import json
import math
import shutil
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path

import pyarrow as pa

from flow_history_download import fetch, parquet_digest
from flow_history_replay import Market
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM, STEPS
from weekly_rotation_review import BASE_COST, STRESS_COST


HOUR = 3600
PERIOD = '2026-01-01/2026-11-01'
REFERENCE = Path(__file__).with_name('weekly_rotation_reentry_results.json')


def source_key(symbol, second):
    begin = datetime.fromtimestamp(second,timezone.utc)
    return f'binance_spot/{begin:%Y-%m-%d/%H}/{symbol}_orderbook.parquet'


def decode_response(path):
    partial = path.with_suffix('.part')
    if not partial.is_file():
        return {}
    with partial.open('rb') as blob:
        if blob.read(4)!=bytes.fromhex('28b52ffd'):
            return {}
        blob.seek(0)
        transport = dict(outer_codec='zstd',wire_sha256=hashlib.file_digest(blob,'sha256').hexdigest(),
                         wire_bytes=partial.stat().st_size)
    if not parquet_digest(path):
        decoded = path.with_suffix('.decoded')
        with pa.input_stream(str(partial),compression='zstd') as source, decoded.open('wb') as target:
            shutil.copyfileobj(source,target,length=256*1024)
        if not parquet_digest(decoded):
            raise ValueError(f'Decoded file is not Parquet: {path}')
        decoded.replace(path)
    return transport


def obtain(root,key):
    path = root/'raw'/key
    try:
        transport = decode_response(path)
    except (OSError, ValueError):
        transport = {}
    key,status,bytes_,digest,error = fetch(root,key)
    if status!='ok' and not transport:
        try:
            transport = decode_response(path)
        except (OSError, ValueError) as exc:
            error = str(exc)
        if transport:
            status,bytes_,digest,error = 'ok',path.stat().st_size,parquet_digest(path),''
    return key,dict(status=status,bytes=bytes_,sha256=digest,error=error,**transport)


def load_books(root, ledger):
    required = {}
    for t in ledger:
        for field in ('entry','exit'):
            key = source_key(t['symbol'],t[field])
            required[key] = t[field]//HOUR*HOUR
    cache, audit = {}, {}
    root.mkdir(parents=True,exist_ok=True)
    with ThreadPoolExecutor(6) as pool:
        for key,info in pool.map(lambda name:obtain(root,name),required):
            audit[key] = info
            if info['status']!='ok':
                continue
            book = Market()
            try:
                quotes,gaps = book.books(root/'raw'/key,required[key])
            except ValueError as exc:
                info.update(status='invalid_book',error=str(exc))
                continue
            cache[key] = quotes
            info.update(source_events=dict(book.audit),quotes=len(quotes),
                        gap_buckets=len(gaps))
    return cache,audit


def price_path(trade, books, delay, cost):
    symbol = trade['symbol']
    buy_key,sell_key = [source_key(symbol,trade[field]) for field in ('entry','exit')]
    result = {k:trade[k] for k in ('symbol','entry','exit','pnl')}
    if buy_key not in books or sell_key not in books:
        return dict(status='source_unavailable',**result)
    fee,slip = cost
    quote = books[buy_key].get(trade['entry']+delay)
    if not quote or not 0<quote[1]<quote[2] or min(quote[3:])<=0:
        return dict(status='no_entry_quote',**result)
    price = quote[2]*(1+slip)
    lot = STEPS[symbol]
    units = math.floor(NOMINAL/price/lot)*lot
    if units>quote[4] or units*price<ORDER_MINIMUM:
        return dict(status='entry_not_executable',**result)
    for second in range(trade['exit']+delay,trade['exit']+delay+120):
        sell = books[sell_key].get(second)
        if (sell and 0<sell[1]<sell[2] and sell[3]>=units
                and units*sell[1]*(1-slip)>=ORDER_MINIMUM):
            return dict(status='priced',quantity=units,buy_ask=quote[2],sell_bid=sell[1],
                        buy_receipt_us=quote[0],sell_receipt_us=sell[0],sell_wait_sec=second-trade['exit']-delay,
                        net_usdt=units*sell[1]*(1-slip)*(1-fee)-units*price*(1+fee),**result)
    return dict(status='no_exit_quote_within_120s',**result)


def review(root):
    report = json.loads(REFERENCE.read_text(encoding='utf-8'))
    ledger = report['results']['binance'][PERIOD]['base']['accounts']['one_week']['ledger']
    books,audit = load_books(root,ledger)
    scenarios = {}
    for delay in (1,3,60):
        for label,cost in [('base',BASE_COST),('stress',STRESS_COST)]:
            rows = [price_path(t,books,delay,cost) for t in ledger]
            priced = [r for r in rows if r['status']=='priced']
            scenarios[f'{delay}s/{label}'] = dict(priced=len(priced),wins=sum(p['net_usdt']>0 for p in priced),
                net_usdt=math.fsum(r['net_usdt'] for r in priced),reference_net_usdt=math.fsum(r['pnl'] for r in priced),
                rows=rows)
    return dict(period=PERIOD,source_ledger_rows=len(ledger),download_audit=audit,scenarios=scenarios)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive',type=Path)
    args = parser.parse_args()
    print(json.dumps(review(args.archive),indent=2))
