"""Measure whether adding after a pullback improves a half-size weekly entry."""
import json

from slow_spot_replay import load, rotation_decisions, simulate
from weekly_rotation_entry_review import entry_rules
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


WINDOWS = [(f'{year}-01-01',f'{year+1}-01-01') for year in range(2018,2026)] + [
    ('2026-01-01','2026-11-01'), ('2018-01-01','2020-03-01'), ('2022-01-01','2026-11-01'),
    ('2018-01-01','2026-11-01')]


def review(bars):
    weekly = rotation_decisions(bars)
    pullback = entry_rules(bars, weekly)['pullback_reclaim']
    add_signals = {i:picks[0] for i,(picks,_) in pullback.items() if picks}
    small = dict(entry_target=12.25, minimum_order_notional=10.)
    variants = {
        'full_on_monday': (weekly, {}),
        'full_on_reclaim': (pullback, {}),
        'half_on_monday': (weekly, small),
        'half_and_reclaim_add': (weekly, {**small, 'add_signals':add_signals}),
    }
    results = {}
    for start, end in WINDOWS:
        results[start+'/'+end] = {}
        for variant, (signals,params) in variants.items():
            accounts = {}
            for label,cost in [('base',BASE_COST),('stress',STRESS_COST)]:
                account = annotate(simulate(bars, {'rotation':signals}, 'rotation', start, end, cost,
                                           profit_protection=True, **params))
                if 'minimum_order_notional' in params:
                    account['positions_with_two_buys'] = sum(len(t['buy_legs']) == 2 for t in account['ledger'])
                accounts[label] = account
            results[start+'/'+end][variant] = accounts
    return dict(initial_usdt=100, target_per_buy_usdt=12.25,
                simulated_order_minimum_usdt=10, results=results)


if __name__ == '__main__':
    bars = load('2017-12-01', allow_aligned_gaps=True)
    print(json.dumps(review(bars), indent=2))
