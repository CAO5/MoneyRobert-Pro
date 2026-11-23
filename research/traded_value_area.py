"""DOGE spot minute proxy for prior-day value acceptance; independent of grid exits."""
import csv
import io
import math
from collections import defaultdict, deque
from pathlib import Path

from position_divergence import archive
from settlement_rotation import MONTHS


def profile(day):
    weights = defaultdict(float)
    for _,_,_,_,close,notional in day:
        key = math.floor(math.log(close)/math.log(1.0025))
        weights[key] += notional
    values = sorted(weights)
    center = max(range(len(values)),key=lambda offset:weights[values[offset]])
    left = right = center
    included = weights[values[center]]
    target = .7*sum(weights.values())
    while included<target:
        down = weights[values[left-1]] if left else -1
        up = weights[values[right+1]] if right+1<len(values) else -1
        if down>=up:
            left -= 1
            included += down
        else:
            right += 1
            included += up
    return 1.0025**values[left],1.0025**(values[center]+.5),1.0025**(values[right]+1)


def bars(month):
    raw = archive(f'spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-{month}.zip')
    return [(int(row[0])//1000,*map(float,(row[1],row[2],row[3],row[4],row[7]))) for row in csv.reader(io.StringIO(raw))]


def run(data,rule,fee):
    cash = peak = 100.
    dd = qty = until = basis = 0
    gains = []
    closes = deque(maxlen=4)
    ranges = None
    for index,(stamp,opening,high,low,close,_) in enumerate(data):
        if index and index%1440==0:
            ranges = profile(data[index-1440:index])
            closes.clear()
        if qty and (index>=until or dd>=.08):
            proceeds = qty*opening*(1-fee)
            cash += proceeds
            gains.append(proceeds-basis)
            qty = 0
        elif not qty and dd<.08 and ranges and len(closes)==4 and index+30<len(data):
            bottom,center,top = ranges
            level = bottom if rule=='reaccept' else top
            hit = closes[0]<level and all(val>=level for val in list(closes)[1:3])
            if hit and (rule!='reaccept' or closes[-2]<center):
                qty = math.floor(25/opening)
                basis = qty*opening*(1+fee)
                cash -= basis
                until = index+30
        peak = max(peak,cash+qty*opening*(1-fee))
        dd = max(dd,1-(cash+qty*low*(1-fee))/peak)
        peak = max(peak,cash+qty*close*(1-fee))
        closes.append(close)
    return len(gains),sum(value>0 for value in gains),round(cash-100,5),round(100*dd,4)


def main():
    months = [bars(month) for month in MONTHS]
    with Path(__file__).with_name('traded_value_area_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('period','rule','fee','trades','wins','net_usdt','dd_pct'))
        for period,chunks in [('formation',months[:2]),('forward_screen',months[2:])]:
            period_bars = [row for chunk in chunks for row in chunk]
            if period_bars[0][0]%86400 or len(period_bars)%1440 or any(after[0]-before[0]!=60 for before,after in zip(period_bars,period_bars[1:])):
                raise ValueError('Invalid calendar')
            for rule in ('reaccept','leave_value'):
                for fee in (0,.001,.002):
                    record = (period,rule,fee,*run(period_bars,rule,fee))
                    writer.writerow(record)
                    print(*record)


if __name__=='__main__':
    main()
