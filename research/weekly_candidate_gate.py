"""Read-only Monday gate for the BTC/ETH volatility-sized rotation from cash."""
import argparse
import json
import math
from datetime import datetime, timezone
from pathlib import Path

from flow_forward import DAY, NS, sealed_rows
from slow_spot_replay import BAR, SYMBOLS, utc
from weekly_profit_volatility_policy import (DAILY_RISK_BUDGET, LOOKBACK_DAYS, MINIMUM_PLANNED_NOTIONAL,
                                            REFERENCE_DAILY_VOL, daily_volatility)
from weekly_rotation_forward import NOMINAL


MAX_PUBLISH_DELAY = 60
FIRST_WEEK = '2026-11-23'


def monday(value):
    stamp = datetime.strptime(value,'%Y-%m-%d').replace(tzinfo=timezone.utc)
    if stamp.weekday()!=0 or value<FIRST_WEEK:
        raise ValueError('Use a Monday on or after the frozen 2026-11-23 boundary')
    return int(stamp.timestamp())


def decision(closes, boundary):
    anchors = (boundary-BAR-LOOKBACK_DAYS*DAY,boundary-BAR)
    if any(stamp not in closes[symbol] for symbol in SYMBOLS for stamp in anchors):
        return dict(state='missing_closed_price_history')
    returns = {symbol:closes[symbol][anchors[1]]/closes[symbol][anchors[0]]-1 for symbol in SYMBOLS}
    target = max(returns,key=returns.get)
    if returns[target]<=0:
        return dict(state='stay_in_cash',trailing_return=returns)
    daily = [closes[target].get(second-BAR) for second in range(boundary-LOOKBACK_DAYS*DAY,boundary+DAY,DAY)]
    sigma = daily_volatility(daily)
    if sigma is None:
        return dict(state='missing_volatility_history',trailing_return=returns,target=target)
    cap = min(NOMINAL,DAILY_RISK_BUDGET/max(sigma,REFERENCE_DAILY_VOL))
    return dict(state='quote_check_only' if cap>=MINIMUM_PLANNED_NOTIONAL else 'below_nominal_buffer',
                trailing_return=returns,target=target,daily_volatility=sigma,max_nominal_usdt=cap)


def gate(archive, boundary, as_of_ns):
    deadline = boundary+MAX_PUBLISH_DELAY
    common = dict(version='volatility-sized BTC/ETH cash entry only',promotion_allowed=False,places_orders=False,
                  decision_boundary_utc=utc(boundary),receipt_deadline_utc_exclusive=utc(deadline))
    if as_of_ns < boundary*NS:
        return dict(**common,state='awaiting_week_boundary')
    closes = {symbol:{} for symbol in SYMBOLS}
    clocks = None
    selection = None
    for wall,mono,venue,kind,content in sealed_rows(archive):
        if clocks is not None and (wall<clocks[0] or mono<clocks[1]):
            raise ValueError('Candle receipt clocks moved backwards')
        clocks = wall,mono
        if wall>as_of_ns or wall>=deadline*NS or selection is not None:
            continue
        if venue!='binance' or kind not in ('rotation_warmup','rotation_candle'):
            continue
        event_boundary,responses = content
        if event_boundary>boundary:
            continue
        if event_boundary%BAR or set(responses)!=set(SYMBOLS):
            raise ValueError('Invalid candle boundary or symbol coverage')
        for symbol,response in responses.items():
            requested,received = response['request_ns'],response['received_ns']
            if not event_boundary*NS<=requested<=received<=wall:
                raise ValueError('Invalid candle publication timestamps')
            for row in response['rows']:
                opened = row[0]//1000
                closing = float(row[4])
                if (row[0]%(BAR*1000) or row[6]!=(opened+BAR)*1000-1 or opened+BAR>event_boundary
                        or not math.isfinite(closing) or closing<=0):
                    raise ValueError('Invalid or unclosed candle')
                previous = closes[symbol].setdefault(opened,closing)
                if previous!=closing:
                    raise ValueError('Previously published candle was revised')
        if event_boundary==boundary:
            if any(not any(row[0]==(boundary-BAR)*1000 for row in entry['rows']) for entry in responses.values()):
                raise ValueError('Weekly close is missing from the two-symbol snapshot')
            selection = dict(**decision(closes,boundary),receipt_utc_ns=wall,
                             earliest_quote_check_utc=utc((wall+NS-1)//NS+1))
    if selection is not None:
        return dict(**common,**selection)
    return dict(**common,state='missed_candle_receipt' if as_of_ns>=deadline*NS else 'awaiting_candle_receipt')


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('archive',type=Path)
    cli.add_argument('--week',type=monday,required=True)
    args = cli.parse_args()
    now = int(datetime.now(timezone.utc).timestamp()*NS)
    print(json.dumps(gate(args.archive,args.week,now),indent=2,allow_nan=False))
