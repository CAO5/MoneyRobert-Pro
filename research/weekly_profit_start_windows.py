"""Stress-test both stop rules from regularly spaced fresh account start dates."""
import argparse
import json
import math
import sys
from datetime import date, timedelta
from functools import partial

from slow_spot_replay import BAR, SYMBOLS, load
from weekly_profit_policy import COSTS, WEEK
from weekly_profit_stop_confirmation import VolatilityCheckedStop
from weekly_profit_volatility import volatility
from weekly_rotation_delayed_account import bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import load_bitstamp


FIRST = date(2018,1,1)
END = date(2026,10,26)
STRIDE_WEEKS = 13
LENGTHS = (26,52)
RULES = {'immediate':False,'confirmed':True}


def windows(length,first=FIRST,end=END):
    duration = timedelta(weeks=length)
    start = first
    while start+duration<=end:
        yield start.isoformat(),(start+duration).isoformat()
        start += timedelta(weeks=STRIDE_WEEKS)


def metrics(account):
    net = account['realized_net_usdt']
    closed = not (account['open_position'] or account['pending'])
    ledger = account['ledger']
    if not math.isclose(net,math.fsum(t['credit']-t['debit'] for t in ledger),abs_tol=1e-9):
        raise ValueError('Rolling window cash ledger disagrees with its net profit')
    amount = account['drawdown_upper_amount']
    pct = account['drawdown_upper_pct']
    longest = max((t['exit']-t['entry'] for t in ledger),default=0)
    size = len(ledger)
    return dict(net=net if closed else None,drawdown_amount=amount,drawdown_pct=pct,
                trades=size,missed_entries=account['missed_entries'],open_at_end=not closed,
                max_holding_sec=longest,missing_4h_bars_held=account['missing_4h_bars_held'],
                passes=bool(closed and not account['missed_entries'] and longest<=WEEK and net>amount and pct<=8))


def evaluate(venue):
    bitstamp = venue=='bitstamp'
    bars = load_bitstamp()[0] if bitstamp else load('2017-12-01',allow_aligned_gaps=True)
    origin = bars[SYMBOLS[0]][0].time
    price = cached_reader(venue,bitstamp_fill if bitstamp else minute_open)
    before = first_minute(venue)

    def measure(symbol,boundary):
        return volatility(bars[symbol],(boundary-origin)//BAR)

    results = {}
    for length in LENGTHS:
        batch = []
        for start,end in windows(length):
            lanes = {}
            for name,confirm in RULES.items():
                account = partial(VolatilityCheckedStop,measure=measure,confirm=confirm)
                lanes[name] = {lane:metrics(run(bars,start,end,cost,price,before,account_type=account,
                                               report_drawdown_amount=True,settle_at_next_fill=True))
                               for lane,cost in COSTS.items()}
            batch.append(dict(start=start,end=end,cases=lanes))
        print(venue,length,len(batch),'rolling accounts periods completed',file=sys.stderr,flush=True)
        results[length] = batch
    return dict(venue=venue,currency='USD' if bitstamp else 'USDT',stride_weeks=STRIDE_WEEKS,
                history_start=FIRST.isoformat(),history_end=END.isoformat(),results=results)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
