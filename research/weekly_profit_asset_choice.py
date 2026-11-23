"""Compare single-asset signals and risk-eligible rotation under the same seven-day account."""
import argparse
import json
import sys
from functools import partial
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, load
from weekly_profit_cycle import WINDOWS, expectancy_interval, summarize
from weekly_profit_policy import COSTS, WEEK
from weekly_profit_volatility import VolatilityBudget, volatility
from weekly_profit_volatility_policy import DAILY_RISK_BUDGET, MINIMUM_PLANNED_NOTIONAL
from weekly_rotation_delayed_account import bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import load_bitstamp


REFERENCE = Path(__file__).with_name('weekly_profit_volatility_results.json')
MAXIMUM_SIGMA = DAILY_RISK_BUDGET/MINIMUM_PLANNED_NOTIONAL


def single_asset(symbol,returns,boundary):
    return symbol if returns[symbol]>0 else None


def eligible_leader(returns,boundary,measure,changes):
    ranked = sorted(returns,key=returns.get,reverse=True)
    positive = [s for s in ranked if returns[s]>0]
    if not positive:
        return None
    sigmas = {s:measure(s,boundary) for s in positive}
    target = next((s for s in positive if sigmas[s] is not None and sigmas[s]<=MAXIMUM_SIGMA),None)
    if target and target!=positive[0]:
        changes.append(dict(boundary=boundary,leader=positive[0],target=target,
                            returns_28d=returns,sigma=sigmas))
    return target


def evaluate(venue):
    bitstamp = venue=='bitstamp'
    bars = load_bitstamp()[0] if bitstamp else load('2017-12-01',allow_aligned_gaps=True)
    origin = bars[SYMBOLS[0]][0].time
    price = cached_reader(venue,bitstamp_fill if bitstamp else minute_open)
    pre_fill = first_minute(venue)
    reference = json.loads(REFERENCE.read_bytes())['venues'][venue]['results']

    def measure(symbol,boundary):
        return volatility(bars[symbol],(boundary-origin)//BAR)

    account_type = partial(VolatilityBudget,measure=measure,scale=True)
    results = {}
    for start,end in WINDOWS:
        span = start+'/'+end
        results[span] = {}
        for lane,cost in COSTS.items():
            selections = []
            rules = {'reference':None,
                     'btc_only':partial(single_asset,'BTCUSDT'),
                     'eth_only':partial(single_asset,'ETHUSDT'),
                     'eligible_rotation':partial(eligible_leader,measure=measure,changes=selections)}
            accounts = {}
            original = reference[span][lane]['scale_and_skip']
            for name,choose in rules.items():
                account = run(bars,start,end,cost,price,pre_fill,account_type=account_type,
                              report_drawdown_amount=True,settle_at_next_fill=True,select_target=choose)
                row = summarize(account,start,end,False)
                row['late_closes'] = sum(t['exit']-t['entry']>WEEK for t in row['ledger'])
                row['passes_numeric_gate'] = (row['late_closes']==0 and row['profit_exceeds_drawdown']
                                              and row['meets_drawdown_cap'])
                if name=='reference':
                    if any(row[k]!=v for k,v in original.items() if k!='paired_weekly_difference_95'):
                        raise ValueError(f'Original rotation changed for {venue} {span} {lane}')
                else:
                    differences = [new-old for new,old in zip(row['weekly_pnl_by_entry'],original['weekly_pnl_by_entry'])]
                    row['paired_weekly_difference_95'] = expectancy_interval(differences)
                accounts[name] = row
                print(venue,span,lane,name,'completed',file=sys.stderr,flush=True)
            accounts['eligible_rotation']['eligible_runner_up_signals'] = selections
            results[span][lane] = accounts
    return dict(venue=venue,quote_unit='USD' if bitstamp else 'USDT',risk_cap_daily_volatility=MAXIMUM_SIGMA,
                reference=REFERENCE.name,results=results)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
