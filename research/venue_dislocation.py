"""Prefrozen DOGE spot dislocation test. Observe three venues; trade only Binance."""
import csv
import math
from pathlib import Path

from independent_round import DAYS, book


def simulate(day):
    home = book(day,'DOGEUSDT')
    peer_a = book(day,'DOGE-USDT','okex')
    peer_b = book(day,'DOGEUSDT','bybit-spot')
    cash = peak = 100.
    dd = 0.
    units = unresolved = 0
    entry = cost = 0.
    exiting = False
    profits = []
    eligible = 0
    for sec in range(86400-1):
        local = home[sec]
        first,second = peer_a[sec],peer_b[sec]
        following = home[sec+1]
        if units:
            if local:
                equity = cash+units*local[1]*.999
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            exiting |= (sec-entry>=60 or dd>=.08 or
                        bool(first and second and local and local[1]>=min(first[1],second[1])))
            if exiting and following and following[3]>=units:
                proceeds = units*following[1]*.999
                cash += proceeds
                profits.append(proceeds-cost)
                units = 0
                exiting = False
            continue
        if sec>=86330 or dd>=.08 or not (local and first and second):
            continue
        eligible += 1
        if min(first[1],second[1])/local[2]<1.0035 or not following:
            continue
        units = math.floor(25/local[2])
        if units>following[4] or not units:
            units = 0
            continue
        cost = units*following[2]*1.001
        cash -= cost
        entry = sec+1
    unresolved += bool(units)
    return day,eligible,len(profits),sum(value>0 for value in profits),round(sum(profits),6),round(dd*100,4),unresolved


if __name__=='__main__':
    with Path(__file__).with_name('venue_dislocation_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','eligible_seconds','trades','wins','net_usdt','max_dd_pct','unresolved'))
        for date_string in DAYS:
            result = simulate(date_string)
            writer.writerow(result)
            print(*result,flush=True)
