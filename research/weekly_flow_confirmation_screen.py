"""Compare weekly direction by price alone with anonymous taker-flow confirmation."""
import argparse
import json
import math
from collections import Counter
from pathlib import Path
from statistics import mean, pstdev

from weekly_account_score import block_intervals
from weekly_relative_pair_screen import (ASSETS, BAR, COSTS, END, INITIAL, LOOKBACK, PAIR_GROSS, SPLIT, START, WEEK,
                                         epoch, load_cached, trade_positions)


DAY = 86400
MODES = ('price_only','flow_aligned','flow_opposed')


def frame(bars, opening):
    cutoff = opening-BAR
    prior = cutoff-BAR
    ranked = []
    for name,series in bars.items():
        closes = [series[stamp].close for stamp in range(prior-LOOKBACK,prior+1,DAY)]
        drift = math.log(closes[-1]/closes[0])
        noise = pstdev(math.log(close/previous) for previous,close in zip(closes,closes[1:]))
        if not noise or drift==0:
            continue
        ranked.append((abs(drift)/noise,name,1 if drift>0 else -1))
    if not ranked:
        return None
    strength,asset,side = max(ranked)
    observed = [bars[asset][stamp] for stamp in range(cutoff-WEEK,cutoff,BAR)]
    turnover = math.fsum(bar.turnover for bar in observed)
    if not turnover:
        return None
    imbalance = (2*math.fsum(bar.taker_buy for bar in observed)-turnover)/turnover
    return dict(asset=asset,side=side,trend_strength=strength,taker_flow_imbalance=imbalance,decision_utc_second=cutoff)


def replay_paths(bars, rates, clocks, start, end, fee, slip):
    for opening in range(start+BAR,end+BAR,WEEK):
        signal = frame(bars,opening)
        if signal is None:
            yield None
            continue
        closing = opening+WEEK
        positions = ((signal['asset'],signal['side'],PAIR_GROSS),)
        marks,final = trade_positions(bars,rates,clocks,opening,closing,positions,fee,slip,intrabar=True)
        yield dict(**signal,entered_utc_second=opening,exited_utc_second=closing,**final),marks


def score(paths, mode, *, with_interval=False):
    equity = peak = INITIAL
    maximum_drawdown = maximum_drawdown_fraction = 0.
    ledger,returns,counts,side_net = [],[],Counter(),Counter()
    for path in paths:
        if path is None:
            returns.append(0.)
            continue
        row,marks = path
        if mode!='price_only':
            signed_flow = row['side']*row['taker_flow_imbalance']
            if (mode=='flow_aligned' and signed_flow<=0) or (mode=='flow_opposed' and signed_flow>=0):
                returns.append(0.)
                continue
        for change in marks:
            value = equity+change
            peak = max(peak,value)
            maximum_drawdown = max(maximum_drawdown,peak-value)
            maximum_drawdown_fraction = max(maximum_drawdown_fraction,(peak-value)/peak)
        equity += row['net']
        ledger.append(row)
        returns.append(row['net'])
        direction = 'long' if row['side']>0 else 'short'
        counts[direction] += 1
        side_net[direction] += row['net']
    net = equity-INITIAL
    output = dict(weeks=len(returns),active_weeks=len(ledger),net=net,mean_weekly_net=mean(returns),
                  max_drawdown=maximum_drawdown,max_drawdown_fraction=maximum_drawdown_fraction,
                  side_counts=dict(counts),side_net=dict(side_net),
                  profit_and_risk_pass=net>maximum_drawdown and maximum_drawdown_fraction<=.08,
                  both_directions_net_positive=side_net['long']>0 and side_net['short']>0,ledger=ledger)
    if with_interval:
        output['historical_weekly_mean_block_intervals']=block_intervals(returns)
    return output


def report():
    inputs = load_cached()
    if set(inputs)!=set(ASSETS):
        raise ValueError('Missing reference instrument')
    bars = {name:{bar.t:bar for bar in values[0]} for name,values in inputs.items()}
    rates = {name:values[1] for name,values in inputs.items()}
    clocks = {name:[row[0] for row in series] for name,series in rates.items()}
    periods = {}
    for first,last in ((START,SPLIT),(SPLIT,END),(START,END)):
        periods[first+'/'+last] = {}
        for lane,cost in COSTS.items():
            paths = list(replay_paths(bars,rates,clocks,epoch(first),epoch(last),*cost))
            periods[first+'/'+last][lane] = {mode:score(paths,mode,with_interval=lane=='stress') for mode in MODES}
    return dict(historical_screen_only=True,promotion_allowed=False,initial_usdt=INITIAL,position_notional_usdt=PAIR_GROSS,
                price_rule='Rank abs 28-day return divided by daily volatility, take its sign and keep the asset fixed',
                flow_rule='Sign of last seven completed days of quote turnover (2*taker buy minus total)',
                hold_seconds=WEEK,entry_delay_seconds=BAR,costs=COSTS,periods=periods,
                limits='Bar and funding replay, adverse high-before-low mark bounds; no order book, lot or margin validation.',
                sources={name:values[2] for name,values in inputs.items()})


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    args.output.write_text(json.dumps(report(),indent=2,allow_nan=False)+'\n',encoding='utf-8')
