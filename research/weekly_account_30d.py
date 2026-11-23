"""Replay the BTC/ETH weekly rules from cash over the latest 30 full UTC days."""
import argparse
import hashlib
import json
import math
import tempfile
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timedelta, timezone
from functools import partial
from pathlib import Path
from urllib.parse import urlencode

from slow_spot_replay import BAR, STEPS, SYMBOLS, Bar, epoch, rotation_decisions, utc
from weekly_account_score import BINANCE_API, INITIAL
from weekly_profit_policy import COSTS, WeeklyTrend
from weekly_profit_start_windows import metrics
from weekly_profit_stop_confirmation import VolatilityCheckedStop
from weekly_profit_volatility import volatility
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_delayed_account import Bounds, bitstamp_fill, first_minute, request_json, run
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import MARKETS, fetch


def candles(venue, symbol, start, stop):
    if venue == 'bitstamp':
        cache = Path(tempfile.gettempdir()) / 'moneyrobert-bitstamp-30d'
        cache.mkdir(exist_ok=True)
        data, source = fetch(cache, (MARKETS[symbol], start, stop))
    else:
        query = urlencode(dict(symbol=symbol, interval='4h', startTime=start*1000,
                               endTime=stop*1000-1, limit=(stop-start)//BAR))
        url = f'{BINANCE_API}?{query}'
        raw = request_json(url)
        data = [Bar(row[0]//1000, *map(float, row[1:5])) for row in raw]
        if any(row[6] >= stop*1000 for row in raw):
            raise ValueError('A Binance candle remains open at the cutoff')
        source = dict(url=url, sha256=hashlib.sha256(json.dumps(raw).encode('ascii')).hexdigest())
    if [b.time for b in data] != list(range(start, stop, BAR)) or any(
        not 0 < b.low <= min(b.open, b.close) <= max(b.open, b.close) <= b.high < math.inf for b in data
    ):
        raise ValueError(f'Incomplete or invalid candle series: {venue} {symbol}')
    return data, source


def passive(symbol, bars, cost, price):
    fee, slip = cost
    opening = price((symbol, bars[0].time+60))
    paid = opening * (1+slip)
    units = math.floor(NOMINAL/paid/STEPS[symbol])*STEPS[symbol]
    notional = units*paid
    if notional < ORDER_MINIMUM:
        raise ValueError('Passive benchmark does not meet the modeled buy floor')
    cash = INITIAL - notional*(1+fee)
    factor = (1-slip)*(1-fee)
    bounds = Bounds()
    for bar in bars:
        bounds.mark(cash+units*bar.high*factor, cash+units*bar.low*factor)
    proceeds = units*bars[-1].close*factor
    return dict(net=cash+proceeds-INITIAL, drawdown_amount=bounds.drawdown_amount,
                drawdown_pct=100*bounds.drawdown, initial_notional=notional,
                final_sell_below_minimum=units*bars[-1].close*(1-slip)<ORDER_MINIMUM,
                underlying_return=bars[-1].close/opening-1)


def replay(venue, start, stop, bars, sources):
    first, end = epoch(start.isoformat()), epoch(stop.isoformat())
    origin = bars[SYMBOLS[0]][0].time
    first_index = (first-origin)//BAR
    decisions = []
    for i, (picks, _) in rotation_decisions(bars).items():
        stamp = origin+i*BAR
        if first <= stamp < end:
            decisions.append(dict(monday_utc=utc(stamp)[:10], ranked_candidate=picks[0] if picks else None,
                                  trailing_28d={s:v[i-1].close/v[i-169].close-1 for s,v in bars.items()}))
    if len(decisions) != sum((start+timedelta(days=i)).weekday() == 0 for i in range(30)):
        raise ValueError(f'Missing weekly decision for {venue}')

    bid_open = partial(minute_open, endpoint=BINANCE_API) if venue == 'binance' else bitstamp_fill
    price = cached_reader(venue, bid_open)
    pre_fill = first_minute(venue, binance_endpoint=BINANCE_API)

    def measure(symbol, boundary):
        return volatility(bars[symbol], (boundary-origin)//BAR)

    candidates = dict(fixed_immediate=WeeklyTrend,
                      scaled_immediate=partial(VolatilityCheckedStop, measure=measure, confirm=False),
                      scaled_confirmed=partial(VolatilityCheckedStop, measure=measure, confirm=True))
    results = {}
    for lane, cost in COSTS.items():
        accounts = {}
        for name, account in candidates.items():
            outcome = run(bars, start.isoformat(), stop.isoformat(), cost, price, pre_fill,
                          account_type=account, report_drawdown_amount=True)
            accounts[name] = dict(**metrics(outcome), ledger=outcome['ledger'],
                                  cutoff_close_used=any(t['reason']=='end' for t in outcome['ledger']))
        baselines = {s:passive(s, bars[s][first_index:], cost, price) for s in SYMBOLS}
        results[lane] = dict(accounts=accounts, passive=dict(cash=dict(net=0, drawdown_amount=0), **baselines))
    return dict(currency='USDT' if venue == 'binance' else 'USD', decisions=decisions, sources=sources,
                bars_per_symbol=len(bars[SYMBOLS[0]])-first_index, costs=results,
                actionable_fill_check='needed' if any(d['ranked_candidate'] for d in decisions) else 'no_order_signals')


def report(stop):
    start = stop-timedelta(days=30)
    warm = start-timedelta(days=30)
    first, end = epoch(warm.isoformat()), epoch(stop.isoformat())
    venues = ('binance', 'bitstamp')
    points = [(v, s, first, end) for v in venues for s in SYMBOLS]
    with ThreadPoolExecutor(len(points)) as pool:
        acquired = dict(zip(((v,s) for v,s,_,_ in points), pool.map(lambda p:candles(*p), points)))
    outcomes = {}
    for v in venues:
        bars = {s:acquired[v,s][0] for s in SYMBOLS}
        sources = {s:acquired[v,s][1] for s in SYMBOLS}
        outcomes[v] = replay(v, start, stop, bars, sources)
    return dict(start_utc=start.isoformat(), end_utc_exclusive=stop.isoformat(), warmup_start_utc=warm.isoformat(),
                initial_cash=INITIAL, days=30, passive_exit_price='last completed four-hour close', venues=outcomes)


if __name__ == '__main__':
    today = datetime.now(timezone.utc).date()
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--end', type=date.fromisoformat, default=today, help='Exclusive UTC date')
    cli.add_argument('--output', type=Path)
    args = cli.parse_args()
    if args.end > today or args.end-timedelta(days=60) < date(2024, 1, 1):
        cli.error('Use a closed UTC window with warm-up from 2024 onward')
    result = json.dumps(report(args.end), indent=2, allow_nan=False) + '\n'
    if args.output:
        args.output.write_text(result, encoding='utf-8')
    else:
        print(result, end='')
