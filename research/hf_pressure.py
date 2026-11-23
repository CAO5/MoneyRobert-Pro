"""Read-only Binance spot event study, UTC. No keys, order endpoints or dependencies."""
import csv
import gzip
import shutil
import statistics
import sys
import tempfile
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import date
from pathlib import Path


DAYS = ('2025-07-01', '2025-08-01', '2025-09-01')
SYMBOLS = ('DOGEUSDT', 'BTCUSDT')
CACHE = Path(tempfile.gettempdir()) / 'moneyrobert-hf-v1'


def source(key, venue='binance'):
    day, symbol, kind = key
    prefix = '' if venue=='binance' else f'{venue}-'
    path = CACHE / f'{prefix}{day}-{symbol}-{kind}.csv.gz'
    url = f'https://datasets.tardis.dev/v1/{venue}/{kind}/{day.replace("-", "/")}/{symbol}.csv.gz'
    if not path.exists() or path.stat().st_size<20:
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=45) as response, path.with_suffix('.part').open('wb') as output:
                    shutil.copyfileobj(response, output)
                with gzip.open(path.with_suffix('.part'),'rb') as archive:
                    while archive.read(1024*1024):
                        pass
                path.with_suffix('.part').replace(path)
                break
            except (OSError, EOFError):
                if attempt == 2:
                    raise
                time.sleep(1)
    return path


def rows(day, symbol, kind, venue='binance'):
    with gzip.open(source((day, symbol, kind),venue), 'rt', newline='') as stream:
        reader = csv.reader(stream)
        next(reader)
        yield from reader


def load_market(day, symbol):
    origin = (date.fromisoformat(day)-date(1970, 1, 1)).days * 86400
    quotes = [None]*86400
    buy, total, count = [0.]*86400, [0.]*86400, [0]*86400
    for row in rows(day, symbol, 'quotes'):
        stamp = int(row[3])
        sec = stamp//1000000-origin
        size, ask, bid, depth = map(float, row[4:8])
        if 0 <= sec < 86400 and (quotes[sec] is None or stamp >= quotes[sec][0]):
            quotes[sec] = (stamp, bid, ask, size, depth) if 0 < bid < ask and min(size,depth)>0 else (stamp,0,0,0,0)
    for row in rows(day, symbol, 'trades'):
        sec = int(row[3])//1000000-origin
        if 0 <= sec < 86400:
            value = float(row[6])*float(row[7])
            total[sec] += value
            buy[sec] += value if row[5]=='buy' else 0
            count[sec] += 1
    for sec in range(1,86400):
        if quotes[sec] is None:
            quotes[sec] = quotes[sec-1]

    return origin, quotes, buy, total, count


def replay(day, symbol):
    origin, quotes, buy, total, count = load_market(day, symbol)

    def fresh(sec):
        quote = quotes[sec]
        return quote and quote[1] and (origin+sec+1)*1000000-quote[0] <= 2000000

    records = []
    controls = []
    blocked = -1
    for sec in range(2,86385):
        if sec <= blocked or not fresh(sec) or not fresh(sec-2):
            continue
        volume = total[sec]+total[sec-1]
        old, now = quotes[sec-2], quotes[sec]
        pressure = volume>0 and count[sec]+count[sec-1]>=5 and buy[sec]+buy[sec-1]>=0.8*volume
        if not pressure:
            if sec%60!=30:
                continue
            label = 'quiet_control'
        elif now[2]>old[2]:
            label = 'advance'
        elif now[2]==old[2] and now[3]>=old[3]:
            label = 'absorb_proxy'
        else:
            continue
        entry = sec+1
        if label != 'quiet_control':
            blocked = entry
        if not fresh(entry) or quotes[entry][2]*quotes[entry][3]<25:
            continue
        exit_sec = entry+5
        quantity = 25/quotes[entry][2]
        while exit_sec<86400 and (not fresh(exit_sec) or quotes[exit_sec][4]<quantity):
            exit_sec += 1
        if exit_sec==86400:
            raise ValueError('Unresolved exit')
        if label != 'quiet_control':
            blocked = exit_sec
        gross = quotes[exit_sec][1]/quotes[entry][2]
        target = controls if label == 'quiet_control' else records
        target.append((day,symbol,label,origin+sec+1,gross-1,gross*(1-.001)/(1+.001)-1))
    return records + controls


def main():
    CACHE.mkdir(exist_ok=True)
    keys = [(day,symbol,kind) for day in DAYS for symbol in SYMBOLS for kind in ('trades','quotes')]
    with ThreadPoolExecutor(6) as pool:
        list(pool.map(source,keys))
    if '--fetch' in sys.argv:
        return
    result = []
    for day in DAYS:
        for symbol in SYMBOLS:
            result.extend(replay(day,symbol))
    with (CACHE/'events.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','symbol','group','decision_utc_epoch','gross_return','net_return'))
        writer.writerows(result)
    for symbol in SYMBOLS:
        for group in ('advance','absorb_proxy','quiet_control'):
            for days in (DAYS[:1],DAYS[1:]):
                events = [event for event in result if event[0] in days and event[1:3]==(symbol,group)]
                if not events:
                    print(symbol,group,days,'no events')
                    continue
                equity = peak = 100.
                dd = 0.
                executed = 0
                for event in events:
                    if equity < 25.025 or dd >= .08:
                        break
                    equity += 25.025*event[-1]
                    executed += 1
                    peak = max(equity,peak)
                    dd = max(dd,1-equity/peak)
                print(symbol,group,days,len(events),*(round(value,3) for value in (
                    10000*statistics.mean(event[-2] for event in events),
                    10000*statistics.mean(event[-1] for event in events),
                    100*sum(event[-1]>0 for event in events)/len(events),100*dd)), executed)


if __name__ == '__main__':
    main()
