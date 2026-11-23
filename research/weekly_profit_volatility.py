"""Measure the effect of a volatility budget in the seven-day spot strategy."""
import argparse
import json
import math
import sys
from functools import partial
from pathlib import Path

from flow_forward import DAY
from slow_spot_replay import BAR, SYMBOLS, load
from weekly_profit_cycle import WINDOWS, summarize
from weekly_profit_policy import COSTS, WEEK, WeeklyTrend
from weekly_profit_volatility_policy import (
    DAILY_RISK_BUDGET, LOOKBACK_DAYS, MINIMUM_PLANNED_NOTIONAL, REFERENCE_DAILY_VOL,
    VolatilityBudgetPolicy, daily_volatility,
)
from weekly_rotation_delayed_account import bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import load_bitstamp


REFERENCE = Path(__file__).with_name('weekly_profit_cycle_results.json')


def volatility(bars, index):
    bars_per_day = DAY//BAR
    first = index-1-LOOKBACK_DAYS*bars_per_day
    if first<0:
        return None
    return daily_volatility([bar.close for bar in bars[first:index:bars_per_day]])


class VolatilityBudget(VolatilityBudgetPolicy, WeeklyTrend):
    pass


def baseline_groups(account,bars,origin):
    groups = {label:[] for label in ('full_budget','reduced_budget','below_minimum','missing_volatility')}
    for trade in account['ledger']:
        index = (trade['entry']-60-origin)//BAR
        sigma = volatility(bars[trade['symbol']],index)
        if sigma is None:
            label = 'missing_volatility'
        elif sigma<=REFERENCE_DAILY_VOL:
            label = 'full_budget'
        elif sigma<=DAILY_RISK_BUDGET/MINIMUM_PLANNED_NOTIONAL:
            label = 'reduced_budget'
        else:
            label = 'below_minimum'
        groups[label].append(trade['pnl'])
    return {label:dict(trades=len(values),net=math.fsum(values),worst=min(values,default=None))
            for label,values in groups.items()}


def evaluate(venue):
    is_bitstamp = venue=='bitstamp'
    bars = load_bitstamp()[0] if is_bitstamp else load('2017-12-01',allow_aligned_gaps=True)
    origin = bars[SYMBOLS[0]][0].time
    price = cached_reader(venue,bitstamp_fill if is_bitstamp else minute_open)
    before = first_minute(venue)
    reference = json.loads(REFERENCE.read_bytes())['venues'][venue]['results']

    def measure(symbol,boundary):
        return volatility(bars[symbol],(boundary-origin)//BAR)

    output = {}
    for start,end in WINDOWS:
        span = start+'/'+end
        output[span] = {}
        for lane,cost in COSTS.items():
            original = reference[span][lane]['weekly_trend']
            outcomes = dict(reference_groups=baseline_groups(original,bars,origin))
            for name,scale in [('skip_only',False),('scale_and_skip',True)]:
                account = run(bars,start,end,cost,price,before,
                              account_type=partial(VolatilityBudget,measure=measure,scale=scale),
                              report_drawdown_amount=True,settle_at_next_fill=True)
                result = summarize(account,start,end,False)
                result['late_closes'] = sum(t['exit']-t['entry']>WEEK for t in account['ledger'])
                result['passes_numeric_gate'] = (result['late_closes']==0 and result['profit_exceeds_drawdown']
                                                 and result['meets_drawdown_cap'])
                outcomes[name] = result
                print(f'{venue} {span} {lane} {name} done',file=sys.stderr,flush=True)
            output[span][lane] = outcomes
    return dict(venue=venue,lookback_days=LOOKBACK_DAYS,reference_daily_vol=REFERENCE_DAILY_VOL,
                daily_risk_budget=DAILY_RISK_BUDGET,max_notional=NOMINAL,min_notional=ORDER_MINIMUM,
                min_planned_notional=MINIMUM_PLANNED_NOTIONAL,results=output)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
