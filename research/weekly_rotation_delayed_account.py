"""Recompute weekly decisions after replacing fills with observed minute openings."""
import argparse
import http.client
import json
import math
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, epoch, load
from weekly_rotation_forward import Account
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_delay_audit import ENDPOINT as BINANCE_ENDPOINT, minute_open
from weekly_rotation_review import BASE_COST, STRESS_COST
from weekly_rotation_venue_review import ENDPOINT as BITSTAMP_ENDPOINT, MARKETS, load_bitstamp


WINDOWS = [('2020-01-01', '2026-11-01'), ('2022-01-01', '2026-11-01')]
REFERENCE = Path(__file__).with_name('weekly_rotation_reentry_results.json')


def request_json(url):
    for attempt in range(5):
        try:
            with urllib.request.urlopen(url,timeout=18) as response:
                return json.load(response)
        except urllib.error.HTTPError as exc:
            if exc.code not in (429,500,502,503,504) or attempt==4:
                raise
            wait = exc.headers.get('Retry-After','')
            time.sleep(max(2*(attempt+1),int(wait) if wait.isdecimal() else 0))
        except (OSError,http.client.IncompleteRead):
            if attempt==4:
                raise
            time.sleep(2*(attempt+1))


def bitstamp_fill(point):
    symbol,stamp = point
    query = urllib.parse.urlencode(dict(step=60,limit=2,end=stamp+60))
    rows = request_json(f'{BITSTAMP_ENDPOINT}/{MARKETS[symbol]}/?{query}')['data']['ohlc']
    selected = [r for r in rows if int(r['timestamp'])==stamp]
    if len(selected)!=1:
        raise ValueError(f'Bitstamp did not report the execution minute: {point}')
    return float(selected[0]['open'])


def first_minute(venue, *, binance_endpoint=BINANCE_ENDPOINT):
    directory = Path(tempfile.gettempdir())/f'moneyrobert-cooldown-{venue}-pre-fill'
    directory.mkdir(exist_ok=True)

    def extrema(symbol, stamp):
        cache = directory/f'{symbol}-{stamp}.json'
        if cache.exists():
            low,high = json.loads(cache.read_text(encoding='ascii'))
        else:
            if venue=='binance':
                query = urllib.parse.urlencode(dict(symbol=symbol,interval='1m',startTime=stamp*1000,limit=1))
                row, = request_json(f'{binance_endpoint}?{query}')
                if row[0]!=stamp*1000:
                    raise ValueError('Binance minute starts at a different time')
                low,high = map(float,(row[3],row[2]))
            else:
                query = urllib.parse.urlencode(dict(step=60,limit=1,end=stamp))
                row, = request_json(f'{BITSTAMP_ENDPOINT}/{MARKETS[symbol]}/?{query}')['data']['ohlc']
                if int(row['timestamp'])!=stamp:
                    raise ValueError('Bitstamp minute starts at a different time')
                low,high = map(float,(row['low'],row['high']))
            if not 0<low<=high<math.inf:
                raise ValueError('Invalid first-minute price range')
            cache.write_text(json.dumps([low,high]),encoding='ascii')
        return low,high

    return extrema


class Bounds:
    def __init__(self):
        self.peak = 100.
        self.drawdown = 0.
        self.drawdown_amount = 0.

    def mark(self, high, low):
        self.peak = max(self.peak,high)
        self.drawdown = max(self.drawdown,1-low/self.peak)
        self.drawdown_amount = max(self.drawdown_amount,self.peak-low)

    def book(self, account, low, high):
        self.mark(account.cash+account.units*high*account.factor,
                  account.cash+account.units*low*account.factor)


def proxy_price(stamp, opening):
    return stamp*1000000,opening,opening,math.inf,math.inf


def run(bars,start,end,cost,execution=None,pre_fill=None,entry_check=None,*,
        account_type=Account,report_drawdown_amount=False,settle_at_next_fill=False,select_target=None):
    if settle_at_next_fill and execution is None:
        raise ValueError('Settlement at the next fill requires a price feed')
    origin = bars[SYMBOLS[0]][0].time
    first,stop = [(epoch(bound)-origin)//BAR for bound in (start,end)]
    delay = 60 if execution else 0
    account,bounds = account_type(*cost),Bounds()
    missing_held = 0
    entry_rejections = []
    for i in range(first,stop):
        stamp = origin+i*BAR
        current = {s:bars[s][i] for s in SYMBOLS}
        if any(b.open is None for b in current.values()):
            missing_held += bool(account.symbol)
            continue
        if all(bars[s][i-1].close is not None for s in SYMBOLS):
            mark = {s:bars[s][i-1].close for s in SYMBOLS}
            weekly = (stamp%86400==0 and (stamp//86400+3)%7==0
                      and all(bars[s][i-169].close is not None for s in SYMBOLS))
            target = None
            if weekly:
                returns = {s:mark[s]/bars[s][i-169].close-1 for s in SYMBOLS}
                if select_target:
                    target = select_target(returns,stamp)
                else:
                    best = max(returns,key=returns.get)
                    if returns[best]>0:
                        target = best
            # The previous close is available at the boundary in this historical
            # scenario. An order due at that boundary waits for the chosen delay.
            account.signal(stamp-1,mark,weekly,target)
            if account.sell and account.sell[0]==stamp:
                account.sell = stamp+delay,account.sell[1]
            if account.buy and account.buy[0]==stamp:
                candidate = account.buy[1]
                if entry_check and not entry_check(bars,candidate,i):
                    entry_rejections.append(dict(symbol=candidate,signal_time=stamp))
                    account.buy = None
                else:
                    account.buy = stamp+delay,candidate
        if account.sell and account.symbol and pre_fill:
            low,high = pre_fill(account.symbol,stamp)
            bounds.book(account,low,high)
        if account.sell or account.buy:
            symbols = {account.symbol} if account.sell else set()
            if account.buy:
                symbols.add(account.buy[1])
            fill = stamp+delay
            quotes = {s:proxy_price(fill,execution((s,fill)) if execution else current[s].open)
                      for s in symbols}
            closed_before = len(account.ledger)
            account.step(fill,quotes)
            if len(account.ledger)>closed_before:
                proceeds_equity = account.cash+(account.debit if account.symbol else 0.)
                bounds.mark(proceeds_equity,proceeds_equity)
        if account.symbol:
            b = current[account.symbol]
            bounds.book(account,b.low,b.high)
        else:
            bounds.mark(account.cash,account.cash)
    if account.symbol:
        stamp = epoch(end)
        symbol = account.symbol
        fill = stamp+delay if settle_at_next_fill else stamp
        if settle_at_next_fill and pre_fill:
            bounds.book(account,*pre_fill(symbol,stamp))
        opening = execution((symbol,fill)) if settle_at_next_fill else bars[symbol][stop-1].close
        account.sell = fill,'end'
        account.step(fill,{symbol:proxy_price(fill,opening)})
        if not account.symbol:
            bounds.mark(account.cash,account.cash)
    outcome = account.result()
    outcome['drawdown_at_order_marks_pct'] = outcome.pop('drawdown_pct')
    outcome.update(drawdown_upper_pct=100*bounds.drawdown,missing_4h_bars_held=missing_held)
    if report_drawdown_amount:
        outcome['drawdown_upper_amount'] = bounds.drawdown_amount
    if entry_check:
        outcome['entry_rejections'] = entry_rejections
    return outcome


def review(venue,report):
    bars = load_bitstamp()[0] if venue=='bitstamp' else load('2017-12-01',allow_aligned_gaps=True)
    execution = cached_reader(venue,bitstamp_fill if venue=='bitstamp' else minute_open)
    before = first_minute(venue)
    source = report['results'][venue]
    known_exits = sorted({(trade['symbol'],trade['exit'])
                          for start,end in WINDOWS for mode in ('base','stress')
                          for trade in source[start+'/'+end][mode]['accounts']['one_week']['ledger']
                          if trade['reason']!='end'})
    with ThreadPoolExecutor(5) as pool:
        for _ in pool.map(lambda point:before(*point),known_exits):
            pass
    result = {}
    for start,end in WINDOWS:
        span = start+'/'+end
        result[span] = {}
        for mode,cost in [('base',BASE_COST),('stress',STRESS_COST)]:
            baseline = report['results'][venue][span][mode]['accounts']['one_week']
            control = run(bars,start,end,cost)
            signature = lambda rows:[(x['symbol'],x['entry'],x['exit'],x['reason']) for x in rows]
            if signature(control['ledger'])!=signature(baseline['ledger']):
                raise ValueError(f'Undelayed decision paths differ for {venue} {span} {mode}')
            if not math.isclose(control['realized_net_usdt'],baseline['net'],abs_tol=1e-8):
                raise ValueError(f'Undelayed cash differs for {venue} {span} {mode}')
            outcome = run(bars,start,end,cost,execution,before)
            print(f'{venue} {span} {mode} finished',file=sys.stderr,flush=True)
            result[span][mode] = outcome
    return result


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    args = parser.parse_args()
    report = json.loads(REFERENCE.read_text(encoding='utf-8'))
    print(json.dumps(review(args.venue,report),indent=2))
