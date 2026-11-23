"""Compare stop timing on matched entries and on independent continuous accounts."""
import json
import math
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, decisions, epoch, load, simulate
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


def paired_stops(bars, weekly_signals, reference, end, cost):
    origin = bars[SYMBOLS[0]][0].time
    stop = (epoch(end)-origin)//BAR
    fee, slippage = cost
    sell_factor = (1-slippage)*(1-fee)
    rows = []
    for trade in reference['ledger']:
        if trade['reason'] != 'position_stop':
            continue
        symbol = trade['symbol']
        series = bars[symbol]
        exited = (trade['exit']-origin)//BAR
        weekly_exit = next((i for i in range(exited, stop)
                            if symbol in weekly_signals.get(i, ((), ()))[1]), stop)
        exit_price = series[weekly_exit].open if weekly_exit < stop else series[stop-1].close
        quantity = trade['quantity']
        weekly_net = quantity*exit_price*sell_factor-trade['debit']
        assert math.isclose(trade['credit'], quantity*series[exited].open*sell_factor, abs_tol=1e-8)
        lowest_price = min((b.low for b in series[exited:weekly_exit]), default=exit_price)
        worst_held = quantity*min(lowest_price, exit_price)*sell_factor-trade['debit']
        rows.append(dict(symbol=symbol, entry=trade['entry'], fixed_exit=trade['exit'],
                         weekly_exit=origin+weekly_exit*BAR,
                         delta_usdt=weekly_net-trade['pnl'], fixed_net_usdt=trade['pnl'],
                         weekly_net_usdt=weekly_net, worst_while_waiting_usdt=worst_held))
    return dict(trades=len(rows), weekly_profitable=sum(r['weekly_net_usdt']>0 for r in rows),
                weekly_better=sum(r['delta_usdt']>0 for r in rows),
                fixed_net_usdt=math.fsum(r['fixed_net_usdt'] for r in rows),
                weekly_net_usdt=math.fsum(r['weekly_net_usdt'] for r in rows),
                saved_by_stop_usdt=math.fsum(-min(0.,r['delta_usdt']) for r in rows),
                forgone_by_stop_usdt=math.fsum(max(0.,r['delta_usdt']) for r in rows),
                worst_while_waiting_usdt=min((r['worst_while_waiting_usdt'] for r in rows), default=None),
                pairs=rows)


def review(bars, previous):
    weekly = decisions(bars)['rotation']
    smaller = {(i, symbol): (20.5, .03) for i, (picks, _) in weekly.items() for symbol in picks}
    variants = {
        'one_close': {'stop_confirmations': 1},
        'two_closes': {'stop_confirmations': 2},
        'no_position_stop': {'stop_confirmations': None},
        'no_position_stop_small_position': {'stop_confirmations': None, 'entry_limits': smaller},
        'no_position_stop_early_pause': {'stop_confirmations': None, 'account_pause_drawdown': .06},
    }
    periods = tuple(previous)
    result = {}
    for period in periods:
        start, end = period.split('/')
        results = {}
        for label, cost in [('base', BASE_COST), ('stress', STRESS_COST)]:
            cases = {}
            for name, kwargs in variants.items():
                cases[name] = annotate(simulate(bars, {'rotation': weekly}, 'rotation', start, end, cost,
                                       profit_protection=True, **kwargs))
            fixed = cases['one_close']
            assert all(fixed[k]==v for k,v in previous[period]['profit_protection'][label].items())
            results[label] = dict(accounts=cases, matched_stops=paired_stops(bars, weekly, fixed, end, cost))
        result[period] = results
    return result


if __name__ == '__main__':
    path = Path(__file__).with_name('weekly_rotation_results.json')
    baseline = json.loads(path.read_text(encoding='utf-8'))
    bars = load('2020-03-01')
    print(json.dumps(review(bars, {**baseline['earlier'], **baseline['recent']}), indent=2))
