"""Test two fixed filters for reentering a recently stopped asset."""
import json
import math
from pathlib import Path

from slow_spot_replay import load, rotation_decisions, simulate
from weekly_rotation_venue_review import WINDOWS, load_bitstamp
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


ARGS = dict(entry_target=12.25, minimum_order_notional=10.,
            profit_protection=True, net_profit_lock=(.03,.015))


def followed_stop(trades):
    matched = [nxt for prior,nxt in zip(trades,trades[1:])
               if nxt['symbol']==prior['symbol'] and nxt['entry']-prior['exit'] <= 7*86400
               and prior['pnl']<0 and prior['reason'] in ('position_stop','net_profit_lock')]
    return dict(trades=len(matched),wins=sum(t['pnl']>0 for t in matched),
                net=math.fsum(t['pnl'] for t in matched))


def review(bars, stored, windows):
    weekly = {'rotation':rotation_decisions(bars)}
    result = {}
    for start,end in windows:
        period = start+'/'+end
        result[period] = {}
        for label,cost in [('base',BASE_COST), ('stress',STRESS_COST)]:
            accounts = {
                name:annotate(simulate(bars,weekly,'rotation',start,end,cost,**ARGS,reentry_policy=policy))
                for name,policy in [('original',None), ('one_week','one_week'), ('recover','recover')]
            }
            assert accounts['original']==stored[period]['early_profit_lock'][label]
            result[period][label] = dict(accounts=accounts,
                immediate_repeats_after_loss=followed_stop(accounts['original']['ledger']))
    return result


if __name__=='__main__':
    reference = Path(__file__).with_name('weekly_rotation_venue_results.json')
    previous = json.loads(reference.read_text(encoding='utf-8'))
    bitstamp,_ = load_bitstamp()
    binance = load('2017-12-01',allow_aligned_gaps=True)
    print(json.dumps(dict(binance=review(binance,previous['binance'],WINDOWS[1:]),
                         bitstamp=review(bitstamp,previous['bitstamp'],WINDOWS)),indent=2))
