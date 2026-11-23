"""Test a subsidy-value proxy vs LTC price leadership. Rejection-only minute screen."""
import csv
import math
from pathlib import Path

from sector_breadth import peer
from session_recurrence import month


def detect(doge, ltc, btc, index):
    end = index-2
    start = end-15
    change = lambda prices: prices[end]/prices[start]-1
    market = change(btc)
    doge_move = doge[end][3]/doge[start][3]-1
    ltc_move = change(ltc)
    prior = 10000*doge[start][3]+2.5*ltc[start]
    mix = (10000*doge[end][3]+2.5*ltc[end])/prior-1
    return abs(doge_move-market)<.001 and ltc_move-market>.005, mix-market


def run(doge, ltc, btc, rule, fee):
    cash = peak = 100.
    dd = qty = basis = end = wins = completed = 0
    for index,(_,opening,low,close) in enumerate(doge):
        if qty and (index==end or dd>=.08):
            proceeds = qty*opening*(1-fee)
            cash += proceeds
            wins += proceeds>basis
            completed += 1
            qty = 0
        if not qty and dd<.08 and index%5==2 and 17<=index<len(doge)-30:
            broad,increment = detect(doge,ltc,btc,index)
            if broad and (rule=='ltc_control' or increment>.001):
                qty = math.floor(25/opening)
                basis = qty*opening*(1+fee)
                cash -= basis
                end = index+30
        peak = max(peak,cash+qty*opening*(1-fee))
        dd = max(dd,1-(cash+qty*low*(1-fee))/peak)
        peak = max(peak,cash+qty*close*(1-fee))
    return completed,wins,round(cash-100,5),round(100*dd,4)


def main():
    with Path(__file__).with_name('mining_mix_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('period','rule','fee','trades','wins','net_usdt','dd_pct'))
        for period,months in [('formation',(7,8,9)),('forward_screen',(10,11))]:
            doge,ltc,btc = [],[],[]
            for number in months:
                bars = month(number)
                stamps = [bar[0] for bar in bars]
                doge.extend(bars)
                ltc.extend(peer('LTCUSDT',number,stamps))
                btc.extend(peer('BTCUSDT',number,stamps))
            for rule in ('subsidy_mix','ltc_control'):
                for fee in (0,.001,.002):
                    record = (period,rule,fee,*run(doge,ltc,btc,rule,fee))
                    writer.writerow(record)
                    print(*record,flush=True)


if __name__=='__main__':
    main()
