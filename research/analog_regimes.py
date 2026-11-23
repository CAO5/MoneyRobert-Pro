"""Chronological analogue screen, trained once. Minute prices only; no live endpoints."""
import calendar
import csv
import heapq
import io
import math
import statistics
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from position_divergence import archive


@dataclass(frozen=True)
class Bar:
    stamp: int
    opening: float
    high: float
    low: float
    close: float
    notional: float
    count: int


def month(number):
    raw = archive(f'spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-2023-{number:02}.zip')
    rows = [Bar(int(row[0])//1000,*map(float,row[1:5]),float(row[7]),int(row[8]))
            for row in csv.reader(io.StringIO(raw))]
    start = int(datetime(2023,number,1,tzinfo=timezone.utc).timestamp())
    minutes = 1440*calendar.monthrange(2023,number)[1]
    expected = set(range(start,start+60*minutes,60))
    stamps = [row.stamp for row in rows]
    missing = expected.difference(stamps)
    if (stamps!=sorted(set(stamps)) or set(stamps)-expected or
            (missing and (number!=3 or len(missing)!=80 or max(missing)-min(missing)!=79*60))):
        raise ValueError(f'Incomplete archive for {number}')
    return rows


def state(data, index, mode):
    window = data[index-61:index-1]
    blocks = [window[pos:pos+15] for pos in range(0,60,15)]
    moves = [math.log(block[-1].close/block[0].opening) for block in blocks]
    dispersion = sum(abs(math.log(bar.close/bar.opening)) for bar in window)
    if mode=='path':
        return (*[move/max(dispersion,.00001) for move in moves],math.log1p(dispersion*100))
    volume = [sum(bar.notional for bar in block) for block in blocks]
    counts = [sum(bar.count for bar in block) for block in blocks]
    return (math.log((1+volume[-1])/(1+sum(volume)/4)),
            math.log((1+counts[-1])/(1+sum(counts)/4)),
            math.log1p(sum(volume)/max(1,sum(counts))),
            sum(moves)/max(.00001,dispersion), math.log1p(dispersion*100))


def sample(data, mode):
    for index in range(62,len(data)-30):
        if data[index].stamp%1800==0 and data[index+30].stamp-data[index-61].stamp==91*60:
            yield index,state(data,index,mode)


def train(data, mode):
    samples = [(features,data[index+30].opening/data[index].opening-1)
               for index,features in sample(data,mode)]
    scales = [max(.000001,statistics.quantiles(column,n=4)[2]-statistics.quantiles(column,n=4)[0])
              for column in zip(*(features for features,_ in samples))]
    return scales,[(tuple(value/scale for value,scale in zip(features,scales)),outcome)
                   for features,outcome in samples]


def predict(features, model):
    scales,samples = model
    first,second,third,fourth,fifth = [value/scale for value,scale in zip(features,scales)]
    closest = heapq.nsmallest(48,(((a-first)**2+(b-second)**2+(c-third)**2+
                                    (d-fourth)**2+(e-fifth)**2,idx,ret)
                                   for idx,((a,b,c,d,e),ret) in enumerate(samples)))
    returns = [ret for _,_,ret in closest]
    return statistics.mean(returns),sum(ret>(1.001/.999-1) for ret in returns)/len(returns)


def replay(data, schedule, fee):
    cash = peak = 100.
    quantity = end = drawdown = 0
    ledger = []
    for index,bar in enumerate(data):
        if quantity and (index==end or drawdown>=.08):
            proceeds = quantity*bar.opening*(1-fee)
            cash += proceeds
            ledger.append((data[start].stamp,bar.stamp,proceeds-cost))
            quantity = 0
        if not quantity and index in schedule and drawdown<.08:
            quantity = math.floor(25/bar.opening)
            cost = quantity*bar.opening*(1+fee)
            if cost>cash:
                raise ValueError('Cash budget exceeded')
            cash -= cost
            start,end = index,index+30
        peak = max(peak,cash+quantity*bar.opening*(1-fee))
        drawdown = max(drawdown,1-(cash+quantity*bar.low*(1-fee))/peak)
        peak = max(peak,cash+quantity*bar.close*(1-fee))
    if quantity or not math.isclose(cash-100,sum(record[2] for record in ledger),abs_tol=1e-8):
        raise ValueError('Unreconciled account')
    return ledger,drawdown


def main():
    formation = [bar for number in range(2,5) for bar in month(number)]
    screen = [bar for number in range(5,8) for bar in month(number)]
    folder = Path(__file__).parent
    with (folder/'analog_regime_results.csv').open('w',newline='') as output, (folder/'analog_regime_signals.csv').open('w',newline='') as details:
        summary,signals = csv.writer(output),csv.writer(details)
        summary.writerow(('period','mode','fee','signals','completed','wins','net_usdt','dd_pct','label_count'))
        signals.writerow(('mode','intent_utc_epoch','past_mean','past_net_win_fraction'))
        for mode in ('path','liquidity'):
            model = train(formation,mode)
            schedule = set()
            for index,features in sample(screen,mode):
                mean,hit = predict(features,model)
                if mean>1.002/.998-1 and hit>.55:
                    signals.writerow((mode,screen[index].stamp,mean,hit))
                    schedule.add(index)
            for fee in (0,.001,.002):
                ledger,dd = replay(screen,schedule,fee)
                row = ('2023-05_to_07',mode,fee,len(schedule),len(ledger),sum(pnl>0 for _,_,pnl in ledger),
                       round(sum(pnl for _,_,pnl in ledger),6),round(dd*100,4),len(model[1]))
                summary.writerow(row)
                print(*row,flush=True)


if __name__=='__main__':
    main()
