"""DOGE public-quote screen for rejection vs acceptance of a half-cent level."""
import csv
import math
from pathlib import Path

from independent_round import book


def screen(quotes, mode, fee):
    cash = peak = 100.
    dd = 0.
    qty = wins = count = 0
    entered = last_signal = -1000
    for sec in range(31,len(quotes)-1):
        now, following = quotes[sec], quotes[sec+1]
        if qty:
            if now:
                equity = cash+qty*now[1]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec-entered>=120 or dd>=.08) and following and following[3]>=qty:
                proceeds = qty*following[1]*(1-fee)
                cash += proceeds
                wins += proceeds>basis
                count += 1
                qty = 0
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
            continue
        if dd>=.08 or sec-last_signal<=150 or sec>=len(quotes)-200 or not now or not following:
            continue
        past = quotes[sec-30:sec+1:5]
        if not all(past):
            continue
        level = round(now[1]/.005)*.005
        if not level or not level*1.0003<=now[1]<=level*1.0008:
            continue
        if mode=='reclaim':
            signal = past[0][1]>level and min(quote[2] for quote in past[1:-1])<level*.9997
        else:
            signal = past[0][2]<level*.9997 and all(quote[1]>level for quote in past[-3:])
        if signal:
            size = math.floor(25/now[2])
            if size<1 or size>following[4] or size*following[2]*(1+fee)>cash:
                continue
            qty = size
            basis = qty*following[2]*(1+fee)
            cash -= basis
            entered = sec+1
            last_signal = sec
    return count,wins,round(cash-100,5) if not qty else None,round(100*dd,4),int(bool(qty))


def main():
    with Path(__file__).with_name('round_level_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','mode','fee','trades','wins','net_usdt','dd_pct','unresolved'))
        for month in range(1,13):
            day = f'2025-{month:02}-01'
            quotes = book(day,'DOGEUSDT')
            for mode in ('reclaim','acceptance'):
                for fee in (.001,.002):
                    writer.writerow((day,mode,fee,*screen(quotes,mode,fee)))


if __name__=='__main__':
    main()
