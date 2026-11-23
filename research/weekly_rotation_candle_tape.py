"""Record published Binance spot candles with local receipt clocks."""
import argparse
import asyncio
import json
import time
from pathlib import Path

import aiohttp

from forward_tape import Tape
from slow_spot_replay import BAR, SYMBOLS


ENDPOINT = 'https://data-api.binance.vision/api/v3/klines'
WARMUP = 31*86400//BAR


async def fetch(session, symbol, first, stop):
    queued = time.time_ns()
    query = dict(symbol=symbol, interval='4h', startTime=1000*first, endTime=1000*stop-1,
                 limit=(stop-first)//BAR)
    async with session.get(ENDPOINT,params=query) as response:
        response.raise_for_status()
        raw = await response.json()
    if [r[0] for r in raw] != [t*1000 for t in range(first,stop,BAR)]:
        raise ValueError(f'Incomplete or not yet published {symbol} candle')
    for row in raw:
        if row[6] != row[0]+1000*BAR-1:
            raise ValueError(f'Unexpected {symbol} candle close time')
        opening, high, low, closing = map(float,row[1:5])
        if not 0<low<=min(opening,closing)<=max(opening,closing)<=high:
            raise ValueError(f'Malformed {symbol} candle')
    return dict(request_ns=queued,received_ns=time.time_ns(),rows=raw)


async def pair(session, first, stop):
    responses = await asyncio.gather(*(fetch(session,s,first,stop) for s in SYMBOLS),
                                     return_exceptions=True)
    for answer in responses:
        if isinstance(answer,Exception):
            raise answer
    return dict(zip(SYMBOLS,responses))


async def collect(root, seconds):
    tape = Tape(root)
    initial = time.monotonic()
    start_wall = int(time.time())
    target = start_wall//BAR*BAR
    ready = False
    reason = 'interrupted'
    try:
        timeout = aiohttp.ClientTimeout(total=15,sock_connect=10)
        async with aiohttp.ClientSession(timeout=timeout,trust_env=True) as session:
            while time.monotonic()-initial < seconds and not (root/'STOP').exists():
                if ready and time.time()<target:
                    await asyncio.sleep(min(10,target-time.time()))
                    continue
                previous = target-(BAR if ready else WARMUP*BAR)
                kind = 'rotation_candle' if ready else 'rotation_warmup'
                try:
                    data = await pair(session,previous,target)
                except (aiohttp.ClientError,TimeoutError,ValueError,TypeError,KeyError) as exc:
                    tape.record('binance','candle_error',[target,str(exc)[:200]])
                    tape.seal()
                    await asyncio.sleep(5)
                    continue
                tape.record('binance',kind,[target,data])
                tape.seal()
                print(json.dumps(dict(event=kind,closed_utc_sec=target,receipt_ns=tape.last)),flush=True)
                ready = True
                target += BAR
            reason = 'stop-file' if (root/'STOP').exists() else 'deadline'
    finally:
        tape.seal()
        tape.catalog.close()
        (root/'summary.json').write_text(json.dumps(dict(reason=reason,start=start_wall,
            end=time.time(),files=tape.shard,counts=tape.counts)),encoding='utf-8')


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root',type=Path)
    parser.add_argument('--seconds',type=int,default=60)
    args = parser.parse_args()
    if not 1<=args.seconds<=240*86400:
        parser.error('Supply a positive collection duration of up to 240 days.')
    asyncio.run(collect(args.root,args.seconds))
