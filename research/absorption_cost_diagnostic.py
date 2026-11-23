"""Separate spot quote markout from fees and compare holding periods on identical pairs."""
import argparse
import json
from collections import Counter
from math import isclose
from pathlib import Path
from statistics import mean

from absorption_pair_audit import before_fee_and_slippage


def zero_slip_break_even_fee(raw_bps, side):
    if raw_bps is None or raw_bps <= 0:
        return None
    move = raw_bps/10000
    return 10000*move/(2+move if side=='bid' else 2-move)


def same_pairs(ledger, horizons, lane):
    for row in ledger:
        marks = [(row['treated_bps'][horizon],row['control_bps'][horizon]) for horizon in horizons]
        if all(left and right and left[lane] is not None and right[lane] is not None for left,right in marks):
            yield row['candidate']['time'],[(left[lane],right[lane]) for left,right in marks]


def holding_profile(ledger, horizons, lane, side, costs):
    pairs = list(same_pairs(ledger,horizons,lane))
    summary = {}
    if pairs:
        for index, horizon in enumerate(horizons):
            strong, control = zip(*(values[index] for _,values in pairs))
            raw = before_fee_and_slippage(mean(strong),side,costs)
            summary[horizon] = dict(net_bps=mean(strong),control_net_bps=mean(control),
                                    incremental_net_bps=mean(left-right for left,right in zip(strong,control)),
                                    no_fee_no_extra_slippage_bps=raw,
                                    zero_extra_slippage_break_even_fee_per_side_bps=zero_slip_break_even_fee(raw,side))
    return dict(pairs=len(pairs),utc_days=len({time//86400 for time,_ in pairs}),horizons=summary)


def primary_groups(sides, side, variant, costs):
    key = 'pressure' if variant=='absorption' else 'exhaustion'
    groups = {}
    for name, suffix in (('without_refill_filter','without_refill_filter'),('strong_refill','with_strong_refill')):
        data = sides[f'{key}_{suffix}']
        base = data['mean_bps']['base']
        raw = before_fee_and_slippage(base,side,costs['base'])
        alternative = before_fee_and_slippage(data['mean_bps']['stress'],side,costs['stress'])
        if raw is not None and data['counts']['base']==data['counts']['stress'] and (
                alternative is None or not isclose(raw,alternative,abs_tol=1e-7)):
            raise ValueError('Cost assumptions do not reconcile on the same fill cohort')
        groups[name] = dict(scored=data['counts']['base'],net_bps=data['mean_bps'],
                            no_fee_no_extra_slippage_bps=raw,
                            zero_extra_slippage_break_even_fee_per_side_bps=zero_slip_break_even_fee(raw,side))
    return groups


def summarize(source):
    if 'spot_ask_absorption' not in source:
        raise ValueError('Historical spot costs only; futures funding cannot be inverted by this diagnostic')
    horizons = list(map(str,source['all_horizons_seconds']))
    costs = source['costs']
    symbols = {}
    totals = {}
    for symbol,panel in source['symbols'].items():
        symbols[symbol] = {}
        for variant,controls in panel['variants'].items():
            symbols[symbol][variant] = {}
            tally = totals.setdefault(variant,Counter())
            for side,sides in controls['calibration_bottom_quartile'].items():
                primary = primary_groups(sides,side,variant,costs)
                strong = primary['strong_refill']
                tally['scored'] += strong['scored']
                tally['groups'] += 1
                tally['positive_without_fees_and_slippage'] += bool((strong['no_fee_no_extra_slippage_bps'] or 0)>0)
                tally['positive_after_base_cost'] += bool((strong['net_bps']['base'] or 0)>0)
                tally['positive_after_stress_cost'] += bool((strong['net_bps']['stress'] or 0)>0)
                profiles = {label:{lane:holding_profile(comparisons[side]['ledger'],horizons,lane,side,costs[lane])
                                   for lane in costs} for label,comparisons in controls.items()}
                symbols[symbol][variant][side] = dict(primary=primary,common_horizon_pairs=profiles)
    return dict(evaluation_start_utc_second=source['evaluation_start_utc_second'],
                end_utc_second=source['end_utc_second'],primary_seconds=source['primary_seconds'],
                quote_markout='Same bid/ask crossing; removes simulated fee and additional slippage, not spread.',
                population='Event means, not account PnL; spot ask side is a directional proxy.',
                totals=totals,symbols=symbols)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('results',type=Path)
    cli.add_argument('output',type=Path)
    args = cli.parse_args()
    report = summarize(json.loads(args.results.read_text(encoding='utf-8')))
    args.output.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
