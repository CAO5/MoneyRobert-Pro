"""Frozen volatility-event candidates, executable quotes and cash/units ledger."""
import csv
import math
from pathlib import Path

from independent_round import book


DAYS = tuple(f'2025-{month:02}-01' for month in range(1,13))
RULES = ('auction_reset','range_expansion','downside_exhaustion')


def simulate(quotes, rule, fee):
    cash = peak = 100.
    dd = 0.
    entry = cost = units = next_entry = failures = 0
    pending_exit = False
    profits = []
    for sec in range(901,len(quotes)-1):
        current = quotes[sec]
        following = quotes[sec+1]
        if units:
            if current:
                equity = cash+units*current[1]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
                change = units*current[1]*(1-fee)/cost-1
                pending_exit |= change>=.004 or change<=-.005
            pending_exit |= sec-entry>=600 or dd>=.08
            if pending_exit and following and following[3]>=units:
                value = units*following[1]*(1-fee)
                cash += value
                profits.append(value-cost)
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
                units = 0
                pending_exit = False
                next_entry = sec+61
            continue
        if (sec%10 or sec<next_entry or sec>=len(quotes)-670 or dd>=.08
                or not current or current[2]/current[1]>1.0005):
            continue
        past = quotes[sec-900:sec:10]
        if any(not quote for quote in past):
            continue
        prices = [(quote[1]+quote[2])/2 for quote in past]
        mid = (current[1]+current[2])/2
        if rule=='auction_reset':
            hit = sec%28800<600 and mid<prices[-7]*.997
        elif rule=='range_expansion':
            hit = max(prices[:-6])/min(prices[:-6])<1.004 and mid>max(prices[:-6])*1.0015
        else:
            hit = min(prices[-6:])<max(prices[:-6])*.99 and mid>max(prices[-3:])*1.0003
        if not hit:
            continue
        qty = math.floor(25/current[2])
        if not following or qty>following[4] or qty<1 or qty*following[2]*(1+fee)>cash:
            failures += 1
            continue
        units = qty
        cost = qty*following[2]*(1+fee)
        cash -= cost
        entry = sec+1
    return len(profits),sum(profit>0 for profit in profits),round(sum(profits),5),round(dd*100,4),failures,int(units>0)


if __name__=='__main__':
    with Path(__file__).with_name('event_candidate_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','rule','fee_per_side','trades','wins','net_usdt','max_dd_pct','unfilled_entry','unresolved_exit'))
        for day in DAYS:
            quotes = book(day,'DOGEUSDT')
            for rule in RULES:
                for fee in (.001,.002):
                    writer.writerow((day,rule,fee,*simulate(quotes,rule,fee)))
            print(day,flush=True)
