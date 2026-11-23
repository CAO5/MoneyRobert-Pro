"""DOGE response to broad meme-sector rotation, using prior completed bars only."""
import csv
import io
import math
from pathlib import Path

from position_divergence import archive
from session_recurrence import month


def peer(symbol,number,stamps):
    raw = archive(f'spot/monthly/klines/{symbol}/1m/{symbol}-1m-2024-{number:02}.zip')
    data = [(int(row[0])//1000,float(row[4])) for row in csv.reader(io.StringIO(raw))]
    if [item[0] for item in data]!=stamps:
        raise ValueError(symbol+' timestamp mismatch')
    return [item[1] for item in data]


def simulate(data,peers,mode,fee):
    qty = exit_at = wins = total = 0
    cash = peak = 100.
    dd = 0.
    for index,(_,opening,low,close) in enumerate(data):
        if qty and (index==exit_at or dd>=.08):
            proceeds = qty*opening*(1-fee)
            cash += proceeds
            wins += proceeds>basis
            total += 1
            qty = 0
        if not qty and dd<.08 and 7<=index<len(data)-15 and index%5==2:
            moves = [prices[index-2]/prices[index-7]-1 for prices in peers]
            doge = data[index-2][3]/data[index-7][3]-1
            hit = min(moves)>.002 and abs(doge)<.001 if mode=='rotation' else max(moves)<-.002 and doge>.001
            if hit:
                qty = math.floor(25/opening)
                basis = qty*opening*(1+fee)
                cash -= basis
                exit_at = index+15
        peak = max(peak,cash+qty*opening*(1-fee))
        dd = max(dd,1-(cash+qty*low*(1-fee))/peak)
        peak = max(peak,cash+qty*close*(1-fee))
    return total,wins,round(cash-100,5),round(100*dd,4)


def main():
    symbols = ('PEPEUSDT','SHIBUSDT','FLOKIUSDT')
    with Path(__file__).with_name('sector_breadth_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('period','rule','fee','trades','wins','net_usdt','dd_pct'))
        for label,months in [('formation',(9,)),('screen',(10,11))]:
            combined = []
            peers = [[] for symbol in symbols]
            for number in months:
                bars = month(number)
                combined.extend(bars)
                stamps = [bar[0] for bar in bars]
                for target,symbol in zip(peers,symbols):
                    target.extend(peer(symbol,number,stamps))
            for mode in ('rotation','decoupling'):
                for fee in (0,.001,.002):
                    record = (label,mode,fee,*simulate(combined,peers,mode,fee))
                    writer.writerow(record)
                    print(*record)


if __name__=='__main__':
    main()
