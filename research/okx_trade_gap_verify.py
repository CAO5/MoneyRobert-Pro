"""Verify vendor trade-sequence gaps against OKX trade history without repairing receipts."""
import argparse
import json
from datetime import datetime, timezone
from decimal import Decimal
from hashlib import file_digest, sha256
from pathlib import Path

from okx_absorption_archive import TRADE_FIELDS, parquet_rows
from okx_absorption_review import NS


def public_trades(raw, instrument):
    data = json.loads(raw)
    if isinstance(data,dict):
        if data['code'] != '0':
            raise ValueError('Official response was not successful')
        data = data['data']
    trades = {}
    for row in data:
        if row['instId'] != instrument or row['side'] not in ('buy','sell'):
            raise ValueError('Unexpected official instrument or taker side')
        trade_id = int(row['tradeId'])
        value = int(row['ts']),Decimal(row['px']),Decimal(row['sz']),row['side']=='sell'
        if trade_id in trades and trades[trade_id] != value:
            raise ValueError('Conflicting official records')
        trades[trade_id] = value
    return trades


def verify_gap(archive, official, instrument, before, after):
    if before >= after-1:
        raise ValueError('Supply two trade IDs framing a forward sequence gap')
    public_raw = official.read_bytes()
    public = public_trades(public_raw,instrument)
    previous, gaps = None,[]
    for row in parquet_rows(archive,TRADE_FIELDS):
        _, _, identifier, _, _, _ = row
        if identifier in (before,after) and identifier in public:
            _, trade_ts, _, price, quantity, buyer_maker = row
            if public[identifier] != (trade_ts,Decimal(price),Decimal(quantity),buyer_maker):
                raise ValueError(f'Anchor trade differs between the two sources: {identifier}')
        if identifier == after and previous is not None and previous[2] == before:
            gaps.append((previous,row))
        previous = row
    if len(gaps) != 1:
        raise ValueError('Expected exactly one observed gap between the supplied IDs')
    left,right = gaps[0]
    missing = sorted(number for number in public if before < number < after)
    if any(not left[1] <= public[number][0] <= right[1] for number in missing):
        raise ValueError('Official exchange timestamp is outside the neighboring trades')
    with archive.open('rb') as source:
        digest = file_digest(source,'sha256').hexdigest()
    return dict(instrument=instrument,vendor_sha256=digest,official_rows_sha256=sha256(public_raw).hexdigest(),
                before_trade_id=before,after_trade_id=after,
                vendor_receipt_before_utc=datetime.fromtimestamp(left[0]/NS,timezone.utc).isoformat(),
                vendor_receipt_after_utc=datetime.fromtimestamp(right[0]/NS,timezone.utc).isoformat(),
                anchor_matches=len({before,after}&public.keys()),expected_missing_ids=after-before-1,
                confirmed_missing_ids=len(missing),
                first_missing_exchange_ms=public[missing[0]][0] if missing else None,
                last_missing_exchange_ms=public[missing[-1]][0] if missing else None,
                backfilled_receipts=False,gap_retained=True)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('archive',type=Path)
    cli.add_argument('official',type=Path)
    cli.add_argument('--instrument',required=True)
    cli.add_argument('--before',type=int,required=True)
    cli.add_argument('--after',type=int,required=True)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    result = verify_gap(args.archive,args.official,args.instrument,args.before,args.after)
    args.output.write_text(json.dumps(result,indent=2,allow_nan=False)+'\n',encoding='utf-8')
