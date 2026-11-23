"""Compare fixed risk changes to weekly BTC/ETH momentum in separate history windows."""
import argparse
import json
import math
from datetime import datetime, timezone
from statistics import pstdev

from slow_spot_replay import BAR, SYMBOLS, decisions, load, simulate


BASE_COST = (.001, .0005)
STRESS_COST = (.002, .001)


def variants(bars):
    rotation = decisions(bars)['rotation']
    breadth, limits = {}, {}
    for i, (picks, exits) in rotation.items():
        returns = {}
        for symbol, series in bars.items():
            closes = [series[j].close for j in range(i-169, i, 6)]
            daily = [math.log(right/left) for left, right in zip(closes, closes[1:])]
            returns[symbol] = math.fsum(daily)
            distance = max(.03, 2*pstdev(daily))
            limits[i, symbol] = min(24.5, 1.5/distance), distance
        breadth[i] = (picks, exits) if all(value > 0 for value in returns.values()) else ([], list(SYMBOLS))
    smaller = {key: (20.5, .03) for key in limits}
    return {
        'original': (rotation, {}),
        'small_position': (rotation, {'entry_limits': smaller}),
        'breadth': (breadth, {}),
        'volatility_budget': (rotation, {'entry_limits': limits}),
        'profit_protection': (rotation, {'profit_protection': True}),
    }


def annotate(result):
    ledger = result['ledger']
    best = max((row['pnl'] for row in ledger), default=0.)
    result['best_trade_usdt'] = max(best, 0.)
    result['net_without_best_usdt'] = result['net'] - result['best_trade_usdt']
    result['reasons'] = {}
    for row in ledger:
        group = result['reasons'].setdefault(row['reason'], {'trades': 0, 'net_usdt': 0.})
        group['trades'] += 1
        group['net_usdt'] += row['pnl']
    return result


def review(bars, windows):
    rules = variants(bars)
    return {
        f'{start}/{end}': {
            name: {
                label: annotate(simulate(bars, {'rotation': signals}, 'rotation', start, end, cost, **kwargs))
                for label, cost in [('base', BASE_COST), ('stress', STRESS_COST)]
            }
            for name, (signals, kwargs) in rules.items()
        }
        for start, end in windows
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('period', choices=('earlier', 'recent'))
    args = parser.parse_args()
    if args.period == 'earlier':
        market = load('2020-03-01', '2022-01-01')
        later = load('2022-01-01')
        for symbol in SYMBOLS:
            market[symbol].extend(later[symbol])
        windows = [('2020-04-01', '2021-01-01'), ('2021-01-01', '2022-01-01'),
                   ('2020-04-01', '2022-01-01'), ('2020-04-01', '2026-11-01')]
    else:
        market = load()
        windows = [(f'{year}-01-01', f'{year+1}-01-01') for year in range(2022, 2026)]
        windows += [('2026-01-01','2026-11-01'), ('2022-01-01','2026-11-01')]
    print(json.dumps(review(market, windows), indent=2))
