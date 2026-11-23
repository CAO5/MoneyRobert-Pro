"""Relative participation of large vs small marketable trades, not participant IDs."""
import csv
import math
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book


def windows(day):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    groups = [[0.]*6 for _ in range(1440)]
    for row in rows(day,'DOGEUSDT','trades'):
        minute = (int(row[3])//1000000-origin)//60
        notional = float(row[6])*float(row[7])
        if not 0<=minute<1440 or not 0<notional or row[5] not in ('buy','sell'):
            continue
        pos = 0 if notional>=1000 else 3 if notional<=100 else None
        if pos is not None:
            group = groups[minute]
            group[pos] += notional
            group[pos+1] += notional if row[5]=='buy' else 0.
            group[pos+2] += 1
    return groups


def replay(quotes, groups, rule, fee):
    eligible = set()
    for minute in range(1,len(groups)):
        big,bought,nb,small,buy_small,ns = [first+second for first,second in zip(groups[minute-1],groups[minute])]
        if nb>=5 and ns>=25:
            large = bought/big
            retail = buy_small/small
            if (large>=.75 and retail<=.4) if rule=='large_buys' else (retail>=.75 and large<=.4):
                eligible.add(minute*60+59)
    cash = peak = 100.
    qty = end = paid = dd = wins = trades = 0
    for sec,now in enumerate(quotes[:-1]):
        after = quotes[sec+1]
        if qty:
            if now:
                equity = cash+qty*now[1]*(1-fee)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec>=end or dd>=.08) and after and after[3]>=qty:
                value = qty*after[1]*(1-fee)
                trades += 1
                wins += value>paid
                cash += value
                qty = 0
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
        elif sec in eligible and dd<.08 and sec<len(quotes)-370 and now and after:
            size = math.floor(25/now[2])
            if size>0 and size<=after[4] and size*after[2]*(1+fee)<=cash:
                qty = size
                paid = qty*after[2]*(1+fee)
                cash -= paid
                end = sec+301
    return len(eligible),trades,wins,round(cash-100,6) if not qty else None,round(100*dd,4),qty


if __name__=='__main__':
    with Path(__file__).with_name('ticket_composition_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','rule','fee','signals','trades','wins','net_usdt','dd_pct','residual'))
        for month in (8,9,10):
            day = f'2023-{month:02}-01'
            quotes = book(day,'DOGEUSDT')
            groups = windows(day)
            for rule in ('large_buys','small_buys'):
                for fee in (.001,.002):
                    result = (day,rule,fee,*replay(quotes,groups,rule,fee))
                    writer.writerow(result)
                    print(*result,flush=True)
