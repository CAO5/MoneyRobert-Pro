"""Screen first-week trend changes against a same-time opposite-direction control."""
import argparse
import json
import math
from pathlib import Path

from weekly_flow_confirmation_screen import score
from weekly_relative_pair_screen import BAR, COSTS, END, INITIAL, LOOKBACK, PAIR_GROSS, SPLIT, WEEK, epoch, load_cached, trade_positions


START = '2024-02-12'
MODES = ('follow_new_direction','fade_new_direction')


def select_switch(bars, opening):
    cutoff = opening-BAR
    prior = cutoff-BAR
    candidates = []
    for name,series in bars.items():
        current = math.log(series[prior].close/series[prior-LOOKBACK].close)
        previous = math.log(series[prior-WEEK].close/series[prior-WEEK-LOOKBACK].close)
        if current*previous >= 0:
            continue
        turnover = math.fsum(series[stamp].turnover for stamp in range(cutoff-WEEK,cutoff,BAR))
        candidates.append((turnover,name,current,previous))
    if not candidates:
        return None
    turnover,name,current,previous = max(candidates)
    return dict(asset=name,side=1 if current>0 else -1,decision_utc_second=cutoff,
                turnover_rank_value=turnover,current_log_return=current,previous_log_return=previous,
                switched_assets=len(candidates))


def replay(bars, rates, clocks, first, last, fee, slip):
    paths = {mode:[] for mode in MODES}
    for opening in range(first+BAR,last+BAR,WEEK):
        selected = select_switch(bars,opening)
        if selected is None:
            for path in paths.values():
                path.append(None)
            continue
        closing = opening+WEEK
        for mode,direction in zip(MODES,(1,-1)):
            side = direction*selected['side']
            positions = ((selected['asset'],side,PAIR_GROSS),)
            marks,final = trade_positions(bars,rates,clocks,opening,closing,positions,fee,slip,intrabar=True)
            case = dict(selected,side=side,entered_utc_second=opening,exited_utc_second=closing,**final)
            paths[mode].append((case,marks))
    return paths


def report():
    inputs = load_cached()
    bars = {name:{row.t:row for row in source[0]} for name,source in inputs.items()}
    rates = {name:source[1] for name,source in inputs.items()}
    clocks = {name:[row[0] for row in series] for name,series in rates.items()}
    periods = {}
    for first,last in ((START,SPLIT),(SPLIT,END),(START,END)):
        group = {}
        for lane,cost in COSTS.items():
            paths = replay(bars,rates,clocks,epoch(first),epoch(last),*cost)
            group[lane] = {mode:score(rows,'price_only',with_interval=lane=='stress') for mode,rows in paths.items()}
        periods[first+'/'+last] = group
    return dict(historical_screen_only=True,promotion_allowed=False,initial_usdt=INITIAL,position_notional_usdt=PAIR_GROSS,
                rule='28-day return sign must reverse since last week. Highest prior-seven-day quote turnover selects the single leg.',
                hold_seconds=WEEK,entry_delay_seconds=BAR,costs=COSTS,periods=periods,
                limits='Cash if no switch. Reused historical bars and settled funding; no order book or margin tests.',
                sources={name:source[2] for name,source in inputs.items()})


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    args.output.write_text(json.dumps(report(),indent=2,allow_nan=False)+'\n',encoding='utf-8')
