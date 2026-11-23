"""Causal-proxy screen: public chain activity vs DOGE spot. No trading endpoints."""
import csv
import gzip
import io
import math
import statistics
import sys
import tempfile
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

from session_recurrence import month


def blocks(day):
    name = f'blockchair_dogecoin_blocks_{day:%Y%m%d}.tsv.gz'
    folder = Path(tempfile.gettempdir())/'doge-chain-screen'
    folder.mkdir(exist_ok=True)
    dest = folder/name
    for attempt in range(3):
        try:
            if not dest.exists():
                with urllib.request.urlopen('https://gz.blockchair.com/dogecoin/blocks/'+name,timeout=25) as response:
                    data = response.read()
                gzip.decompress(data)
                dest.write_bytes(data)
            break
        except (OSError,EOFError):
            if attempt==2:
                raise
            time.sleep(2)
    raw = gzip.decompress(dest.read_bytes()).decode()
    return [(int(row['id']),int(datetime.fromisoformat(row['time']).replace(tzinfo=timezone.utc).timestamp()),int(row['transaction_count'])-1)
            for row in csv.DictReader(io.StringIO(raw),delimiter='\t')]


def schedule(records,bars):
    ordered = sorted(records)
    origin = bars[0][0]
    relevant = [i for i,(_,stamp,_) in enumerate(ordered) if origin-86400//2<=stamp<=bars[-1][0]+86400//2]
    if not relevant or ordered[0][1]>=origin or ordered[-1][1]<=bars[-1][0]:
        raise ValueError('Missing session boundaries')
    window = ordered[min(relevant):max(relevant)+1]
    if any(right[0]!=left[0]+1 for left,right in zip(window,window[1:])):
        raise ValueError('Nonconsecutive block heights around session')
    counts = [0]*len(bars)
    txs = [0]*len(bars)
    latest = 0
    for index,(_,stamp,_) in enumerate(ordered):
        latest = max(latest,stamp)
        moment = (latest+120-origin)//60+1
        if index>=6 and 0<=moment<len(bars):
            counts[moment] += 1
            txs[moment] += ordered[index-6][2]
    return counts,txs


def run(bars,counts,txs,mode,fee):
    cash = peak = 100.
    qty = end = dd = wins = completed = basis = 0
    for index,(_,opening,low,close) in enumerate(bars):
        if qty and (index==end or dd>=.08):
            proceeds = qty*opening*(1-fee)
            cash += proceeds
            completed += 1
            wins += proceeds>basis
            qty = 0
        if not qty and dd<.08 and index%15==0 and 375<=index<len(bars)-30:
            def rate(start):
                return sum(txs[start:start+15])/max(1,sum(counts[start:start+15]))
            baseline = statistics.median(rate(start) for start in range(index-375,index-15,15))
            count = sum(counts[index-15:index])
            hit = count>=8 and rate(index-15)>2*max(1,baseline) if mode=='activity_burst' else count>=24
            if hit:
                qty = math.floor(25/opening)
                basis = qty*opening*(1+fee)
                end = index+30
                cash -= basis
        peak = max(peak,cash+qty*opening*(1-fee))
        dd = max(dd,1-(cash+qty*low*(1-fee))/peak)
        peak = max(peak,cash+qty*close*(1-fee))
    return completed,wins,round(cash-100,5),round(100*dd,4)


def main(partial=False):
    first = date(2024,10,6 if partial else 1)
    days = 4 if partial else 31
    label = '2024-10-06_to_09_partial' if partial else '2024-10'
    with ThreadPoolExecutor(4) as pool:
        records = [block for group in pool.map(blocks,[first+timedelta(days=offset) for offset in range(-1,days+1)]) for block in group]
    begin = (first-date(1970,1,1)).days*86400
    bars = [bar for bar in month(10) if begin<=bar[0]<begin+days*86400]
    counts,txs = schedule(records,bars)
    filename = 'chain_activity_partial_results.csv' if partial else 'chain_activity_results.csv'
    with Path(__file__).with_name(filename).open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('period','rule','fee','trades','wins','net_usdt','dd_pct','blocks_available'))
        for name in ('activity_burst','issuance_burst'):
            for fee in (0,.001,.002):
                record = (label,name,fee,*run(bars,counts,txs,name,fee),sum(counts))
                writer.writerow(record)
                print(*record,flush=True)


if __name__=='__main__':
    main('--partial' in sys.argv)
