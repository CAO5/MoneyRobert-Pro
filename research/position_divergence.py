"""Continuous DOGE signal screen; public archives, no order entry. Not an execution backtest."""
import csv
import hashlib
import io
import math
import tempfile
import time
import urllib.request
import zipfile
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timedelta, timezone
from pathlib import Path


ROOT = 'https://s3-ap-northeast-1.amazonaws.com/data.binance.vision/data/'
CACHE = Path(tempfile.gettempdir())/'doge-position-screen'
RULES = ('position_rotation','crowded_short','leverage_exit')


def archive(path):
    CACHE.mkdir(exist_ok=True)
    dest = CACHE/path.split('/')[-1]
    if not dest.exists():
        for attempt in range(3):
            try:
                with urllib.request.urlopen(ROOT+path+'.CHECKSUM',timeout=20) as stream:
                    digest = stream.read().decode().split()[0]
                with urllib.request.urlopen(ROOT+path,timeout=30) as stream:
                    data = stream.read()
                if hashlib.sha256(data).hexdigest()!=digest:
                    raise ValueError('Digest mismatch '+path)
                dest.write_bytes(data)
                break
            except OSError:
                if attempt==2:
                    raise
                time.sleep(1)
    with zipfile.ZipFile(dest) as zipped:
        return zipped.read(zipped.namelist()[0]).decode()


def positions(day):
    path = f'futures/um/daily/metrics/DOGEUSDT/DOGEUSDT-metrics-{day}.zip'
    result = {}
    for row in csv.DictReader(io.StringIO(archive(path))):
        stamp = int(datetime.fromisoformat(row['create_time']).replace(tzinfo=timezone.utc).timestamp())
        try:
            values = tuple(float(row[key]) for key in ('sum_toptrader_long_short_ratio','count_long_short_ratio','sum_open_interest'))
        except ValueError:
            continue
        if all(math.isfinite(value) and value>0 for value in values):
            result[stamp] = values
    return result


def signals(now, before):
    size, accounts, interest = (math.log(new/old) for new,old in zip(now,before))
    return (size>.015 and accounts<0, accounts<-.015 and interest>.003,
            interest<-.01 and size>0)


def screen(bars, metrics, rule, fee):
    cash = peak = 100.
    dd = units = basis = enter = wins = 0
    ledger = []
    for index,(stamp,opening,low,close) in enumerate(bars):
        if units and (stamp-enter>=1800 or dd>=.08 or index==len(bars)-1):
            proceeds = units*opening*(1-fee)
            cash += proceeds
            pnl = proceeds-basis
            ledger.append((enter,stamp,pnl))
            wins += pnl>0
            units = 0
        elif not units and dd<.08 and stamp%300==60 and index<len(bars)-31:
            known = stamp-360
            if known in metrics and known-900 in metrics and signals(metrics[known],metrics[known-900])[rule]:
                units = math.floor(25/opening)
                basis = units*opening*(1+fee)
                cash -= basis
                enter = stamp
        floor = cash+units*low*(1-fee)
        dd = max(dd,1-floor/peak)
        peak = max(peak,cash+units*close*(1-fee))
    return (len(ledger),wins,round(cash-100,5),round(dd*100,4)),ledger


def main():
    first = date(2024,9,1)
    days = [first+timedelta(days=offset) for offset in range(30)]
    with ThreadPoolExecutor(6) as pool:
        metrics = {stamp:value for block in pool.map(positions,days) for stamp,value in block.items()}
    raw = archive('spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-2024-09.zip')
    bars = [(int(row[0])//1000,float(row[1]),float(row[3]),float(row[4])) for row in csv.reader(io.StringIO(raw))]
    origin = int(datetime(2024,9,1,tzinfo=timezone.utc).timestamp())
    if [bar[0] for bar in bars]!=list(range(origin,origin+30*86400,60)):
        raise ValueError('Incomplete minute series')
    with Path(__file__).with_name('position_divergence_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('month','rule','fee','n','wins','net_usdt','drawdown_pct','valid_metric_samples'))
        for rule,name in enumerate(RULES):
            for fee in (0,.001,.002):
                result,_ = screen(bars,metrics,rule,fee)
                writer.writerow(('2024-09',name,fee,*result,len(metrics)))
                print(name,fee,result,len(metrics),flush=True)


if __name__=='__main__':
    main()
