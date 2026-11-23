"""Read-only long/short breakout research on 2024+ Binance linear perpetuals."""
import argparse
import csv
import hashlib
import json
import math
import tempfile
from bisect import bisect_left, bisect_right
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from datetime import date, datetime, timedelta, timezone
from functools import lru_cache
from pathlib import Path
from statistics import mean, median, quantiles
from urllib.parse import urlencode
from urllib.request import urlopen

from macro_release import official_clock
from slow_spot_replay import BAR, epoch, utc
from weekly_account_score import block_intervals
from weekly_profit_policy import COSTS, WEEK
from weekly_rotation_delayed_account import Bounds, request_json


ASSETS = ('DOGEUSDT', 'ETHUSDT', 'BTCUSDT')
DATA_START = '2024-01-01'
SPLIT = '2026-01-01'
INITIAL = 100.
MAX_NOTIONAL = 25.
RISK_PER_ORDER = .75
MAX_DRAWDOWN = .08
DAY = 86400
API = 'https://fapi.binance.com/fapi/v1'
CURVE = 'https://fred.stlouisfed.org/graph/fredgraph.csv?id=T10Y2Y'
FOMC_SOURCE = 'https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm'
# Regularly scheduled statement dates, announced in advance. Eastern time is 14:00.
FOMC = {
    2024: '01-31 03-20 05-01 06-12 07-31 09-18 11-07 12-18',
    2025: '01-29 03-19 05-07 06-18 07-30 09-17 10-29 12-10',
    2026: '01-28 03-18 04-29 06-17 07-29 09-16 10-28 12-09',
}


@dataclass(frozen=True)
class Candle:
    t: int
    opening: float
    high: float
    low: float
    close: float
    turnover: float
    taker_buy: float


@dataclass(frozen=True)
class Signal:
    side: int
    atr: float
    strength: float


@dataclass
class Position:
    symbol: str
    side: int
    t: int
    price: float
    units: float
    atr: float
    curve: str
    fee: float
    funding: float = 0.


def digest(payload):
    return hashlib.sha256(json.dumps(payload, separators=(',', ':')).encode('ascii')).hexdigest()


def batches(symbol, endpoint, start, end):
    cache = Path(tempfile.gettempdir())/'moneyrobert-long-short'
    cache.mkdir(exist_ok=True)
    path = cache/f'{symbol}-{endpoint}-{start}-{end}.json'
    if path.exists():
        return json.loads(path.read_bytes())
    cursor = start*1000
    rows = []
    time_key = 0 if endpoint == 'klines' else 'fundingTime'
    while cursor < end*1000:
        params = dict(symbol=symbol, startTime=cursor, endTime=end*1000-1, limit=1000)
        if endpoint == 'klines':
            params['interval'] = '4h'
        part = request_json(f'{API}/{endpoint}?{urlencode(params)}')
        if not isinstance(part, list):
            raise ValueError(f'Unexpected venue response for {symbol} {endpoint}')
        if not part:
            break
        if part[0][time_key] < cursor or part[-1][time_key] >= end*1000:
            raise ValueError(f'Venue returned data outside the requested period for {symbol}')
        rows.extend(part)
        cursor = part[-1][time_key]+1
        if len(part) < 1000:
            break
    if not rows:
        raise ValueError(f'Missing {endpoint} for {symbol}')
    path.write_text(json.dumps(rows), encoding='ascii')
    return rows


def market(symbol, start, end):
    raw = batches(symbol, 'klines', start, end)
    funding = batches(symbol, 'fundingRate', start, end)
    bars = [Candle(r[0]//1000, *map(float, (*r[1:5], r[7], r[10]))) for r in raw]
    if [b.t for b in bars] != list(range(start, end, BAR)) or any(
        not (0 < b.low <= min(b.opening, b.close) <= max(b.opening, b.close) <= b.high < math.inf
             and 0 <= b.taker_buy <= b.turnover < math.inf) for b in bars
    ):
        raise ValueError(f'Incorrect or incomplete candles for {symbol}')
    rates = [(int(r['fundingTime']), float(r['fundingRate']), float(r['markPrice'])) for r in funding]
    times = [row[0] for row in rates]
    if (times != sorted(set(times)) or times[0] > (start+8*3600)*1000 or
            times[-1] < (end-9*3600)*1000 or any(b-a > (8*3600+60)*1000 for a,b in zip(times,times[1:])) or
            any(not math.isfinite(r) or not 0 < p < math.inf for _,r,p in rates)):
        raise ValueError(f'Incomplete or invalid funding for {symbol}')
    return bars, rates, dict(candles=len(bars), funding_events=len(rates),
                            candle_sha256=digest(raw), funding_sha256=digest(funding))


def inputs(stop):
    start, end = epoch(DATA_START), epoch(stop)
    with ThreadPoolExecutor(len(ASSETS)) as pool:
        markets = dict(zip(ASSETS, pool.map(lambda s:market(s, start, end), ASSETS)))
    info = request_json(f'{API}/exchangeInfo')
    limits = {}
    for s in info['symbols']:
        if s['symbol'] not in ASSETS:
            continue
        fields = {row['filterType']:row for row in s['filters']}
        market_rule = fields['MARKET_LOT_SIZE']
        limits[s['symbol']] = dict(step=float(market_rule['stepSize']), minimum_qty=float(market_rule['minQty']),
                                   minimum_notional=float(fields['MIN_NOTIONAL']['notional']), status=s['status'])
    if set(limits) != set(ASSETS) or any(r['status'] != 'TRADING' or r['step'] <= 0 for r in limits.values()):
        raise ValueError('Incomplete contract filters')
    url = f'{CURVE}&cosd={DATA_START}&coed={stop}'
    with urlopen(url, timeout=20) as response:
        payload = response.read()
    curve = [(epoch(row['observation_date'])+2*DAY, float(row['T10Y2Y']))
             for row in csv.DictReader(payload.decode('ascii').splitlines()) if row['T10Y2Y'] not in ('', '.')]
    if curve[-1][0] < end-8*DAY:
        raise ValueError('Treasury curve download ends too early')
    return markets, limits, curve, dict(url=url, sha256=hashlib.sha256(payload).hexdigest())


def release_hours():
    output = []
    for year, days in FOMC.items():
        for month_day in days.split():
            day = f'{year}-{month_day}'
            h, m = official_clock('fomc_statement', day)
            output.append(epoch(day)+h*3600+m*60)
    return output


def feature_series(bars, funding, family):
    times = [row[0] for row in funding]
    rates = [row[1] for row in funding]
    smoothed = [mean(rates[i-2:i+1]) for i in range(2, len(rates))]
    signals = {}
    counts = Counter()
    for i in range(120, len(bars)):
        stamp = bars[i].t
        last = bars[i-1]
        history = bars[i-43:i-1]
        central = mean(b.close for b in bars[i-120:i])
        if family == 'breakout':
            support, resistance = min(b.low for b in history), max(b.high for b in history)
            side = 1 if last.close > max(resistance, central) else -1 if last.close < min(support, central) else 0
        elif family == 'pullback':
            prior_level = mean(b.close for b in bars[i-13:i-1])
            week_ago = history[0].close
            above = last.close > max(central, week_ago, prior_level)
            below = last.close < min(central, week_ago, prior_level)
            side = 1 if above and last.low <= prior_level else -1 if below and last.high >= prior_level else 0
            support = resistance = prior_level
        else:
            raise ValueError(f'Unknown signal family: {family}')
        if side == 0:
            continue
        counts[f'technical_{side}'] += 1
        if side*math.fsum(2*b.taker_buy-b.turnover for b in bars[i-6:i]) <= 0:
            counts[f'flow_veto_{side}'] += 1
            continue
        low = max(2,bisect_left(times,(stamp-28*DAY)*1000))
        high = bisect_left(times,stamp*1000) - 1
        reference = smoothed[low-2:high-2]
        if len(reference) < 42:
            counts[f'funding_warmup_veto_{side}'] += 1
            continue
        deciles = quantiles(reference,n=10,method='inclusive')
        latest = smoothed[high-2]
        if (side == 1 and latest > deciles[-1]) or (side == -1 and latest < deciles[0]):
            counts[f'crowding_veto_{side}'] += 1
            continue
        atr = mean(max(b.high-b.low,abs(b.high-a.close),abs(b.low-a.close))
                   for a,b in zip(bars[i-15:i-1],bars[i-14:i]))
        if atr <= 0:
            raise ValueError('ATR must be positive')
        strength = (last.close-resistance if side == 1 else support-last.close)/atr
        signals[i] = Signal(side,atr,strength)
    return signals, counts


def week_start(stamp):
    return stamp - (stamp+3*DAY) % WEEK


def evaluate(bars, funding, signals, limits, curve, start, end, cost, sides, universe, execution=None, *, delay_seconds=60,
             notional_cap=MAX_NOTIONAL, risk_per_order=RISK_PER_ORDER):
    fee, slippage = cost
    bounds = Bounds()
    cash = INITIAL
    seat = None
    halted = False
    ledger = []
    weekly = {}
    rejects = Counter()
    news = release_hours()
    curve_times = [t for t,_ in curve]
    funding_times = {s:[t for t,_,_ in rates] for s,rates in funding.items()}
    origin = bars[universe[0]][0].t
    first,last = (start-origin)//BAR,(end-origin)//BAR
    if not 0 <= delay_seconds < BAR:
        raise ValueError('The execution delay must stay inside one candle')
    if not (math.isfinite(notional_cap) and math.isfinite(risk_per_order) and notional_cap > 0 and risk_per_order > 0):
        raise ValueError('Position limits must be positive and finite')
    delay = delay_seconds if execution else 0

    def mark(position, price):
        unwind = price*(1-position.side*slippage)
        return cash + position.side*position.units*(unwind-position.price)-position.units*unwind*fee

    def close(position, stamp, reference, reason):
        net_value = mark(position, reference)
        gross = position.side*position.units*(reference-position.price)
        ledger.append(dict(symbol=position.symbol, side='long' if position.side == 1 else 'short',
                           entry=position.t, exit=stamp, hours=(stamp-position.t)/3600, regime=position.curve,
                           notional=position.price*position.units, pnl=net_value-cash-position.fee+position.funding,
                           funding=position.funding, gross_before_exit_friction=gross, reason=reason))
        return net_value

    def collect_funding(position, left, right, *, exclude_right=False):
        nonlocal cash
        times = funding_times[position.symbol]
        begin = bisect_right(times,left*1000)
        finish = (bisect_left if exclude_right else bisect_right)(times,right*1000)
        for _,rate,reference in funding[position.symbol][begin:finish]:
            change = -position.side*position.units*reference*rate
            cash += change
            position.funding += change
            value = mark(position,reference)
            bounds.mark(value,value)

    opening_week = INITIAL
    for i in range(first,last):
        stamp = origin+i*BAR
        fill = stamp+delay
        if seat:
            tape = bars[seat.symbol]
            last_close = tape[i-1].close
            moved = seat.side*(last_close-seat.price)
            if bounds.drawdown >= MAX_DRAWDOWN:
                reason = 'account_drawdown'
            elif fill-seat.t >= WEEK:
                reason = 'seven_days'
            elif moved <= -2*seat.atr:
                reason = 'atr_stop'
            elif moved >= 3*seat.atr:
                reason = 'atr_target'
            else:
                reason = None
            if reason:
                if delay:
                    collect_funding(seat,stamp,fill)
                price = execution(seat.symbol,fill) if execution else tape[i].opening
                cash = close(seat, fill, price, reason)
                bounds.mark(cash,cash)
                seat = None
        halted = halted or bounds.drawdown >= MAX_DRAWDOWN

        if not seat and not halted:
            candidates = sorted(((signals[s][i],s) for s in universe if i in signals[s] and signals[s][i].side in sides),
                                key=lambda pair:-pair[0].strength)
            if candidates and any(abs(stamp-event) <= 6*3600 for event in news):
                rejects['fomc'] += 1
                candidates = []
            for signal,s in candidates:
                known = bisect_right(curve_times, stamp)
                if known < 20 or stamp-curve_times[known-1] > 7*DAY:
                    rejects['macro_unavailable'] += 1
                    break
                inverted = median(v for _,v in curve[known-20:known]) < 0
                scale = .5 if inverted else 1.
                opening = execution(s,fill) if execution else bars[s][i].opening
                price = opening*(1+signal.side*slippage)
                # Stop distance here is a sizing estimate. A close-based alert can fill worse.
                risk_per_unit = 2*signal.atr + 2*(fee+slippage)*price
                units = min(notional_cap*scale, cash)/price
                units = min(units, min(cash, INITIAL)*risk_per_order/INITIAL*scale/risk_per_unit)
                limit = limits[s]
                units = math.floor(units/limit['step'])*limit['step']
                notional = units*price
                if units < limit['minimum_qty'] or notional < limit['minimum_notional']:
                    rejects[f'{s}_size_below_floor'] += 1
                    continue
                paid = notional*fee
                cash -= paid
                seat = Position(s,signal.side,fill,price,units,signal.atr,
                                'inverted' if inverted else 'normal',paid)
                break

        marked = cash
        if seat:
            current = bars[seat.symbol][i]
            low,high = current.low,current.high
            adverse,favorable = (low,high) if seat.side == 1 else (high,low)
            bounds.mark(mark(seat,favorable),mark(seat,adverse))
            collect_funding(seat,max(stamp,seat.t),stamp+BAR,exclude_right=i == last-1)
            marked = mark(seat,current.close)
            bounds.mark(marked,marked)
        if i == last-1 and seat:
            cash = close(seat,end-1,bars[seat.symbol][i].close,'window_end')
            bounds.mark(cash,cash)
            marked = cash
        if week_start(stamp+BAR) != week_start(stamp) or i == last-1:
            weekly[week_start(stamp)] = marked-opening_week
            opening_week = marked

    if not math.isclose(cash-INITIAL,math.fsum(t['pnl'] for t in ledger),abs_tol=1e-8):
        raise ValueError('Cash and trade ledgers do not reconcile')
    if max((t['exit']-t['entry'] for t in ledger),default=0) > WEEK:
        raise ValueError('A trade exceeded the seven-day deadline')
    if not math.isclose(cash-INITIAL,math.fsum(weekly.values()),abs_tol=1e-8):
        raise ValueError('Weekly net results do not reconcile with the account')

    def side_stats(name):
        trades = [t for t in ledger if t['side'] == name]
        pnl = [t['pnl'] for t in trades]
        wins = [x for x in pnl if x > 0]
        losses = [x for x in pnl if x < 0]
        return dict(trades=len(trades), net=math.fsum(pnl), mean_per_trade=mean(pnl) if pnl else None,
                    profit_trades=len(wins), avg_win=mean(wins) if wins else None,
                    loss_trades=len(losses), avg_loss=-mean(losses) if losses else None,
                    funding=math.fsum(t['funding'] for t in trades),
                    without_best_three=math.fsum(pnl)-math.fsum(sorted(wins,reverse=True)[:3]))

    full_weeks = [v for t,v in weekly.items() if start <= t and t+WEEK <= end]
    return dict(net=cash-INITIAL, drawdown=bounds.drawdown_amount, drawdown_pct=100*bounds.drawdown,
                passed=(cash-INITIAL > bounds.drawdown_amount and bounds.drawdown <= MAX_DRAWDOWN), halted=halted,
                longs=side_stats('long'), shorts=side_stats('short'), rejects=dict(rejects),
                weekly_mean=mean(full_weeks) if full_weeks else None, complete_weeks=len(full_weeks),
                weekly_interval=block_intervals(full_weeks) if len(full_weeks) >= 26 else None,
                weekly_net={utc(t)[:10]:v for t,v in weekly.items()}, ledger=ledger)


@lru_cache(maxsize=None)
def minute_price(symbol, stamp):
    cache = Path(tempfile.gettempdir())/'moneyrobert-long-short'/'execution'
    cache.mkdir(exist_ok=True)
    file = cache/f'{symbol}-{stamp}.json'
    if file.exists():
        rows = json.loads(file.read_bytes())
    else:
        query = urlencode(dict(symbol=symbol,interval='1m',startTime=stamp*1000,limit=1))
        rows = request_json(f'{API}/klines?{query}')
    if len(rows) != 1 or rows[0][0] != stamp*1000 or not 0 < float(rows[0][1]) < math.inf:
        raise ValueError(f'Incorrect one-minute execution proxy for {symbol} at {stamp}')
    if not file.exists():
        file.write_text(json.dumps(rows),encoding='ascii')
    return float(rows[0][1])


def study(stop, delayed=False):
    markets, limits, curve, macro_source = inputs(stop)
    bars = {s:m[0] for s,m in markets.items()}
    funding = {s:m[1] for s,m in markets.items()}
    finish = epoch(stop)
    first = epoch('2024-02-05')
    ranges = {'continuous': (first,finish), 'before_2026': (first,epoch(SPLIT)), '2026': (epoch(SPLIT),finish),
              'recent_30d': (finish-30*DAY,finish)}
    result = {}
    execution_audit = {}
    scenarios = {'doge_long': ((1,), ('DOGEUSDT',)), 'doge_short': ((-1,), ('DOGEUSDT',)),
                 'doge_both': ((1,-1),('DOGEUSDT',)), 'portfolio_both': ((1,-1),ASSETS)}
    audits = {}
    originals = {family:{s:feature_series(bars[s],funding[s],family) for s in ASSETS}
                 for family in ('breakout','pullback')}
    btc = bars['BTCUSDT']
    breadth = {i:btc[i-1].close-mean(b.close for b in btc[i-120:i]) for i in range(120,len(btc))}
    for family in ('breakout','pullback','market_confirmed_breakout'):
        candidates = originals['breakout' if family == 'market_confirmed_breakout' else family]
        signals = {s:r[0] for s,r in candidates.items()}
        audits[family] = {s:dict(r[1]) for s,r in candidates.items()}
        if family == 'market_confirmed_breakout':
            signals = {s:{i:signal for i,signal in series.items() if signal.side*breadth[i] > 0}
                       for s,series in signals.items()}
            for s in ASSETS:
                audits[family][s]['market_confirmation_veto'] = len(candidates[s][0])-len(signals[s])
        result[family] = {period:{lane:{name:evaluate(bars,funding,signals,limits,curve,start,end,cost,sides,universe)
                                        for name,(sides,universe) in scenarios.items()} for lane,cost in COSTS.items()}
                          for period,(start,end) in ranges.items()}
        if delayed and family != 'pullback':
            points = {(t['symbol'], t[key]+60)
                      for lanes in result[family].values() for cases in lanes.values()
                      for name,a in cases.items() if name.startswith('doge_')
                      for t in a['ledger'] for key in ('entry','exit') if key=='entry' or t['reason']!='window_end'}
            with ThreadPoolExecutor(8) as pool:
                list(pool.map(lambda point:minute_price(*point),sorted(points)))
            execution_audit[family] = {period:{lane:{name:evaluate(bars,funding,signals,limits,curve,start,end,cost,sides,
                                                                   universe,execution=minute_price)
                                                       for name,(sides,universe) in scenarios.items() if name.startswith('doge_')}
                                              for lane,cost in COSTS.items()} for period,(start,end) in ranges.items()}
    return dict(data_start=DATA_START, end_utc_exclusive=stop, range_utc={k:[utc(a),utc(b)] for k,(a,b) in ranges.items()},
                cost_model=COSTS, limit_snapshot=limits, treasury=macro_source, news_calendar=FOMC_SOURCE,
                feed={s:m[2] for s,m in markets.items()}, signal_audit=audits, scenarios=result,
                doge_one_minute_delay=execution_audit)


if __name__ == '__main__':
    today = datetime.now(timezone.utc).date()
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--end',type=date.fromisoformat,default=today)
    cli.add_argument('--output',type=Path)
    cli.add_argument('--delayed',action='store_true',help='Audit DOGE breakout using observed one-minute opens')
    args = cli.parse_args()
    if not date.fromisoformat(SPLIT) < args.end <= today or args.end.year not in FOMC:
        cli.error('Use a completed UTC date covered by the FOMC schedule')
    payload = json.dumps(study(args.end.isoformat(),args.delayed),indent=2,allow_nan=False)+'\n'
    if args.output:
        args.output.write_text(payload,encoding='utf-8')
    else:
        print(payload,end='')
