"""Non-compulsory deep resting spot bids. Research only, no trading API."""
import csv
import math
from collections import defaultdict
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book


DAYS = ('2024-01-01','2024-02-01','2024-03-01')
SYMBOL = 'DOGEUSDT'


def seller_tape(day):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    result = defaultdict(list)
    for row in rows(day,SYMBOL,'trades'):
        if row[5]=='sell':
            local = int(row[3])//1000000-origin
            exchange = int(row[2])//1000000-origin
            if 0<=local<86400:
                result[local].append((exchange,float(row[6]),float(row[7])))
    return result


def replay(quotes,tape,fee,mode):
    cash = peak = 100.
    drawdown = 0.
    trades = []
    blocked = posted = missed = 0
    residual = 0.
    for decision in range(29,len(quotes)-260,60):
        if decision<=blocked or drawdown>=.08:
            continue
        signal, arrival = quotes[decision],quotes[decision+1]
        if not signal or not arrival:
            continue
        limit = math.floor((signal[1]+signal[2])*.5*.9955*100000)/100000
        size = math.floor(25/limit)
        if limit>=arrival[2] or size<1 or size*limit*(1+fee)>cash:
            continue
        posted += 1
        filled = 0.
        basis = 0.
        first = None
        active = decision+2
        deadline = active+120
        blocked = deadline
        for sec in range(active,deadline):
            for exchange,price,amount in tape.get(sec,()):
                if active<=exchange<=sec and price<limit:
                    acquired = min(size-filled,amount)
                    if acquired<=0:
                        continue
                    if first is None:
                        first = sec
                    filled += acquired
                    if mode=='resting':
                        cost = acquired*limit*(1+fee)
                        basis += cost
                        cash -= cost
            if filled and mode=='resting' and quotes[sec]:
                equity = cash+filled*quotes[sec][1]*(1-fee)
                peak = max(peak,equity)
                drawdown = max(drawdown,1-equity/peak)
            if filled>=size or drawdown>=.08:
                break
        if not filled:
            continue
        if mode=='reactive':
            after = quotes[first+1]
            if not after or after[4]<size or size*after[2]*(1+fee)>cash:
                missed += 1
                continue
            filled = size
            basis = filled*after[2]*(1+fee)
            cash -= basis
            mark_from = first+1
        else:
            mark_from = sec+1
        for later in range(mark_from,len(quotes)):
            quote = quotes[later]
            if not quote:
                continue
            equity = cash+filled*quote[1]*(1-fee)
            peak = max(peak,equity)
            drawdown = max(drawdown,1-equity/peak)
            if later<first+120 and drawdown<.08:
                continue
            following = quotes[later+1] if later+1<len(quotes) else None
            if not following or following[3]<filled:
                continue
            proceeds = filled*following[1]*(1-fee)
            cash += proceeds
            drawdown = max(drawdown,1-cash/peak)
            peak = max(peak,cash)
            trades.append((decision,first,later+1,filled,basis,proceeds-basis))
            blocked = max(blocked,later+1)
            break
        else:
            residual = filled
            break
    return (posted,len(trades),sum(row[-1]>0 for row in trades),
            round(cash-100,6) if not residual else None, round(100*drawdown,4),residual,missed),trades


def main():
    root = Path(__file__).parent
    with (root/'liquidity_insurance_results.csv').open('w',newline='') as output, (root/'liquidity_insurance_trades.csv').open('w',newline='') as ledger:
        writer,details = csv.writer(output),csv.writer(ledger)
        writer.writerow(('day','mode','fee','attempts','trades','wins','net_usdt','dd_pct','residual','missed'))
        details.writerow(('day','mode','fee','intent_sec','trigger_sec','exit_sec','quantity','basis','net_pnl'))
        for day in DAYS:
            quotes,tape = book(day,SYMBOL),seller_tape(day)
            for mode in ('resting','reactive'):
                for fee in (.001,.002):
                    summary,trades = replay(quotes,tape,fee,mode)
                    writer.writerow((day,mode,fee,*summary))
                    details.writerows((day,mode,fee,*row) for row in trades)
                    print(day,mode,fee,*summary,flush=True)


if __name__=='__main__':
    main()
