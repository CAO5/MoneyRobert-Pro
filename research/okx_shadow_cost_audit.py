"""Explain the frozen vendor holdout costs without reselecting signals or trades."""
import argparse
import json
from math import isclose
from pathlib import Path
from statistics import mean

from absorption_event_review import nonoverlapping_primary_events
from okx_absorption_review import AccountedFills, DAY, DELAY, PRIMARY, Quality, SWAP_COSTS
from okx_tardis_audit import replay_native


def decompose(panel, event, outcomes):
    entry = panel.quotes[event.time+DELAY]
    exit = panel.quotes[event.time+DELAY+PRIMARY]
    buy_first = event.side == 'bid'
    buy,sell = (entry.prices,exit.prices) if buy_first else (exit.prices,entry.prices)
    move = (sell[0]+sell[1]-buy[0]-buy[1])/2
    spread = (buy[1]-buy[0]+sell[1]-sell[0])/2
    carry = panel.funding_per_unit(entry,exit)*(-1 if buy_first else 1)
    unadjusted_reference = buy[1] if buy_first else sell[0]
    before_costs = (sell[0]-buy[1]+carry)/unadjusted_reference*10000
    lanes = {}
    for lane,(fee,slippage) in SWAP_COSTS.items():
        paid,received = buy[1]*(1+slippage),sell[0]*(1-slippage)
        reference = paid if buy_first else received
        factor = 10000/reference
        components = dict(directional_mid_bps=move*factor,visible_spread_cost_bps=spread*factor,
                          extra_slippage_cost_bps=((paid-buy[1])+(sell[0]-received))*factor,
                          fee_cost_bps=(paid+received)*fee*factor,signed_funding_bps=carry*factor)
        reconstructed = (components['directional_mid_bps']-components['visible_spread_cost_bps']
                         -components['extra_slippage_cost_bps']-components['fee_cost_bps']
                         +components['signed_funding_bps'])
        if not isclose(reconstructed,outcomes[lane],rel_tol=1e-10,abs_tol=1e-8):
            raise ValueError(f'Cost components do not reconcile for {event.time} {lane}')
        lanes[lane] = dict(**components,net_bps=outcomes[lane])
    return dict(decision_utc_second=event.time,zero_fee_zero_extra_slippage_bps=before_costs,
                round_trip_turnover_per_open_notional=(buy[1]+sell[0])/unadjusted_reference,cost_lanes=lanes)


def summarize(samples):
    before_costs = mean(row['zero_fee_zero_extra_slippage_bps'] for row in samples) if samples else None
    zero_extra_slippage_fee_limit = (
        before_costs/mean(row['round_trip_turnover_per_open_notional'] for row in samples)
        if before_costs is not None and before_costs > 0 else None)
    means = {lane:{key:mean(row['cost_lanes'][lane][key] for row in samples)
                   for key in samples[0]['cost_lanes'][lane]} if samples else None for lane in SWAP_COSTS}
    return dict(scored=len(samples),mean_zero_fee_zero_extra_slippage_bps=before_costs,
                zero_extra_slippage_break_even_one_way_fee_bps=zero_extra_slippage_fee_limit,
                means_by_cost_lane=means,samples=samples)


def side_components(panel, events, score, evaluate, variant, evaluation_start, side):
    prefix = 'pressure' if variant=='absorption' else 'exhaustion'
    prior = score[f'{prefix}_with_strong_refill']
    candidates = []
    if 'refill_cut' in score:
        for event in events:
            origin = event.time if variant=='absorption' else event.shock_time
            if (origin >= evaluation_start and event.side==side and event.pressure >= score['pressure_cut_usdt']
                    and event.impact_bps <= score['impact_cut_bps'] and event.refill > 0
                    and event.refill >= score['refill_cut']):
                candidates.append(event)
    if len(candidates) != score['candidates']:
        raise ValueError('Frozen strong-refill candidates do not reconcile')
    samples, missing = [],0
    for event in nonoverlapping_primary_events(candidates):
        result = evaluate(event)[str(PRIMARY)]
        if not result or any(value is None for value in result.values()):
            missing += 1
            continue
        samples.append(decompose(panel,event,result))
    if missing != prior['missing'] or any(len(samples) != prior['counts'][lane] for lane in SWAP_COSTS):
        raise ValueError('Frozen execution sample changed')
    report = summarize(samples)
    for lane in SWAP_COSTS:
        if samples and not isclose(report['means_by_cost_lane'][lane]['net_bps'],prior['mean_bps'][lane],
                                   rel_tol=1e-10,abs_tol=1e-8):
            raise ValueError('Frozen sample mean changed')
    return dict(**report,candidate_count=len(candidates),rejected=missing)


def audit(root, frozen):
    start,end = frozen['evaluation_start_utc_second'],frozen['end_utc_second']
    if (not frozen.get('diagnostic_only') or frozen.get('promotion_allowed') is not False or end-start != DAY
            or frozen['delay_seconds'] != DELAY or frozen['primary_seconds'] != PRIMARY
            or {name:tuple(cost) for name,cost in frozen['costs'].items()} != SWAP_COSTS):
        raise ValueError('Expected the unchanged one-day vendor shadow report')
    panels, _, metadata = replay_native(root,start,24,keep_execution=True,collect_events=True)
    if metadata != frozen['evaluation_source']:
        raise ValueError('The native holdout archive differs from the frozen report')
    reports = {}
    for name, panel in panels.items():
        prior = frozen['symbols'][name]
        if not prior['ready']:
            reports[name] = dict(ready=False,reasons=prior['reasons'])
            continue
        panel.funding_times, panel.mark_times = sorted(panel.funding),sorted(panel.mark)
        evaluate = AccountedFills(panel,Quality(start,end,panel.gaps),end)
        reports[name] = {
            variant:{side:side_components(panel,events,score,evaluate,variant,start,side) for side,score in
                     prior['variants'][variant]['calibration_bottom_quartile'].items()}
            for variant,events in (('absorption',panel.events),('exhaustion',panel.confirmations))}
    return dict(diagnostic_only=True,promotion_allowed=False,primary_seconds=PRIMARY,delay_seconds=DELAY,
                note='Same scored strong-refill orders. A break-even estimate is not a feasible fee quote.',
                evaluation_source=metadata,symbols=reports)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('root',type=Path)
    cli.add_argument('frozen_results',type=Path)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    report = audit(args.root,json.loads(args.frozen_results.read_text(encoding='utf-8')))
    args.output.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
