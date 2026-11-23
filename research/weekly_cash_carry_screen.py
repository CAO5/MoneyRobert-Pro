"""Test seven-day matched-quantity spot/perpetual carry entirely from existing cache."""
import argparse
import json
import math
from bisect import bisect_left
from pathlib import Path
from statistics import mean, median

from weekly_relative_pair_screen import BAR, COSTS, END, INITIAL, PAIR_GROSS, SPLIT, START, WEEK, epoch, funding_cash_rows, load_cached
from weekly_rotation_path_stress import cached_bars


STRESS_HURDLE = 4*sum(COSTS['stress'])


def select(spot, swap, rates, clocks, opening):
    cutoff = opening-BAR
    options = []
    for asset,series in spot.items():
        premia = [(swap[asset][stamp].close-series[stamp].close)/series[stamp].close
                   for stamp in range(cutoff-WEEK,cutoff,BAR)]
        first = bisect_left(clocks[asset],(cutoff-WEEK)*1000)
        last = bisect_left(clocks[asset],cutoff*1000)
        paid_to_short = math.fsum(rate for _,rate,_ in rates[asset][first:last])
        edge = premia[-1]-median(premia)+paid_to_short
        options.append((edge,asset,paid_to_short,premia[-1],median(premia)))
    edge,asset,funding,premium,normal = max(options)
    return dict(asset=asset,decision_utc_second=cutoff,estimated_edge=edge,
                previous_week_funding=funding,latest_premium=premium,trailing_premium_median=normal)


def value_path(spot, swap, rates, clocks, plan, first, last, fee, slip):
    asset = plan['asset']
    spot_mid = spot[asset][first].open
    swap_mid = swap[asset][first].opening
    spot_paid,short_received = spot_mid*(1+slip),swap_mid*(1-slip)
    units = PAIR_GROSS/spot_paid
    collateral = units*short_received
    required_cash = units*spot_paid*(1+fee)+collateral*(1+fee)
    funding = funding_cash_rows(rates[asset],clocks[asset],first,last,-1,units)
    weakest_collateral = collateral

    def value(spot_price, swap_price, stamp):
        nonlocal weakest_collateral
        sold,bought = spot_price*(1-slip),swap_price*(1+slip)
        fees = units*fee*(spot_paid+short_received+sold+bought)
        traded = units*(sold-spot_paid+short_received-bought)
        carry = math.fsum(amount for when,amount in funding if when<=stamp*1000)
        short_available = collateral+units*(short_received-bought-fee*(short_received+bought))+carry
        weakest_collateral = min(weakest_collateral,short_available)
        return dict(marked_net=traded-fees+carry,funding=carry,fees=fees)

    marks = [value(spot_mid,swap_mid,first)['marked_net']]
    for stamp in range(first,last,BAR):
        spot_bar,swap_bar = spot[asset][stamp],swap[asset][stamp]
        marks.append(value(spot_bar.high,swap_bar.low,stamp+BAR)['marked_net'])
        marks.append(value(spot_bar.low,swap_bar.high,stamp+BAR)['marked_net'])
    spot_exit,swap_exit = spot[asset][last].open,swap[asset][last].opening
    final = value(spot_exit,swap_exit,last)
    marks.append(final['marked_net'])
    basis_pnl = units*((swap_mid-spot_mid)-(swap_exit-spot_exit))
    slippage = units*(spot_mid*slip+swap_mid*slip+spot_exit*slip+swap_exit*slip)
    if not math.isclose(basis_pnl+final['funding']-final['fees']-slippage,final['marked_net'],abs_tol=1e-8):
        raise ValueError('Matching quantities must reconcile to basis, funding and costs')
    return dict(**plan,entry=first,exit=last,quantity=units,net=final['marked_net'],
                funding=final['funding'],basis_pnl=basis_pnl,fees=final['fees'],slippage=slippage,
                required_cash=required_cash,minimum_short_collateral=weakest_collateral),marks


def account(paths, *, cost_hurdle):
    equity = peak = INITIAL
    drawdown = drawdown_fraction = 0.
    ledger,all_weeks = [],[]
    collateral_failures,skipped_for_cash = 0,0
    for row,marks in paths:
        if (cost_hurdle and row['estimated_edge']<=STRESS_HURDLE) or equity<row['required_cash']:
            skipped_for_cash += equity<row['required_cash']
            all_weeks.append(0.)
            continue
        for change in marks:
            value = equity+change
            peak = max(peak,value)
            drawdown = max(drawdown,peak-value)
            drawdown_fraction = max(drawdown_fraction,(peak-value)/peak)
        equity += row['net']
        all_weeks.append(row['net'])
        ledger.append(row)
        collateral_failures += row['minimum_short_collateral']<=0
    net = equity-INITIAL
    return dict(weeks=len(all_weeks),active_weeks=len(ledger),net=net,mean_weekly_net=mean(all_weeks),
                max_drawdown=drawdown,max_drawdown_fraction=drawdown_fraction,short_reserve_exhaustions=collateral_failures,
                skipped_for_cash=skipped_for_cash,passes=net>drawdown and drawdown_fraction<=.08 and collateral_failures==0,
                **{f'total_{field}':math.fsum(row[field] for row in ledger) for field in ('basis_pnl','funding','fees','slippage')},
                ledger=ledger)


def report():
    spot,metadata = cached_bars('binance')
    futures = load_cached()
    swap = {name:{bar.t:bar for bar in futures[name][0]} for name in spot}
    rates = {name:futures[name][1] for name in spot}
    clocks = {name:[row[0] for row in series] for name,series in rates.items()}
    tables = {}
    for start,end in ((START,SPLIT),(SPLIT,END),(START,END)):
        weeks = [(stamp,select(spot,swap,rates,clocks,stamp)) for stamp in range(epoch(start)+BAR,epoch(end)+BAR,WEEK)]
        tables[start+'/'+end] = {}
        for lane,cost in COSTS.items():
            paths = [value_path(spot,swap,rates,clocks,signal,stamp,stamp+WEEK,*cost) for stamp,signal in weeks]
            tables[start+'/'+end][lane] = dict(always=account(paths,cost_hurdle=False),
                                                cost_hurdle=account(paths,cost_hurdle=True))
    return dict(historical_screen_only=True,promotion_allowed=False,initial_usdt=INITIAL,spot_nominal_usdt=PAIR_GROSS,
                prior_week_stress_cost_hurdle=STRESS_HURDLE,holding_seconds=WEEK,decision_delay_seconds=BAR,
                notes='Equal units. Adverse independent four-hour extrema approximate drawdown; short reserve check is not a liquidation model.',
                spot_source=metadata,swap_sources={name:futures[name][2] for name in spot},costs=COSTS,periods=tables)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    args.output.write_text(json.dumps(report(),indent=2,allow_nan=False)+'\n',encoding='utf-8')
