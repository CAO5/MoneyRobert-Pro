"""Record a separate public BTC/ETH quote mirror for availability checks."""
import argparse
import asyncio
import json
import shutil
import time
from pathlib import Path

import aiohttp

from forward_tape import GIB, PortfolioTape, public_metadata, receive


SYMBOLS = ('BTCUSDT','ETHUSDT')
URL = 'wss://stream.binance.com:443/stream?streams=btcusdt@bookTicker/ethusdt@bookTicker'


async def collect(root, seconds):
    tape = PortfolioTape(root)
    started = time.monotonic()
    reason = 'interrupted'
    try:
        timeout = aiohttp.ClientTimeout(total=None,sock_connect=10)
        async with aiohttp.ClientSession(timeout=timeout,trust_env=True) as session:
            tasks = [asyncio.create_task(receive('binance',URL,session,tape)),
                     asyncio.create_task(public_metadata(session,tape,SYMBOLS,False))]
            try:
                while True:
                    for task in tasks:
                        if task.done():
                            task.result()
                            raise RuntimeError('A data task stopped')
                    elapsed = time.monotonic()-started
                    if elapsed>=seconds:
                        reason = 'deadline'
                    elif (root/'STOP').exists():
                        reason = 'stop-file'
                    elif tape.size()>=8*GIB-2*1024**2 or shutil.disk_usage(root).free<10*GIB:
                        reason = 'storage-limit'
                    if reason!='interrupted':
                        break
                    if elapsed>=35 and tape.shard==0:
                        tape.flush()
                        tape.seal()
                    await asyncio.sleep(1)
            finally:
                for task in tasks:
                    task.cancel()
                await asyncio.gather(*tasks,return_exceptions=True)
    finally:
        tape.flush()
        tape.seal()
        tape.catalog.close()
        (root/'summary.json').write_text(json.dumps(dict(reason=reason,files=tape.shard,
            bytes=tape.sealed,counts=tape.counts)),encoding='utf-8')


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    parser.add_argument('--seconds',type=int,default=60)
    args = parser.parse_args()
    if not 1<=args.seconds<=240*86400:
        parser.error('Supply a positive collection duration of up to 240 days.')
    asyncio.run(collect(args.directory,args.seconds))
