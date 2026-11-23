"""Paired DOGE spot execution replay; never places orders."""
import csv
import statistics
import sys
from collections import defaultdict
from datetime import date

from hf_pressure import CACHE, rows


DAYS = ('2025-11-01', '2025-12-01', '2026-01-01', '2026-02-01')
FEE = .001


def executed(quantity, price, ahead, sells):
    filled = 0.
    for trade_price, size in sells:
        if trade_price > price:
            continue
        if trade_price == price:
            consumed = min(size, ahead)
            ahead -= consumed
            size -= consumed
        else:
            ahead = 0.
        filled += min(quantity-filled, size)
        if filled >= quantity:
            break
    return filled


def replay(day, selective=False):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    quotes = [None]*86400
    sells = defaultdict(list)
    sell_qty = [0.]*86400
    for row in rows(day,'DOGEUSDT','quotes'):
        stamp = int(row[3])
        sec = stamp//1000000-origin
        if 0<=sec<86400 and (not quotes[sec] or quotes[sec][0]<=stamp):
            size, ask, bid, depth = map(float,row[4:8])
            quotes[sec] = (stamp,bid,ask,size,depth)
    for row in rows(day,'DOGEUSDT','trades'):
        local = int(row[3])//1000000-origin
        exchange = int(row[2])//1000000-origin
        if row[5]=='sell' and 0<=local<86400:
            sell_qty[local] += float(row[7])
        if row[5]=='sell' and 31<=local%60<36 and exchange>=local-local%60+31:
            sells[local//60].append((float(row[6]),float(row[7])))
    for sec in range(1,len(quotes)):
        if not quotes[sec]:
            quotes[sec] = quotes[sec-1]

    def fresh(sec):
        quote = quotes[sec]
        return quote and 0<quote[1]<quote[2] and min(quote[3:])>0 and (origin+sec+1)*1000000-quote[0]<=2000000

    for sec in range(29,86400,60):
        if not fresh(sec) or not fresh(sec+1):
            yield ('missing_entry',0,0,0)
            continue
        limit = quotes[sec][1]
        _, bid, ask, depth, ahead = quotes[sec+1]
        qty = 25/quotes[sec][2]
        if depth<qty:
            yield ('missing_entry',0,0,0)
            continue
        if selective and (quotes[sec][4]+qty>sum(sell_qty[sec-4:sec+1]) or
                          quotes[sec][2]/limit-1<.0001):
            yield ('immediate',0,0,0)
            continue
        filled = executed(qty,limit,ahead,sells[sec//60]) if bid==limit else 0.
        remain = qty-filled
        fallback = ask
        if remain>0:
            for later in range(sec+7,sec+18):
                if fresh(later) and quotes[later][3]>=remain:
                    fallback = quotes[later][2]
                    break
            else:
                yield ('unresolved',filled/qty,0,0)
                continue
        saving = (ask-limit)*filled+(ask-fallback)*remain
        yield ('posted',filled/qty,10000*saving/(ask*qty),saving*(1+FEE))


def main():
    CACHE.mkdir(exist_ok=True)
    selective = '--selective' in sys.argv
    dates = ('2026-03-01','2026-04-01','2026-05-01') if selective else DAYS
    filename = 'selective-entry-events.csv' if selective else 'passive-entry-events.csv'
    with (CACHE/filename).open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('date','minute_utc','status','maker_fraction','saved_bp','saved_usdt'))
        for day in dates:
            events = list(replay(day,selective))
            writer.writerows((day,minute,*event) for minute,event in enumerate(events))
            paired = [event for event in events if event[0] in ('immediate','posted')]
            posted = [event for event in paired if event[0]=='posted']
            print(day,'paired/posted/partial/full/unresolved',len(paired),len(posted),
                  sum(0<event[1]<1 for event in posted),sum(event[1]==1 for event in posted),
                  sum(event[0]=='unresolved' for event in events),'better/equal/worse',
                  sum(event[2]>0 for event in paired),sum(event[2]==0 for event in paired),
                  sum(event[2]<0 for event in paired),'mean bp / saved USDT',
                  round(statistics.mean(event[2] for event in paired),4),
                  round(sum(event[3] for event in paired),4),flush=True)


if __name__=='__main__':
    main()
