"""Frozen DOGE minute-level turnover-clock candidates, screening only."""
import csv
import io
import math
from pathlib import Path

from position_divergence import archive


def run(data, mode, fee):
    cash = peak = 100.
    qty = end = 0
    gains = []
    drawdown = balance = 0.
    for index,(opening,low,close,volume,buy) in enumerate(data):
        if qty and index==end:
            proceeds = qty*opening*(1-fee)
            cash += proceeds
            gains.append(proceeds-basis)
            qty = 0
        if index and not qty and drawdown<.08 and index<len(data)-31:
            if trigger:
                qty = math.floor(25/opening)
                basis = qty*opening*(1+fee)
                cash -= basis
                end = index+30
        drawdown = max(drawdown,1-(cash+qty*low*(1-fee))/peak)
        peak = max(peak,cash+qty*close*(1-fee))
        if index%60==0:
            balance = 0.
            initial = opening
        balance += 2*buy-volume
        trigger = index%60==59 and (
            (balance>0 and close<initial*.995) if mode=='inventory' else
            (balance<0 and close>initial*1.005))
    return len(gains),sum(gain>0 for gain in gains),round(sum(gains),5),round(drawdown*100,4)


if __name__=='__main__':
    raw = archive('spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-2024-09.zip')
    records = list(csv.reader(io.StringIO(raw)))
    stamps = [int(row[0]) for row in records]
    if len(stamps)!=43200 or any(second-first!=60000 for first,second in zip(stamps,stamps[1:])):
        raise ValueError('Incomplete month')
    data = [tuple(float(row[index]) for index in (1,3,4,7,10)) for row in records]
    with Path(__file__).with_name('volume_clock_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('month','rule','fee','trades','wins','net_usdt','drawdown_pct'))
        for mode in ('inventory','resilience'):
            for fee in (0,.001,.002):
                row = ('2024-09',mode,fee,*run(data,mode,fee))
                writer.writerow(row)
                print(*row)
