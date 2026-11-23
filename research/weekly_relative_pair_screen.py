"""Screen weekly relative-strength pairs using cached prices and settled funding only."""
import argparse
import json
import math
import tempfile
from bisect import bisect_left, bisect_right
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean

from long_short_expectancy import ASSETS, BAR, DATA_START, epoch, market
from weekly_profit_policy import COSTS, WEEK


CACHE_END = '2026-11-20'
START = '2024-02-05'
SPLIT = '2026-01-05'
END = '2026-11-16'
LOOKBACK = 4*WEEK
INITIAL = 100.
PAIR_GROSS = 25.
SETTLEMENT_GUARD_MS = 60_000


def load_cached():
    start,end = epoch(DATA_START),epoch(CACHE_END)
    root = Path(tempfile.gettempdir())/'moneyrobert-long-short'
    expected = [root/f'{symbol}-{endpoint}-{start}-{end}.json' for symbol in ASSETS
                for endpoint in ('klines','fundingRate')]
    if missing := [str(path) for path in expected if not path.is_file()]:
        raise ValueError(f'Cached data required; not downloading: {missing[0]}')
    return {name:market(name,start,end) for name in ASSETS}


def funding_cash_rows(rates, timestamps, start, end, side, units):
    opening,closing = start*1000,end*1000
    first,last = bisect_left(timestamps,opening),bisect_right(timestamps,closing)
    rows = []
    for stamp,rate,mark in rates[first:last]:
        carry = -side*units*mark*rate
        if min(stamp-opening,closing-stamp) <= SETTLEMENT_GUARD_MS:
            carry = min(0.,carry)
        rows.append((stamp,carry))
    return rows


def direction_order(bars, opening):
    signal = opening-BAR
    prior = signal-BAR
    return sorted(ASSETS,key=lambda asset:(math.log(bars[asset][prior].close/bars[asset][prior-LOOKBACK].close),asset))


def trade_week(bars, rates, clocks, opening, closing, long_asset, short_asset, fee, slip):
    positions = ((long_asset,1,PAIR_GROSS/2),(short_asset,-1,PAIR_GROSS/2))
    return trade_positions(bars,rates,clocks,opening,closing,positions,fee,slip)


def trade_positions(bars, rates, clocks, opening, closing, positions, fee, slip, *, intrabar=False):
    legs = []
    for asset,side,notional in positions:
        entry = bars[asset][opening].opening*(1+side*slip)
        units = notional/entry
        carry = funding_cash_rows(rates[asset],clocks[asset],opening,closing,side,units)
        legs.append((asset,side,entry,units,carry))

    def mark(stamp, *, closing_trade=False, opening_quote=False, extreme=None):
        gross,carry,cost = 0.,0.,0.
        for asset,side,entry,units,funding in legs:
            mid = bars[asset][stamp].opening if closing_trade or opening_quote else bars[asset][stamp-BAR].close
            if extreme:
                bar = bars[asset][stamp-BAR]
                mid = bar.high if extreme*side > 0 else bar.low
            exit_price = mid*(1-side*slip)
            gross += side*units*(exit_price-entry)
            cost += fee*units*(entry+exit_price)
            carry += math.fsum(amount for release,amount in funding if release <= stamp*1000)
        return dict(trade_pnl_after_slippage=gross,fees=cost,funding=carry,net=gross+carry-cost)

    marks = [mark(opening,opening_quote=True)['net']]
    for stamp in range(opening+BAR,closing+1,BAR):
        if intrabar:
            marks.extend(mark(stamp,extreme=sign)['net'] for sign in (1,-1))
        marks.append(mark(stamp)['net'])
    final = mark(closing,closing_trade=True)
    marks.append(final['net'])
    return marks, final


def assess(bars, rates, clocks, first, last, reverse, cost):
    cash = peak = INITIAL
    drawdown = drawdown_pct = 0.
    weeks = []
    for opening in range(first+BAR,last+BAR,WEEK):
        closing = opening+WEEK
        laggards = direction_order(bars,opening)
        short_asset,long_asset = (laggards[-1],laggards[0]) if reverse else (laggards[0],laggards[-1])
        marks,result = trade_week(bars,rates,clocks,opening,closing,long_asset,short_asset,*cost)
        for value in marks:
            equity = cash+value
            peak = max(peak,equity)
            drawdown = max(drawdown,peak-equity)
            drawdown_pct = max(drawdown_pct,(peak-equity)/peak)
        cash += result['net']
        weeks.append(dict(entered_utc_second=opening,exited_utc_second=closing,long_asset=long_asset,
                          short_asset=short_asset,**result))
    return dict(weeks=len(weeks),average_weekly_net=mean(row['net'] for row in weeks),net=cash-INITIAL,
                max_drawdown=drawdown,max_drawdown_fraction=drawdown_pct,
                historical_screen_pass=(cash-INITIAL>drawdown and drawdown_pct<=.08),ledger=weeks)


def report():
    inputs = load_cached()
    bars = {name:{row.t:row for row in data[0]} for name,data in inputs.items()}
    rates = {name:data[1] for name,data in inputs.items()}
    clocks = {name:[row[0] for row in series] for name,series in rates.items()}
    reports = {}
    for first,last in ((START,SPLIT),(SPLIT,END),(START,END)):
        reports[first+'/'+last] = {
            name:{lane:assess(bars,rates,clocks,epoch(first),epoch(last),reverse,cost) for lane,cost in COSTS.items()}
            for name,reverse in (('long_leader_short_laggard',False),('reverse_control',True))}
    return dict(historical_screen_only=True,promotion_allowed=False,initial_usdt=INITIAL,pair_gross_usdt=PAIR_GROSS,
                signal='Previous 28 days, rank by return, then wait four hours before entering',hold_seconds=WEEK,
                pricing='4-hour opening proxy, adverse slippage, 4-hour mark-to-liquidation drawdown proxy',
                limitations='Equal dollar legs are not beta-neutral. No lot sizes, collateral or actual fills modeled.',
                sources={name:data[2] for name,data in inputs.items()},costs=COSTS,periods=reports,
                completed_utc=datetime.now(timezone.utc).isoformat())


if __name__=='__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    args.output.write_text(json.dumps(report(),indent=2,allow_nan=False)+'\n',encoding='utf-8')
