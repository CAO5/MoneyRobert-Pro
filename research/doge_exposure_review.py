"""Compare moderate nominal exposure changes on the unchanged DOGE breakout."""
import argparse
import json
import math
from datetime import date, datetime, timezone
from functools import partial
from pathlib import Path

from doge_latency_review import BASELINE, SYMBOL, recorded_execution
from long_short_expectancy import COSTS, INITIAL, MAX_NOTIONAL, RISK_PER_ORDER, epoch, evaluate, feature_series, inputs, minute_price


MULTIPLIERS = (1., 1.25, 1.5, 2.)
LATENCY_BASELINE = Path(__file__).with_name('doge_latency_review_results.json')
PERIODS = {'continuous':(60,), 'before_2026':(60,), '2026':(1,5), 'recent_30d':(1,5)}


def study(stop):
    reference = json.loads(BASELINE.read_bytes())
    latency = json.loads(LATENCY_BASELINE.read_bytes())
    if reference['end_utc_exclusive'] != stop or latency['end_utc_exclusive'] != stop:
        raise ValueError('Use the same end date as the saved breakout reviews')
    markets, limits, curve, source = inputs(stop)
    candles, rates, provenance = markets[SYMBOL]
    if provenance != reference['feed'][SYMBOL] or limits[SYMBOL] != reference['limit_snapshot'][SYMBOL]:
        raise ValueError('Source observations or modeled contract filters changed')
    signals = {SYMBOL:feature_series(candles,rates,'breakout')[0]}
    manifests = {}
    second_price = partial(recorded_execution,manifests)
    result = {}
    for period,delays in PERIODS.items():
        start,end = (epoch(stamp[:10]) for stamp in reference['range_utc'][period])
        result[period] = {}
        for delay in delays:
            execution = minute_price if delay == 60 else second_price
            result[period][str(delay)] = {}
            for lane,cost in COSTS.items():
                runs = {}
                for multiple in MULTIPLIERS:
                    # Margin allocation is descriptive here. The simulator applies notional and loss budgets.
                    notional = MAX_NOTIONAL*multiple
                    for mode in ('fixed_risk','scaled_risk') if multiple != 1 else ('fixed_risk',):
                        risk = RISK_PER_ORDER*(multiple if mode == 'scaled_risk' else 1)
                        outcome = evaluate({SYMBOL:candles},{SYMBOL:rates},signals,limits,curve,start,end,cost,
                                           (1,-1),(SYMBOL,),execution,delay_seconds=delay,
                                           notional_cap=notional,risk_per_order=risk)
                        if multiple == 1:
                            expected = (reference['doge_one_minute_delay']['breakout'][period][lane]['doge_both']
                                        if delay == 60 else latency['cases'][period][lane][str(delay)])
                            if any(value != expected[key] for key,value in json.loads(json.dumps(outcome)).items()):
                                raise ValueError('The unscaled control differs from the saved breakout result')
                        traded = [t['notional'] for t in outcome['ledger']]
                        if max(traded,default=0) > notional+1e-9:
                            raise ValueError('Position exceeds nominal cap')
                        runs[f'{multiple:g}x_{mode}'] = dict(**outcome, nominal_cap=notional,
                                                            planned_risk=risk, peak_nominal=max(traded,default=0),
                                                            average_nominal=math.fsum(traded)/len(traded) if traded else None)
                result[period][str(delay)][lane] = runs
    return dict(end_utc_exclusive=stop, initial_equity=INITIAL, unchanged_signal='DOGE bidirectional breakout',
                note='Multipliers apply to a 25 USDT notional cap on 100 USDT initial equity. No simultaneous positions. '
                     'Funding and trade costs are included. Margin liquidation is outside the model.',
                assumptions=dict(max_drawdown_pause_pct=8, nominal_isolated_budget_reference=MAX_NOTIONAL,
                                 fixed_risk=RISK_PER_ORDER, contract_filters=limits[SYMBOL]),
                curve=source, new_price_observation_days=sorted(manifests), scenarios=result)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--end',type=date.fromisoformat,default=datetime.now(timezone.utc).date())
    cli.add_argument('--output',type=Path)
    args = cli.parse_args()
    payload = json.dumps(study(args.end.isoformat()),indent=2,allow_nan=False)+'\n'
    if args.output:
        args.output.write_text(payload,encoding='utf-8')
    else:
        print(payload,end='')
