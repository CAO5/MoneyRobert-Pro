"""Cash-to-cash, two passive legs with strict trade-through evidence. No live trading."""
import csv
import math
from collections import defaultdict
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book


DAYS = ('2024-04-01','2024-05-01','2024-06-01')


def tape(day):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    events = defaultdict(list)
    for row in rows(day,'DOGEUSDT','trades'):
        sec = int(row[3])//1000000-origin
        exchange = int(row[2])//1000000-origin
        if 0<=sec<86400 and row[5] in ('buy','sell'):
            events[sec].append((exchange,row[5],float(row[6]),float(row[7])))
    return events


def fair(home, away, mode):
    mid = (home[1]+home[2])/2
    if mode=='local':
        return mid
    return (mid+(away[1]+away[2])/2)/2


def through(events, start, sec, side, price, remaining):
    amount = sum(size for exchange,aggressor,level,size in events.get(sec,())
                 if start<=exchange<=sec and aggressor==side and
                 (level<price if side=='sell' else level>price))
    return min(remaining,amount)


def replay(home,away,events,mode,fee):
    cash = peak = 100.
    dd = 0.
    blocked = -1
    ledger = []
    offers = 0
    def mark(held,quote):
        nonlocal peak,dd
        if quote or not held:
            equity = cash+(held*quote[1]*(1-fee) if held else 0.)
            peak = max(peak,equity)
            dd = max(dd,1-equity/peak)

    for sec in range(29,len(home)-420,60):
        if sec<=blocked or dd>=.08 or not(home[sec] and away[sec] and home[sec+1]):
            continue
        buy = math.floor(fair(home[sec],away[sec],mode)*.9975*100000)/100000
        limit_qty = math.floor(25/buy)
        if buy>=home[sec+1][2] or limit_qty<1 or limit_qty*buy*(1+fee)>cash:
            continue
        offers += 1
        held = 0.
        start_cash = cash
        for tick in range(sec+2,sec+62):
            added = through(events,sec+2,tick,'sell',buy,limit_qty-held)
            cash -= added*buy*(1+fee)
            held += added
            mark(held,home[tick])
            if held>=limit_qty or dd>=.08:
                break
        blocked = tick
        if not held:
            continue
        acquired = held
        sell = None
        reference = max(buy/.9975,fair(home[tick],away[tick],mode)) if home[tick] and away[tick] else None
        if dd<.08 and reference is not None and home[tick+1]:
            target = math.ceil(reference*1.0025*100000)/100000
            if target>home[tick+1][1]:
                sell = target
        mark(held,home[tick+1])
        exit_start = tick+2
        exit_end = tick+1
        if sell and dd<.08:
            for later in range(exit_start,exit_start+240):
                sold = through(events,exit_start,later,'buy',sell,held)
                cash += sold*sell*(1-fee)
                held -= sold
                mark(held,home[later])
                exit_end = later
                if held<=1e-10 or dd>=.08:
                    break
        residual = max(0.,held)
        if residual>1e-10:
            for later in range(exit_end+2,len(home)):
                quote = home[later]
                mark(held,quote)
                if quote and quote[3]>=held:
                    cash += held*quote[1]*(1-fee)
                    held = 0.
                    exit_end = later
                    mark(0.,quote)
                    break
            else:
                return offers,ledger,dd,held
        blocked = exit_end
        ledger.append((sec,exit_end,acquired,residual,cash-start_cash))
    if not math.isclose(cash-100,sum(row[-1] for row in ledger),abs_tol=1e-8):
        raise ValueError('Cash ledger imbalance')
    return offers,ledger,dd,0.


if __name__=='__main__':
    root = Path(__file__).parent
    with (root/'spread_capture_results.csv').open('w',newline='') as output, (root/'spread_capture_trades.csv').open('w',newline='') as details:
        summary,orders = csv.writer(output),csv.writer(details)
        summary.writerow(('day','reference','fee','offers','roundtrips','wins','forced_sales','net_usdt','dd_pct','open_quantity'))
        orders.writerow(('day','reference','fee','decision_sec','exit_sec','bought_qty','forced_qty','net_usdt'))
        for day in DAYS:
            home,away = book(day,'DOGEUSDT'),book(day,'DOGE-USDT','okex')
            events = tape(day)
            for mode in ('local','two_venue'):
                for fee in (.001,.002):
                    offers,ledger,dd,residual = replay(home,away,events,mode,fee)
                    orders.writerows((day,mode,fee,*row) for row in ledger)
                    record = (day,mode,fee,offers,len(ledger),sum(row[-1]>0 for row in ledger),
                              sum(row[-2]>1e-10 for row in ledger),
                              round(sum(row[-1] for row in ledger),6) if not residual else None,round(dd*100,4),residual)
                    summary.writerow(record)
                    print(*record,flush=True)
