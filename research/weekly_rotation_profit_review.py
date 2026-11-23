"""Audit profit opportunities on matched entries and compare two early exits."""
import json
import math
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, load, rotation_decisions, simulate
from weekly_rotation_forecast_review import ACCOUNT_LIMITS
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


NET_TARGET = .06
NET_LOCK = (.03, .015)
MODES = {'original':{}, 'target':{'net_profit_target':NET_TARGET},
         'lock':{'net_profit_lock':NET_LOCK}}


def match(bars, account, cost):
    origin = bars[SYMBOLS[0]][0].time
    fee, slip = cost
    factor = (1-slip)*(1-fee)
    rows = []
    for trade in account['ledger']:
        start, stop = [(trade[k]-origin)//BAR for k in ('entry','exit')]
        tape = bars[trade['symbol']]
        qty, debit, original = (trade[k] for k in ('quantity','debit','pnl'))
        peak = float('-inf')
        best = None
        outcomes = {}
        waiting = set()
        for i in range(start+1, stop):
            prior, execution = tape[i-1].close, tape[i].open
            if prior is not None:
                observed = qty*prior*factor/debit-1
                peak = max(peak, observed)
                if observed >= NET_TARGET:
                    waiting.add('target')
                if peak >= NET_LOCK[0] and observed <= NET_LOCK[1]:
                    waiting.add('lock')
            if execution is None:
                continue
            net = qty*execution*factor-debit
            if prior is not None and observed > 0:
                best = net if best is None else max(best,net)
            for name in waiting:
                outcomes.setdefault(name, dict(exit=tape[i].time, net=net))
            waiting.clear()
        rows.append(dict(symbol=trade['symbol'], entry=trade['entry'], original_exit=trade['exit'],
                         original_net=original, best_next_open_after_positive_close=best,
                         target=outcomes.get('target',dict(exit=trade['exit'],net=original)),
                         lock=outcomes.get('lock',dict(exit=trade['exit'],net=original))))
    return dict(baseline_losses=sum(r['original_net']<0 for r in rows),
                losses_with_positive_opportunity=sum(r['original_net']<0
                    and (r['best_next_open_after_positive_close'] or 0)>0 for r in rows),
                paired={name:dict(
                    net_usdt=math.fsum(r[name]['net'] for r in rows),
                    wins=sum(r[name]['net']>0 for r in rows),
                    rescued_losses=sum(r['original_net']<0<r[name]['net'] for r in rows),
                    forgone_usdt=math.fsum(max(0.,r['original_net']-r[name]['net']) for r in rows),
                    gained_usdt=math.fsum(max(0.,r[name]['net']-r['original_net']) for r in rows),
                ) for name in ('target','lock')}, rows=rows)


def review(bars, baseline):
    signals = {'rotation':rotation_decisions(bars)}
    results = {}
    for period, controls in baseline.items():
        start,end = period.split('/')
        results[period] = {}
        for label,cost in [('base',BASE_COST),('stress',STRESS_COST)]:
            accounts = {
                name: annotate(simulate(bars,signals,'rotation',start,end,cost,
                                       **ACCOUNT_LIMITS,**params))
                for name,params in MODES.items()
            }
            assert accounts['original']==controls['hold_momentum'][label]
            results[period][label] = dict(accounts=accounts,
                                          matched=match(bars,accounts['original'],cost))
    return results


if __name__ == '__main__':
    control = Path(__file__).with_name('weekly_rotation_forecast_results.json')
    baseline = json.loads(control.read_text(encoding='utf-8'))['results']
    bars = load('2017-12-01', allow_aligned_gaps=True)
    print(json.dumps(review(bars,baseline),indent=2))
