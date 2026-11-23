"""Observe per second; enter paired DOGE spot/linear perp only on a large premium."""
import csv
import math
from pathlib import Path

from independent_round import book


def replay(day):
    spot = book(day,'DOGEUSDT')
    perp = book(day,'DOGEUSDT','binance-futures')
    pnl = []
    qty = misses = eligible = 0
    start = debit = 0.
    cash = peak = 100.
    dd = 0.
    liquidate = False
    for sec in range(86400-1):
        local, hedge = spot[sec],perp[sec]
        if not (local and hedge):
            continue
        later, future = spot[sec+1],perp[sec+1]
        if qty:
            liquidation = qty*(local[1]*.999-hedge[2]*1.0005)-debit
            equity = cash+liquidation
            peak = max(peak,equity)
            dd = max(dd,1-equity/peak)
            liquidate |= (sec-start>=300 or hedge[2]/local[1]<1.0005 or dd>=.08)
            if liquidate and later and future and min(later[3],future[4])>=qty:
                gain = qty*(later[1]*.999-future[2]*1.0005)-debit
                pnl.append(gain)
                cash += gain
                qty = 0
                liquidate = False
            continue
        if dd>=.08 or sec%3600>=3250 or sec%3600<30:
            continue
        eligible += 1
        if hedge[1]/local[2]<1.005:
            continue
        size = math.floor(20/local[2])
        if not later or not future or min(later[4],future[3])<size or not size:
            misses += 1
            continue
        qty = size
        debit = qty*(later[2]*1.001-future[1]*.9995)
        start = sec+1
    return day,eligible,len(pnl),sum(gain>0 for gain in pnl),round(sum(pnl),5),round(dd*100,4),misses,int(qty>0)


if __name__=='__main__':
    with Path(__file__).with_name('hedged_basis_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','eligible','trades','wins','net_usdt','max_dd_pct','entry_miss','open_at_end'))
        for month in range(1,9):
            result = replay(f'2026-{month:02}-01')
            writer.writerow(result)
            print(*result,flush=True)
