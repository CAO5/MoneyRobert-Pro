"""Independent spot sleeves, no orders. Run: python research/small_account.py [year]."""
import calendar
import math
import sys
from concurrent.futures import ThreadPoolExecutor
from statistics import mean

from spot_pullback import month_data


def replay(bars, cost, hold=False):
    cash = peak = 100.0
    units = basis = drawdown = 0.0
    trades = []
    for index in range(186, len(bars)):
        stamp, opening, close = bars[index]
        if stamp % 86400 == 0:
            closes = [bars[offset][2] for offset in range(index-115, index, 6)]
            active = hold or closes[-1] > mean(closes)
            if active and not units:
                basis = cash * 0.25
                units = basis / (opening * (1+cost))
                cash -= basis
            elif not active and units:
                proceeds = units * opening * (1-cost)
                cash += proceeds
                trades.append(proceeds/basis-1)
                units = 0
        if index == len(bars)-1 and units:
            proceeds = units * close * (1-cost)
            cash += proceeds
            trades.append(proceeds/basis-1)
            units = 0
        equity = cash + units*close
        peak = max(peak, equity)
        drawdown = max(drawdown, 1-equity/peak)
    return round(cash-100, 2), round(drawdown*100, 2), sum(ret>0 for ret in trades), len(trades)


def main(year, last_month=12):
    if not 1 <= last_month <= 12:
        raise ValueError('Expected a month from 1 to 12')
    months = [f'{year-1}-12'] + [f'{year}-{month:02}' for month in range(1, last_month+1)]
    end = calendar.timegm((year+last_month//12,last_month%12+1,1,0,0,0))
    expected = list(range(calendar.timegm((year-1,12,1,0,0,0)), end, 14400))
    for symbol in ('BTCUSDT', 'ETHUSDT', 'DOGEUSDT'):
        with ThreadPoolExecutor(6) as pool:
            bars = [bar for chunk in pool.map(month_data, [(symbol, month) for month in months]) for bar in chunk]
        if [bar[0] for bar in bars] != expected or not all(math.isfinite(price) and price>0 for _, opening, close in bars for price in (opening,close)):
            raise ValueError('Invalid candles')
        print(symbol, 'net%, drawdown%, wins, trades:', replay(bars,.0015), 'double cost:', replay(bars,.003), 'hold:', replay(bars,.0015,True))


if __name__ == '__main__':
    main(*(int(value) for value in sys.argv[1:])) if len(sys.argv)>1 else main(2025)
