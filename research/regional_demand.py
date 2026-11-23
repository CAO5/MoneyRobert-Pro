"""Test regional fiat demand after executable FX conversion; trade only DOGEUSDT."""
import csv
import math
from collections import deque
from pathlib import Path
from statistics import median

from independent_round import book


def run(local,forex,global_book,rule,fee):
    past = deque(maxlen=60)
    cash = peak = 100.
    dd = units = entered = wins = completed = misses = ready = 0
    cooldown = -1
    for sec in range(len(local)-1):
        regional,fx,now = local[sec],forex[sec],global_book[sec]
        after = global_book[sec+1]
        signal = False
        if sec%10==0:
            premium = None
            if regional and fx and now and min(regional[3]*regional[1]/fx[2],fx[4],now[4]*now[2])>=25:
                premium = regional[1]/fx[2]/now[2]-1
            valid = [value for value in past if value is not None]
            if premium is not None and len(valid)>=30:
                ready += 1
                signal = premium>median(valid)+.0015 if rule=='innovation' else premium>.0015
            past.append(premium)
        if units:
            if now:
                equity = cash+units*now[1]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec-entered>=300 or dd>=.08) and after and after[3]>=units:
                proceeds = units*after[1]*(1-fee)
                cash += proceeds
                completed += 1
                wins += proceeds>basis
                units = 0
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
                cooldown = sec+61
        elif signal and sec>cooldown and dd<.08 and sec<len(local)-350:
            quantity = math.floor(25/now[2])
            if not after or quantity<1 or quantity>after[4] or quantity*after[2]*(1+fee)>cash:
                misses += 1
                continue
            units = quantity
            basis = units*after[2]*(1+fee)
            cash -= basis
            entered = sec+1
    return ready,completed,wins,round(cash-100,6) if not units else None,round(dd*100,4),misses,units


if __name__=='__main__':
    with Path(__file__).with_name('regional_demand_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('utc_day','rule','fee','valid_ticks','completed','wins','net_usdt','dd_pct','misses','residual'))
        for month in (8,9,10):
            day = f'2023-{month:02}-01'
            books = [book(day,pair) for pair in ('DOGETRY','USDTTRY','DOGEUSDT')]
            for rule in ('innovation','level'):
                for fee in (.001,.002):
                    record = (day,rule,fee,*run(*books,rule,fee))
                    writer.writerow(record)
                    print(*record,flush=True)
