"""Reprice fixed trades with official minute candles after a one-minute delay."""
import argparse
import json
import math
import time
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from slow_spot_replay import STEPS
from weekly_rotation_review import BASE_COST, STRESS_COST


PERIOD = '2022-01-01/2026-11-01'
ENDPOINT = 'https://data-api.binance.vision/api/v3/klines'


def minute_open(point, *, endpoint=ENDPOINT):
    symbol, stamp = point
    query = urllib.parse.urlencode(dict(symbol=symbol, interval='1m', startTime=stamp*1000, limit=1))
    for attempt in range(3):
        try:
            with urllib.request.urlopen(f'{endpoint}?{query}', timeout=15) as response:
                rows = json.load(response)
            if len(rows) != 1 or rows[0][0] != stamp*1000 or not float(rows[0][1])>0:
                raise ValueError(f'Expected exact minute at {point}')
            return float(rows[0][1])
        except OSError:
            if attempt == 2:
                raise
            time.sleep(1+attempt)


def execution_minutes(trade):
    closing_offset = -60 if trade['reason'] == 'end' else 60
    return trade['entry']+60, trade['exit']+closing_offset


def reprice(accounts, reader=minute_open, workers=8):
    points = sorted({(row['symbol'],stamp) for a in accounts.values()
                     for row in a['ledger'] for stamp in execution_minutes(row)})
    with ThreadPoolExecutor(workers) as pool:
        opens = dict(zip(points,pool.map(reader,points)))
    repriced = {}
    for mode,cost in [('base',BASE_COST),('stress',STRESS_COST)]:
        a = accounts[mode]
        fee,slip = cost
        rows = []
        for trade in a['ledger']:
            symbol = trade['symbol']
            buy,sell = [opens[symbol,stamp] for stamp in execution_minutes(trade)]
            price = buy*(1+slip)
            step = STEPS[symbol]
            units = math.floor(12.25/price/step)*step
            nominal = units*price
            proceeds = units*sell*(1-slip)
            if nominal < 10 or proceeds < 10:
                raise ValueError('Delayed price crosses the simulated order minimum')
            net = proceeds*(1-fee)-nominal*(1+fee)
            rows.append(dict(symbol=symbol, entry=trade['entry'], exit=trade['exit'],
                             minute_buy=buy, minute_sell=sell, original_net=trade['pnl'],
                             delayed_net=net))
        repriced[mode] = dict(trades=len(rows), original_wins=a['wins'], original_usdt=a['net'],
                              delayed_wins=sum(r['delayed_net']>0 for r in rows),
                              delayed_usdt=math.fsum(r['delayed_net'] for r in rows), rows=rows)
    return dict(unique_minute_opens=len(opens), results=repriced)


def review(results):
    accounts = {mode: r['accounts']['lock'] for mode,r in results[PERIOD].items()}
    return dict(endpoint=ENDPOINT, **reprice(accounts))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path)
    args = parser.parse_args()
    data = json.loads(args.results.read_bytes())
    print(json.dumps(review(data.get('results',data)),indent=2))
