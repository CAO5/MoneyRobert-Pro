"""Screen change in BTC option extrinsic premium as a public DOGE spot signal."""
import csv
import math
from collections import deque
from datetime import date
from pathlib import Path
from statistics import median

from hf_pressure import rows
from independent_round import book


CONTRACTS = (('2024-01-01','26JAN24',42000),('2024-02-01','23FEB24',42000),('2024-03-01','29MAR24',60000))


def option(day, symbol):
    start = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    ticks = [None]*86400
    for row in rows(day,symbol,'quotes','deribit'):
        stamp = int(row[3])
        sec = stamp//1000000-start
        if 0<=sec<86400 and (ticks[sec] is None or stamp>=ticks[sec][0]):
            ask_size,ask,bid,bid_size = (float(value or 0) for value in row[4:8])
            ticks[sec] = (stamp,bid,ask) if 0<bid<ask and min(ask_size,bid_size)>0 else (stamp,0,0)
    latest = None
    for sec,quote in enumerate(ticks):
        if quote is not None:
            latest = quote
        ticks[sec] = latest if latest and latest[1] and (start+sec+1)*1000000-latest[0]<=60_000_000 else None
    return ticks


def run(btc,doge,call,put,strike,direction,fee):
    cash = peak = 100.
    window = deque(maxlen=20)
    units = enter = wins = trades = ready = dd = 0
    for sec in range(len(doge)-1):
        now,after = doge[sec],doge[sec+1]
        hit = False
        if sec%30==0:
            extrinsic = None
            if btc[sec] and call[sec] and put[sec]:
                ref = (btc[sec][1]+btc[sec][2])/2
                intrinsic = abs(1-strike/ref)
                lower = call[sec][1]+put[sec][1]-intrinsic
                upper = call[sec][2]+put[sec][2]-intrinsic
                if lower>0 and upper/lower<1.25:
                    extrinsic = (lower,upper)
            good = [item for item in window if item is not None]
            if extrinsic and len(good)>=14:
                ready += 1
                hit = (extrinsic[0]>median(item[1] for item in good)*1.02 if direction=='rise' else
                       extrinsic[1]<median(item[0] for item in good)*.98)
            window.append(extrinsic)
        if units:
            if now:
                equity = cash+units*now[1]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec-enter>=600 or dd>=.08) and after and after[3]>=units:
                proceeds = units*after[1]*(1-fee)
                cash += proceeds
                trades += 1
                wins += proceeds>cost
                units = 0
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
        elif hit and dd<.08 and now and after and sec<len(doge)-700:
            size = math.floor(25/now[2])
            if 0<size<=after[4] and size*after[2]*(1+fee)<=cash:
                units = size
                cost = units*after[2]*(1+fee)
                cash -= cost
                enter = sec+1
    return ready,trades,wins,round(cash-100,6) if not units else None,round(100*dd,4),units


if __name__=='__main__':
    with Path(__file__).with_name('option_insurance_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','signal','fee','ready','trades','wins','net_usdt','dd_pct','residual'))
        for day,expiry,strike in CONTRACTS:
            btc,doge = (book(day,asset) for asset in ('BTCUSDT','DOGEUSDT'))
            call,put = (option(day,f'BTC-{expiry}-{strike}-{side}') for side in ('C','P'))
            for direction in ('rise','fall'):
                for fee in (.001,.002):
                    record = (day,direction,fee,*run(btc,doge,call,put,strike,direction,fee))
                    writer.writerow(record)
                    print(*record,flush=True)
