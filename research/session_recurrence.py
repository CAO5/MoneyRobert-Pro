"""Frozen time-of-day candidates: train July-September, screen October-November.

Minute-only rejection screen, not evidence of executable returns.
"""
import csv
import io
import math
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from statistics import median

from position_divergence import archive


def month(number):
    raw = archive(f'spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-2024-{number:02}.zip')
    data = [(int(row[0])//1000, float(row[1]), float(row[3]), float(row[4]))
            for row in csv.reader(io.StringIO(raw))]
    epoch = lambda value: int(datetime(2024,value,1,tzinfo=timezone.utc).timestamp())
    if [bar[0] for bar in data] != list(range(epoch(number),epoch(number+1),60)):
        raise ValueError('Minute coverage mismatch')
    return data


def select_slots(data):
    buckets = defaultdict(list)
    for index in range(0,len(data)-16,15):
        stamp = data[index][0]
        buckets[(stamp%86400)//900].append(data[index+16][1]/data[index+1][1]-1)
    return sorted(buckets,key=lambda slot:(-median(buckets[slot]),slot))[:4]


def replay(data,slots,fee):
    cash = peak = 100.
    drawdown = 0.
    trades = []
    quantity = exit_at = 0
    for index,(stamp,opening,low,close) in enumerate(data):
        if quantity and (index==exit_at or drawdown>=.08):
            proceeds = quantity*opening*(1-fee)
            cash += proceeds
            trades.append(proceeds-paid)
            quantity = 0
        if not quantity and drawdown<.08 and stamp%900==60 and (stamp%86400)//900 in slots and index+15<len(data):
            quantity = math.floor(25/opening)
            paid = quantity*opening*(1+fee)
            if paid>cash:
                raise ValueError('Insufficient cash')
            cash -= paid
            exit_at = index+15
        peak = max(peak,cash+quantity*opening*(1-fee))
        drawdown = max(drawdown,1-(cash+quantity*low*(1-fee))/peak)
        peak = max(peak,cash+quantity*close*(1-fee))
    return len(trades),sum(value>0 for value in trades),round(cash-100,5),round(100*drawdown,4)


def main():
    training = sorted(bar for number in (7,8,9) for bar in month(number))
    slots = select_slots(training)
    follow = month(10)+month(11)
    with Path(__file__).with_name('session_recurrence_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('period','rule','quarter_hour_slots_utc','fee','trades','wins','net_usdt','dd_pct'))
        for period,data in [('formation',training),('forward',follow)]:
            for name,selection in [('trained_clock',slots),('six_hour_clock',[0,24,48,72])]:
                for fee in (0,.001,.002):
                    record = (period,name,';'.join(map(str,selection)),fee,*replay(data,selection,fee))
                    writer.writerow(record)
                    print(*record)


if __name__=='__main__':
    main()
