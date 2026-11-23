"""BTC/ETH spot-only research replay. UTC, no API keys or orders."""
import calendar
import csv
import hashlib
import io
import math
import sys
import tempfile
import urllib.request
import zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

SYMBOLS = ('BTCUSDT', 'ETHUSDT')
BASE = 'https://s3-ap-northeast-1.amazonaws.com/data.binance.vision/data/spot/monthly/klines'


def month_data(key):
    symbol, month = key
    name = f'{symbol}-4h-{month}.zip'
    url = f'{BASE}/{symbol}/4h/{name}'
    path = Path(tempfile.gettempdir()) / 'moneyrobert-spot-v1' / name
    path.parent.mkdir(exist_ok=True)
    if not path.exists():
        with urllib.request.urlopen(url, timeout=35) as response:
            payload = response.read()
        with urllib.request.urlopen(url + '.CHECKSUM', timeout=35) as response:
            expected = response.read().decode().split()[0]
        if hashlib.sha256(payload).hexdigest() != expected:
            raise ValueError(name)
        path.write_bytes(payload)
    with zipfile.ZipFile(path) as archive:
        rows = csv.reader(io.StringIO(archive.read(name[:-4]+'.csv').decode()))
        return [(int(row[0]) // (1000000 if len(row[0]) == 16 else 1000), float(row[1]), float(row[4])) for row in rows]


def run(data, cost, passive, ledger=None):
    cash, units, basis, entered = [0.5]*2, [0.]*2, [0.]*2, [0]*2
    trades = []
    peak, drawdown = 1., 0.
    for index in range(31*6, len(data[0])):
        equity = 0.
        final = index == len(data[0])-1
        for slot, bars in enumerate(data):
            stamp, opening, close = bars[index]
            history = [bar[2] for bar in bars[index-120:index]]
            last = history[-1]
            trend = last > sum(history)/120
            buy = passive or (trend and last < history[-2] < history[-3])
            sell = not passive and (not trend or last >= sum(history[-6:])/6 or index-entered[slot] >= 12)
            if units[slot] and (sell or final):
                proceeds = units[slot]*(opening if sell else close)*(1-cost)
                net_return = proceeds/basis[slot]-1
                trades.append(net_return)
                if ledger is not None:
                    ledger.writerow([SYMBOLS[slot], bars[entered[slot]][0], stamp if sell else stamp+14400, round(net_return*100, 5), round((proceeds-basis[slot])*100, 5)])
                cash[slot] += proceeds
                units[slot] = 0.
            elif not units[slot] and buy and not final:
                basis[slot] = cash[slot]*0.25
                units[slot] = basis[slot]/(opening*(1+cost))
                cash[slot] -= basis[slot]
                entered[slot] = index
            equity += cash[slot]+units[slot]*close
        peak = max(peak, equity)
        drawdown = max(drawdown, 1-equity/peak)
    losses = -sum(min(value, 0) for value in trades)
    return dict(net_pct=round(100*(sum(cash)-1), 2), max_dd_pct=round(100*drawdown, 2),
                trades=len(trades), wins=sum(value>0 for value in trades),
                mean_trade_pct=round(100*sum(trades)/len(trades), 3) if trades else None,
                profit_factor=round(sum(max(value, 0) for value in trades)/losses, 2) if losses else None)


def main(year):
    months = [f'{year-1}-12'] + [f'{year}-{month:02}' for month in range(1, 13)]
    with ThreadPoolExecutor(max_workers=8) as pool:
        chunks = list(pool.map(month_data, [(symbol, month) for symbol in SYMBOLS for month in months]))
    data = [[bar for chunk in chunks[offset:offset+13] for bar in chunk] for offset in (0, 13)]
    expected = list(range(calendar.timegm((year-1,12,1,0,0,0)), calendar.timegm((year+1,1,1,0,0,0)), 14400))
    for bars in data:
        if [bar[0] for bar in bars] != expected or not all(math.isfinite(price) and price>0 for _, opening, close in bars for price in (opening,close)):
            raise ValueError('Invalid or incomplete candles; refusing to score')
    for label, cost, passive in [('baseline', .0015, False), ('double_cost', .003, False), ('25pct_hold', .0015, True)]:
        if label == 'baseline':
            path = Path(tempfile.gettempdir()) / f'moneyrobert-spot-trades-{year}.csv'
            with path.open('w', newline='') as output:
                ledger = csv.writer(output)
                ledger.writerow(['symbol','entry_epoch_utc','exit_epoch_utc','trade_net_pct','pnl_pct_initial_equity'])
                result = run(data, cost, passive, ledger)
            print('Trade log:', path)
        else:
            result = run(data, cost, passive)
        print(year, label, result)


if __name__ == '__main__':
    main(int(sys.argv[1]) if len(sys.argv)>1 else 2025)
