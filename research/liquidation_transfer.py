"""Futures forced executions as exogenous signals; limit exposure to spot inventory."""
import csv
import math
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book


def forced(day):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    signals = {'sell':set(),'buy':set()}
    for row in rows(day,'DOGEUSDT','liquidations','binance-futures'):
        sec = int(row[3])//1000000-origin
        if 0<=sec<86400 and float(row[6])*float(row[7])>=1000:
            signals[row[5]].add(sec)
    return signals


def replay(quotes, times, fee):
    profits = []
    blocked = failures = 0
    for sec in sorted(times):
        if sec<blocked or sec>=86100 or not quotes[sec]:
            continue
        arrival = quotes[sec+1]
        qty = math.floor(25/quotes[sec][2])
        if not arrival or not qty or arrival[4]<qty:
            continue
        for exit_sec in range(sec+181,86400):
            exit_quote = quotes[exit_sec]
            if exit_quote and exit_quote[3]>=qty:
                profits.append(qty*(exit_quote[1]*(1-fee)-arrival[2]*(1+fee)))
                blocked = exit_sec+1
                break
        else:
            failures += 1
            break
    return len(profits),sum(value>0 for value in profits),round(sum(profits),5),failures


if __name__=='__main__':
    with Path(__file__).with_name('liquidation_transfer_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','forced_side','per_side_fee','trades','wins','net_usdt','unresolved'))
        for month in range(1,13):
            day = f'2025-{month:02}-01'
            signals = forced(day)
            quotes = book(day,'DOGEUSDT')
            for side,times in signals.items():
                for fee in (.001,.002):
                    writer.writerow((day,side,fee,*replay(quotes,times,fee)))
            print(day,flush=True)
