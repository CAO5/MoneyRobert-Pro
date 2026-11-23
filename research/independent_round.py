"""Independent spot candidates. Public quotes only; never submits orders."""
import csv
import math
from datetime import date
from pathlib import Path

from hf_pressure import CACHE, rows


PAIRS = ('DOGEUSDT','BTCUSDT','DOGEBTC')
DAYS = tuple(f'2025-{month:02}-01' for month in range(1,7))
STEPS = {'DOGEUSDT':1.,'BTCUSDT':.00001,'DOGEBTC':1.}
FEE = .001


def book(day, symbol, venue='binance'):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    timeline = [None]*86400
    previous = None
    for row in rows(day,symbol,'quotes',venue):
        stamp = int(row[3])
        sec = stamp//1000000-origin
        if 0<=sec<86400 and (timeline[sec] is None or stamp>=timeline[sec][0]):
            ask_size,ask,bid,bid_size = map(float,row[4:8])
            timeline[sec] = (stamp,bid,ask,bid_size,ask_size)
    for sec,current in enumerate(timeline):
        if current is not None:
            previous = current
        valid = (previous and (origin+sec+1)*1000000-previous[0]<=2000000 and
                 0<previous[1]<previous[2] and min(previous[3:])>0)
        timeline[sec] = previous if valid else None
    return timeline


def convert(amount, quote, symbol, buy, fee):
    if not quote:
        return None
    _,bid,ask,bid_size,ask_size = quote
    step = STEPS[symbol]
    qty = math.floor((amount/ask if buy else amount)/step)*step
    if qty<=0 or qty>(ask_size if buy else bid_size):
        return None
    return (qty if buy else qty*bid)*(1-fee)


def candidate(name, books):
    doge,btc,cross = (books[pair] for pair in PAIRS)
    net = []
    failed = 0
    cooldown = 0
    for sec in range(31,86320):
        if sec<cooldown or not doge[sec] or not btc[sec]:
            continue
        if name=='leader':
            if not doge[sec-10] or not btc[sec-10]:
                continue
            if btc[sec][1]/btc[sec-10][2]<1.001 or doge[sec][2]>doge[sec-10][1]:
                continue
            legs = [(0,True,1),(0,False,61)]
        else:
            if not cross[sec]:
                continue
            legs = ([(0,True,1),(2,False,2),(1,False,3)] if name=='cycle_doge'
                    else [(1,True,1),(2,True,2),(0,False,3)])
            amount = 25.
            for pair,buy,_ in legs:
                amount = convert(amount,books[PAIRS[pair]][sec],PAIRS[pair],buy,FEE)
                if amount is None:
                    break
            if amount is None or amount<25.025:
                continue
        cooldown = sec+legs[-1][2]+1
        amount = 25.
        for pair,buy,delay in legs:
            amount = convert(amount,books[PAIRS[pair]][sec+delay],PAIRS[pair],buy,FEE)
            if amount is None:
                break
        if amount is None:
            failed += 1
        else:
            net.append(amount-25)
    return net,failed


def main():
    CACHE.mkdir(exist_ok=True)
    out = Path(__file__).with_name('independent_round_results.csv')
    with out.open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','candidate','complete','unresolved','wins','net_usdt','max_dd_pct_from_100_closed'))
        for day in DAYS:
            books = {pair:book(day,pair) for pair in PAIRS}
            for name in ('cycle_doge','cycle_btc','leader'):
                trades,failed = candidate(name,books)
                peak = cash = 100.
                dd = 0.
                for pnl in trades:
                    cash += pnl
                    peak = max(peak,cash)
                    dd = max(dd,1-cash/peak)
                record = (day,name,len(trades),failed,sum(pnl>0 for pnl in trades),round(sum(trades),5),round(dd*100,4))
                writer.writerow(record)
                print(*record,flush=True)


if __name__=='__main__':
    main()
