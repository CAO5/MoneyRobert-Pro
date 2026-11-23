"""Post-settlement DOGE spot screen; archive-rate publication time is not a fill feed."""
import csv
import io
import math
from calendar import monthrange
from datetime import datetime, timezone
from pathlib import Path

from position_divergence import archive


MONTHS = ('2023-10','2023-11','2023-12','2024-01')


def inputs(month):
    raw = archive(f'spot/monthly/klines/DOGEUSDT/1m/DOGEUSDT-1m-{month}.zip')
    bars = [(int(row[0])//1000,*map(float,(row[1],row[3],row[4]))) for row in csv.reader(io.StringIO(raw))]
    origin = int(datetime.fromisoformat(month+'-01').replace(tzinfo=timezone.utc).timestamp())
    days = monthrange(*map(int,month.split('-')))[1]
    if [bar[0] for bar in bars]!=list(range(origin,origin+days*86400,60)):
        raise ValueError('Incomplete spot minutes')
    raw = archive(f'futures/um/monthly/fundingRate/DOGEUSDT/DOGEUSDT-fundingRate-{month}.zip')
    rates = [(int(row['calc_time'])//60000*60+360,float(row['last_funding_rate']))
             for row in csv.DictReader(io.StringIO(raw))]
    if len(rates)!=len(dict(rates)) or any(not math.isfinite(rate) for _,rate in rates):
        raise ValueError('Invalid funding history')
    return bars,rates


def screen(bars, rates, rule, fee):
    cash = peak = 100.
    qty = basis = dd = end = wins = total = 0
    for index,(stamp,opening,low,close) in enumerate(bars):
        if qty and (stamp>=end or dd>=.08):
            value = qty*opening*(1-fee)
            cash += value
            total += 1
            wins += value>basis
            qty = 0
        rate = rates.get(stamp)
        if not qty and dd<.08 and rate is not None and index<len(bars)-60:
            hit = rule=='clock' or (rate>=.0002 if rule=='long_payers' else rate<0)
            if hit:
                qty = math.floor(25/opening)
                basis = qty*opening*(1+fee)
                cash -= basis
                end = stamp+3600
        peak = max(peak,cash+qty*opening*(1-fee))
        dd = max(dd,1-(cash+qty*low*(1-fee))/peak)
        peak = max(peak,cash+qty*close*(1-fee))
    return total,wins,round(cash-100,6),round(100*dd,4)


def main():
    loaded = [inputs(month) for month in MONTHS]
    with Path(__file__).with_name('settlement_rotation_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('period','rule','fee','settlements','trades','wins','net_usdt','dd_pct'))
        for period,blocks in [('discovery',loaded[:2]),('forward_screen',loaded[2:])]:
            bars = [bar for data,_ in blocks for bar in data]
            rates = dict(pair for _,data in blocks for pair in data)
            for rule in ('long_payers','short_payers','clock'):
                for fee in (0,.001,.002):
                    record = (period,rule,fee,len(rates),*screen(bars,rates,rule,fee))
                    writer.writerow(record)
                    print(*record,flush=True)


if __name__=='__main__':
    main()
