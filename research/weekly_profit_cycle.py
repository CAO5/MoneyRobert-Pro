"""Evaluate seven-day spot trade cycles against net expectancy and cash drawdown."""
import argparse
import json
import math
import sys
from random import Random

from slow_spot_replay import epoch, load
from weekly_profit_policy import COSTS, MONDAY_OFFSET, WEEK, WeeklySettlement, WeeklyTrend
from weekly_rotation_delayed_account import bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_forward import Account
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import load_bitstamp


WINDOWS = (
    ('2020-01-06','2026-10-26'),
    ('2018-01-01','2020-01-06'),
    ('2020-01-06','2022-01-03'),
    ('2022-01-03','2024-01-01'),
    ('2024-01-01','2026-01-05'),
    ('2026-01-05','2026-10-26'),
)


CASES = {'rotation_reference':Account, 'weekly_protect':WeeklySettlement, 'weekly_trend':WeeklyTrend}


def expectancy_interval(weekly_pnl):
    """Percentiles of the mean under a stationary bootstrap with four-week mean blocks."""
    generator = Random(7)
    n = len(weekly_pnl)
    draws = []
    for _ in range(3000):
        index = generator.randrange(n)
        sample = [weekly_pnl[index]]
        for _ in range(n-1):
            if generator.random()<.25:
                index = generator.randrange(n)
            else:
                index = (index+1)%n
            sample.append(weekly_pnl[index])
        draws.append(math.fsum(sample)/n)
    draws.sort()
    return [draws[int(p*(len(draws)-1))] for p in (.025,.975)]


def summarize(account,start,end,max_one_week):
    start,end = epoch(start),epoch(end)
    weeks = (end-start)//WEEK
    ledger = account['ledger']
    trade_length = max((t['exit']-t['entry'] for t in ledger),default=0)
    if (account['pending'] or account['open_position'] or account['missed_entries']
            or (max_one_week and trade_length>WEEK)):
        raise ValueError('Incomplete execution or a trade exceeded its seven-day deadline')
    weekly_pnl = [0.]*weeks
    for trade in ledger:
        if (trade['entry']-60)%WEEK != MONDAY_OFFSET:
            raise ValueError('A historical entry fell outside the weekly decision minute')
        week = (trade['entry']-start)//WEEK
        weekly_pnl[week] += trade['pnl']
    net = account['realized_net_usdt']
    if not math.isclose(math.fsum(weekly_pnl),net,abs_tol=1e-8):
        raise ValueError('Weekly results do not sum to the closed cash ledger')
    drawdown = account['drawdown_upper_amount']
    return dict(calendar_weeks=weeks,avg_net_per_entry_week=net/weeks,
                weekly_expectancy_95=expectancy_interval(weekly_pnl),
                max_holding_sec=trade_length,net_above_drawdown_amount=net-drawdown,
                profit_exceeds_drawdown=net>drawdown,
                meets_drawdown_cap=account['drawdown_upper_pct']<=8,
                net_excluding_best_trade=net-max((t['pnl'] for t in ledger),default=0.),
                weekly_pnl_by_entry=weekly_pnl,**account)


def evaluate(venue):
    is_bitstamp = venue=='bitstamp'
    bars = load_bitstamp()[0] if is_bitstamp else load('2017-12-01',allow_aligned_gaps=True)
    price = cached_reader(venue,bitstamp_fill if is_bitstamp else minute_open)
    pre_fill = first_minute(venue)
    outcomes = {}
    for start,end in WINDOWS:
        span = start+'/'+end
        outcomes[span] = {}
        for mode,cost in COSTS.items():
            cases = {}
            for name,account_type in CASES.items():
                account = run(bars,start,end,cost,price,pre_fill,account_type=account_type,
                              report_drawdown_amount=True,settle_at_next_fill=True)
                cases[name] = summarize(account,start,end,account_type is not Account)
                print(f'{venue} {span} {mode} {name} done',file=sys.stderr,flush=True)
            outcomes[span][mode] = cases
    return dict(venue=venue,quote_currency='USD' if is_bitstamp else 'USDT',
                starting_cash=100,order_delay_sec=60,cost_fee_slippage=COSTS,results=outcomes)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
