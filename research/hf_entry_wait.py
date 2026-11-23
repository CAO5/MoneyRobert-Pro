"""Paired execution study for synthetic, clock-scheduled spot buy intentions."""
import csv
import math
import statistics

from hf_pressure import CACHE, DAYS, SYMBOLS, load_market


HOLDOUT = ('2025-10-01', '2025-11-01')
FEE = .001


def pressure(sec, buy, total, count):
    volume = total[sec-1]+total[sec]
    return volume>0 and count[sec-1]+count[sec]>=5 and buy[sec-1]+buy[sec]>=.8*volume


def fill(origin, quotes, start, quantity):
    for sec in range(start, min(len(quotes), start+11)):
        quote = quotes[sec]
        if (quote and quote[1]>0 and quote[3]>=quantity and
                (origin+sec+1)*1000000-quote[0]<=2000000):
            return quote[2]
    return None


def study(day, symbol):
    origin, quotes, buy, total, count = load_market(day,symbol)
    for sec in range(29,86400,60):
        quote = quotes[sec]
        if not quote or not quote[1] or (origin+sec+1)*1000000-quote[0]>2000000:
            yield (day,symbol,origin+sec+1,'no_quote',0,None,None,None)
            continue
        quantity = 25/quote[2]
        delay = 0
        while delay<5 and pressure(sec+delay,buy,total,count):
            delay += 1
        immediate = fill(origin,quotes,sec+1,quantity)
        waited = fill(origin,quotes,sec+delay+1,quantity)
        saving = (immediate-waited)/immediate*10000 if immediate and waited else None
        yield (day,symbol,origin+sec+1,'paired' if saving is not None else 'missing',
               delay,saving,quantity*(immediate-waited)*(1+FEE) if saving is not None else None,
               quantity)


def main():
    CACHE.mkdir(exist_ok=True)
    with (CACHE/'entry-wait-events.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','symbol','decision_utc_epoch','status','wait_seconds','saved_bp','saved_usdt','quantity'))
        for day in DAYS + HOLDOUT:
            for symbol in SYMBOLS:
                events = list(study(day,symbol))
                writer.writerows(events)
                paired = [event for event in events if event[3]=='paired']
                delayed = [event for event in paired if event[4]]
                values = sorted(event[5] for event in delayed)
                if not values:
                    print(day,symbol,'no delayed pairs',flush=True)
                    continue
                print(day,symbol,'pairs/waits/missing_delays',len(paired),len(values),
                      sum(event[4]>0 and event[3]=='missing' for event in events),
                      'better/equal/worse',sum(value>0 for value in values),values.count(0),
                      sum(value<0 for value in values),'mean_bp/p05_bp',
                      round(statistics.mean(values),4),round(values[math.ceil(.05*len(values))-1],4),
                      'total_saved_usdt',round(sum(event[6] for event in paired),4),flush=True)


if __name__ == '__main__':
    main()
