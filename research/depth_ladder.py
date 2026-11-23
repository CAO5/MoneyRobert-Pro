"""DOGE hourly Gate L2 archives: screen thin vs dense near-touch supply."""
import bisect
import csv
import gzip
import math
import tempfile
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path


CACHE = Path(tempfile.gettempdir())/'doge-gate-l2'


def fetch(hour):
    CACHE.mkdir(exist_ok=True)
    name = f'DOGE_USDT-{hour}.csv.gz'
    path = CACHE/name
    if not path.exists():
        for attempt in range(4):
            try:
                with urllib.request.urlopen(f'https://download.gatedata.org/spot/orderbooks/{hour[:6]}/{name}',timeout=18) as stream:
                    data = stream.read()
                gzip.decompress(data)
                path.write_bytes(data)
                break
            except (OSError,EOFError):
                if attempt==3:
                    raise
                time.sleep(1+attempt)
    return path


def replay_hour(hour):
    origin = int(datetime.strptime(hour,'%Y%m%d%H').replace(tzinfo=timezone.utc).timestamp())
    levels = [{},{}]
    prices = [[],[]]
    initialized = False
    last = None
    second = -1
    states = [None]*3600

    def snapshot():
        asks,bids = prices
        if len(asks)<5 or len(bids)<5 or asks[0]<=bids[-1]:
            return None
        return (bids[-1],asks[0],levels[1][bids[-1]],levels[0][asks[0]],
                (asks[4]-asks[0])/asks[0],(bids[-1]-bids[-5])/bids[-1])

    with gzip.open(fetch(hour),'rt',newline='') as stream:
        for stamp,side,action,price,amount,seq,count in csv.reader(stream):
            slot = int(side)-1
            key,volume = float(price),float(amount)
            tick = int(float(stamp))-origin
            if tick>=3600:
                break
            if tick<0 or tick<second:
                raise ValueError(f'Timestamp discontinuity: {hour} {stamp}')
            if tick!=second and second>=0 and initialized:
                value = snapshot()
                for pos in range(second,min(tick,second+2)):
                    states[pos] = value
            second = tick
            if action=='set':
                if initialized or (last is not None and last!=int(seq)):
                    raise ValueError('Unexpected snapshot reset')
                levels[slot][key] = volume
                last = int(seq)
                continue
            if not initialized:
                if not all(levels):
                    raise ValueError('Snapshot missing')
                prices = [sorted(side_levels) for side_levels in levels]
                initialized = True
            if int(seq)!=last+1 or int(count)<1 or action not in ('make','take'):
                raise ValueError(f'Invalid update sequence: {hour}/{seq}')
            last = int(seq)+int(count)-1
            old = levels[slot].get(key,0.)
            new = old+(volume if action=='make' else -volume)
            if new < -1e-6:
                raise ValueError(f'Negative depth: {hour}/{seq} {new}')
            if new<=1e-8:
                if old:
                    del levels[slot][key]
                    prices[slot].pop(bisect.bisect_left(prices[slot],key))
            else:
                if not old:
                    bisect.insort(prices[slot],key)
                levels[slot][key] = new
    if not initialized:
        raise ValueError('No increments')
    for pos in range(second,min(3600,second+2)):
        states[pos] = snapshot()
    states[0] = None
    return states


def simulate(states,side,fee):
    cash = peak = 100.
    qty = last = entered = wins = trades = 0
    dd = basis = 0.
    for sec in range(len(states)-1):
        now,after = states[sec],states[sec+1]
        if qty:
            if now:
                equity = cash+qty*now[0]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec-entered>=120 or dd>=.08) and after and after[2]>=qty:
                proceeds = qty*after[0]*(1-fee)
                cash += proceeds
                trades += 1
                wins += proceeds>basis
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
                qty = 0
                last = sec+1
            continue
        if not (now and after) or sec%5 or sec-last<60 or sec>len(states)-250 or dd>=.08:
            continue
        sparse,dense = now[4:] if side=='ask_gap' else now[4:][::-1]
        if sparse<.0005 or sparse<4*dense or now[1]/now[0]>1.0005:
            continue
        units = math.floor(25/now[1])
        if not units or units>after[3] or units*after[1]*(1+fee)>cash:
            continue
        qty = units
        basis = units*after[1]*(1+fee)
        cash -= basis
        entered = sec+1
    return trades,wins,round(cash-100,6) if not qty else None,round(100*dd,4),qty


def main():
    day = '20240601'
    hours = [f'{day}{hour:02}' for hour in range(24)]
    with ThreadPoolExecutor(4) as pool:
        list(pool.map(fetch,hours))
    states = [state for hour in hours for state in replay_hour(hour)]
    with Path(__file__).with_name('depth_ladder_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','signal','fee','valid_seconds','trades','wins','net_usdt','dd_pct','residual'))
        for side in ('ask_gap','bid_gap'):
            for fee in (.001,.002):
                record = (day,side,fee,sum(state is not None for state in states),*simulate(states,side,fee))
                writer.writerow(record)
                print(*record,flush=True)


if __name__=='__main__':
    main()
