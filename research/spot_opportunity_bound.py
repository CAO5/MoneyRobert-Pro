"""Diagnostic only: hindsight ceiling for long-only taker winners on sampled quotes."""
import csv
from collections import deque
from pathlib import Path

from independent_round import book


def count_bound(quotes, holding, fee):
    candidates = deque()
    winners = 0
    for sec, quote in enumerate(quotes):
        while candidates and candidates[0][0] < sec-holding:
            candidates.popleft()
        if quote is None:
            continue
        if candidates and quote[1]*(1-fee) > candidates[0][1]*(1+fee):
            winners += 1
            candidates.clear()
            continue
        while candidates and candidates[-1][1] >= quote[2]:
            candidates.pop()
        candidates.append((sec,quote[2]))
    return winners


def main():
    with Path(__file__).with_name('spot_opportunity_bound_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','usable_seconds','horizon_sec','fee_each_side','max_disjoint_profitable_trades'))
        for month in range(1,13):
            day = f'2025-{month:02}-01'
            quotes = book(day,'DOGEUSDT')
            valid = sum(quote is not None for quote in quotes)
            for holding in (5,30,120):
                for fee in (.001,.002):
                    writer.writerow((day,valid,holding,fee,count_bound(quotes,holding,fee)))


if __name__=='__main__':
    main()
