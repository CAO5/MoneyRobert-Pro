"""Record public OKX trades and best quotes with the existing research recorder."""

import argparse
import asyncio
import json
import sys
from pathlib import Path

import aiohttp

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from okx_behavior_tape import GIB, collect

MARKETS = ('BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'SOL-USDT-SWAP')
URL = 'https://www.okx.com/api/v5/public/instruments'


async def run(root, hours, cap):
    async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=20)) as client:
        async def specification(market):
            async with client.get(URL, params={'instType': 'SWAP', 'instId': market}) as response:
                response.raise_for_status()
                packet = await response.json()
                spec, = packet['data']
                if packet['code'] != '0' or spec['instId'] != market or float(spec['tickSz']) <= 0:
                    raise ValueError(f'Invalid tick specification for {market}')
                return market, spec

        specs = dict(await asyncio.gather(*(specification(market) for market in MARKETS)))
    root.mkdir(parents=True, exist_ok=False)
    (root / 'instruments.json').write_text(json.dumps(specs, indent=2), encoding='utf-8')
    await collect(root / 'raw', hours * 3600, cap * GIB, split_streams=True,
                  instruments=MARKETS, channels=('trades', 'bbo-tbt'),
                  observer=lambda *_: None, receive_timeout=3, ws_compression=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--hours', type=int, default=24, choices=(2, 24))
    parser.add_argument('--max-gib', type=int, default=8, choices=range(1, 9))
    args = parser.parse_args()
    asyncio.run(run(args.directory, args.hours, args.max_gib))
