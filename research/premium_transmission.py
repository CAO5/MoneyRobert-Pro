"""DOGE spot follows a BTC derivatives premium innovation; no derivatives orders."""
import csv
import math
from collections import deque
from pathlib import Path
from statistics import median

from independent_round import book


def run(spot, perpetual, doge, fee, side):
    history = deque(maxlen=30)
    cash = peak = 100.
    dd = qty = entry = basis = wins = count = misses = seen = 0
    last = -1000
    for sec in range(len(doge)-1):
        local, after = doge[sec],doge[sec+1]
        impulse = None
        if sec%10==0:
            before, now = spot[sec],perpetual[sec]
            premium = (now[1]+now[2])/(before[1]+before[2])-1 if now and before else None
            good = [value for value in history if value is not None]
            if len(good)>=27 and premium is not None:
                seen += 1
                impulse = premium-median(good)
            history.append(premium)
        if qty:
            if local:
                equity = cash+qty*local[1]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec-entry>=180 or dd>=.08) and after and after[3]>=qty:
                proceeds = qty*after[1]*(1-fee)
                cash += proceeds
                count += 1
                wins += proceeds>basis
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
                qty = 0
                last = sec+1
        elif (impulse is not None and side*impulse>.0005 and local and
              dd<.08 and sec-last>60 and sec<len(doge)-250):
            size = math.floor(25/local[2])
            if not after or not size or size>after[4] or size*after[2]*(1+fee)>cash:
                misses += 1
                continue
            qty = size
            basis = size*after[2]*(1+fee)
            cash -= basis
            entry = sec+1
    return seen,count,wins,round(cash-100,6) if not qty else None,round(dd*100,4),misses,qty


def main():
    with Path(__file__).with_name('premium_transmission_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','side','fee','eligible_ticks','trades','wins','net_usdt','dd_pct','misses','residual'))
        for number in (1,2,3,4):
            day = f'2026-{number:02}-01'
            spot = book(day,'BTCUSDT')
            perpetual = book(day,'BTCUSDT','binance-futures')
            doge = book(day,'DOGEUSDT')
            for side in (1,-1):
                for fee in (.001,.002):
                    record = (day,side,fee,*run(spot,perpetual,doge,fee,side))
                    writer.writerow(record)
                    print(*record,flush=True)


if __name__=='__main__':
    main()
