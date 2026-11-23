"""Test previously defined rotation candidates on the earlier archive, preserving gaps."""
import json

from slow_spot_replay import BAR, SYMBOLS, epoch, load, rotation_decisions, simulate, utc
from weekly_rotation_entry_review import entry_rules
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


WINDOWS = (('2018-01-01', '2019-01-01'), ('2019-01-01', '2020-01-01'),
           ('2018-01-01', '2020-03-01'), ('2018-01-01', '2026-11-01'))


def review(bars):
    weekly = rotation_decisions(bars)
    entries = entry_rules(bars, weekly)
    variants = {
        'original': (weekly, {}),
        'profit_protection': (weekly, {'profit_protection': True}),
        'two_closes': (weekly, {'profit_protection': True, 'stop_confirmations': 2}),
        'no_position_stop': (weekly, {'profit_protection': True, 'stop_confirmations': None}),
        'pullback_reclaim': (entries['pullback_reclaim'], {'profit_protection': True}),
    }
    coverage = {}
    for start, end in WINDOWS:
        first, stop = epoch(start), epoch(end)
        missing = [utc(b.time) for b in bars[SYMBOLS[0]] if first <= b.time < stop and b.close is None]
        coverage[start+'/'+end] = dict(scheduled_bars=(stop-first)//BAR, missing_4h_utc=missing,
            accounts={
                name: {label: annotate(simulate(bars, {'rotation': signal}, 'rotation',
                                                start, end, cost, **params))
                       for label,cost in [('base',BASE_COST), ('stress',STRESS_COST)]}
                for name,(signal,params) in variants.items()
            })
    return coverage


if __name__ == '__main__':
    data = load('2017-12-01', allow_aligned_gaps=True)
    print(json.dumps(review(data), indent=2))
