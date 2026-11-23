"""Summarize paired absorption scores with calendar-block uncertainty."""
import argparse
import json
from collections import defaultdict
from datetime import datetime, timezone
from math import isclose, log, sqrt
from pathlib import Path
from random import Random
from statistics import mean

from absorption_event_review import PRIMARY, quantile


DAY = 86400
BLOCK_DAYS = 3
MIN_PAIR_DAYS = 12
DRAW_COUNT = 4000


def valid_rows(rows, horizon, lane):
    for row in rows:
        left, right = row['treated_bps'][horizon], row['control_bps'][horizon]
        if left is not None and right is not None and left[lane] is not None and right[lane] is not None:
            yield row,left[lane],right[lane]


def grouped_pairs(rows, horizon, lane):
    groups = defaultdict(list)
    for row,left,right in valid_rows(rows,horizon,lane):
        day = row['candidate']['time']//DAY
        if day != row['control']['time']//DAY:
            raise ValueError('Matched events must share a UTC date')
        groups[day].append((left,right))
    return groups


def block_interval(groups, days, *, kind='delta'):
    if len(groups) < MIN_PAIR_DAYS or len(days) < 2*BLOCK_DAYS:
        return None
    daily = [groups.get(day,[]) for day in days]
    rolling = [(len(group),sum(left-right if kind=='delta' else left for left,right in group)) for group in daily]
    rng = Random(417)
    outcomes = []
    for _ in range(DRAW_COUNT):
        count, profit, remaining = 0,0.,len(rolling)
        while remaining:
            offset = rng.randrange(len(rolling))
            block = min(BLOCK_DAYS,remaining)
            for step in range(block):
                partial_count, partial_profit = rolling[(offset+step)%len(rolling)]
                count += partial_count
                profit += partial_profit
            remaining -= block
        if count:
            outcomes.append(profit/count)
    return [quantile(outcomes,.025),quantile(outcomes,.975)] if outcomes else None


def before_fee_and_slippage(net_bps, side, cost):
    if net_bps is None:
        return None
    fee, slip = cost
    net = net_bps/10000
    if side == 'bid':
        ratio = (net+1+fee)/(1-fee)*(1+slip)/(1-slip)
        return (ratio-1)*10000
    ratio = (1-fee-net)/(1+fee)*(1-slip)/(1+slip)
    return (1-ratio)*10000


def feature_balance(rows, horizon, lane):
    valid = [(row['candidate'],row['control']) for row,_,_ in valid_rows(rows,horizon,lane)]
    if not valid:
        return None
    features = dict(log_pressure=lambda event:log(event['pressure']),impact_bps=lambda event:event['impact_bps'],
                    spread_bps=lambda event:event['spread_bps'],range_bps=lambda event:event['range_bps'],
                    log_depth=lambda event:log(event['depth']),normalized_ofi=lambda event:event['ofi'])
    if 'pressure_ratio' in valid[0][0]:
        features.update(remaining_pressure=lambda event:event['pressure_ratio'],
                        recovery_bps=lambda event:event['recovery_bps'])
    balance = {}
    for feature, measurement in features.items():
        targets,controls = zip(*((measurement(candidate),measurement(control)) for candidate,control in valid))
        target_mean, control_mean = mean(targets), mean(controls)
        pooled = sqrt(mean((left-target_mean)**2+(right-control_mean)**2 for left,right in zip(targets,controls))/2)
        difference = target_mean-control_mean
        balance[feature] = dict(treated_mean=target_mean,control_mean=control_mean,difference=difference,
                                standardized_difference=difference/pooled if len(valid)>1 and pooled>0 else None)
    return dict(closest_gap_seconds=min(abs(c['time']-k['time']) for c,k in valid),
                farthest_gap_seconds=max(abs(c['time']-k['time']) for c,k in valid),features=balance)


def report_side(report, days, costs, side, *, spot):
    horizon = str(PRIMARY)
    levels = {}
    for lane, cost in costs.items():
        groups = grouped_pairs(report['ledger'],horizon,lane)
        if any(day not in days for day in groups):
            raise ValueError('Matched row outside evaluation')
        pairs = [pair for group in groups.values() for pair in group]
        expected = report['scores'][horizon][lane]
        if len(pairs) != expected['pairs']:
            raise ValueError('Ledger pair count disagrees with reported score')
        delta = mean(left-right for left,right in pairs) if pairs else None
        strong = mean(left for left,_ in pairs) if pairs else None
        if pairs and (not isclose(strong,expected['treated_mean_bps'],abs_tol=1e-9)
                      or not isclose(delta,expected['paired_difference_bps'],abs_tol=1e-9)):
            raise ValueError('Ledger pair means disagree with reported score')
        level = dict(pairs=len(pairs),pair_days=len(groups),delta_mean_bps=delta,strong_net_mean_bps=strong,
                     delta_calendar_block_95_bps=block_interval(groups,days),
                     strong_calendar_block_95_bps=block_interval(groups,days,kind='strong'),
                     positive_delta_days=sum(mean(left-right for left,right in bucket)>0 for bucket in groups.values()),
                     balance=feature_balance(report['ledger'],horizon,lane))
        for name,key in (('strong','with_strong_refill'),('pressure','without_refill_filter')):
            standalone = report.get(f'exhaustion_{key}',report.get(f'pressure_{key}'))
            if standalone:
                value = standalone['mean_bps'][lane]
                level[f'{name}_standalone_count'] = standalone['counts'][lane]
                level[f'{name}_standalone_net_bps'] = value
                if spot:
                    level[f'{name}_standalone_before_fee_and_slippage_bps'] = before_fee_and_slippage(value,side,cost)
        levels[lane] = level
    return levels


def summarize(source):
    start = source['evaluation_start_utc_second']
    end = source['end_utc_second']
    if start % DAY or end % DAY or start >= end:
        raise ValueError('This audit requires complete UTC days')
    days = list(range(start//DAY,end//DAY))
    spot = 'spot_ask_absorption' in source
    reports = {}
    for symbol, panel in source['symbols'].items():
        if panel.get('ready') is False or (source.get('diagnostic_only') and 'variants' not in panel):
            reports[symbol] = {'ready':False}
            continue
        if 'variants' in panel:
            reports[symbol] = {
                variant:{control:{side:report_side(report,days,source['costs'],side,spot=spot) for side,report in sides.items()}
                         for control,sides in choices.items()}
                for variant,choices in panel['variants'].items()
            }
        else:
            reports[symbol] = {side:report_side(report,days,source['costs'],side,spot=spot)
                               for side,report in panel['sides'].items()}
    return dict(start_utc=datetime.fromtimestamp(start,timezone.utc).date().isoformat(),
                end_utc_exclusive=datetime.fromtimestamp(end,timezone.utc).date().isoformat(),
                horizon_seconds=PRIMARY,block_days=BLOCK_DAYS,min_pair_days=MIN_PAIR_DAYS,
                **({'diagnostic_only':True,'promotion_allowed':False} if source.get('diagnostic_only') else {}),
                balance_note='OFI is descriptive, not a matching input; part of it can reflect refilling itself.',
                before_fee_and_slippage='Still includes the visible bid/ask spread; no funding.' if spot else None,
                symbols=reports)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('results',type=Path)
    cli.add_argument('output',type=Path)
    args = cli.parse_args()
    report = summarize(json.loads(args.results.read_text(encoding='utf-8')))
    args.output.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
