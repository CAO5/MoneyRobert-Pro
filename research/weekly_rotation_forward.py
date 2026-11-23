"""Score a frozen weekly spot rule from sealed quote and candle receipt archives."""
import argparse
import json
import math
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

from flow_forward import DAY, EPOCH, NS, intervals, read_days, sealed_rows
from flow_forward_review import frozen_start, sealed_endpoint
from slow_spot_replay import BAR, STEPS, SYMBOLS, exit_reason
from weekly_rotation_review import BASE_COST, STRESS_COST


VERSION = Path(__file__).with_name('weekly_rotation_forward_version.json')
NOMINAL = 12.25
ORDER_MINIMUM = 10.


def schedule(root, start, end):
    closes = {s:{} for s in SYMBOLS}
    events = defaultdict(list)
    boundaries = []
    previous = None
    for wall, mono, venue, kind, content in sealed_rows(root):
        if wall//NS>=end:
            break
        if previous and (wall<previous[0] or mono<previous[1]):
            raise ValueError('Archived candle receipt clocks moved backwards')
        previous = wall,mono
        if venue!='binance' or kind not in ('rotation_warmup','rotation_candle'):
            continue
        boundary, responses = content
        if set(responses)!=set(SYMBOLS) or not all(boundary*NS<=r['received_ns']<=wall for r in responses.values()):
            raise ValueError('Invalid candle receipt or missing symbol')
        for symbol,data in responses.items():
            for row in data['rows']:
                opened = row[0]//1000
                if opened+BAR>boundary:
                    raise ValueError('Unclosed candle in the archive')
                closes[symbol][opened] = float(row[4])
        received = (wall+NS-1)//NS
        if kind!='rotation_candle' or not start<=boundary<end or received>=end:
            continue
        boundaries.append(boundary)
        mark = {s:closes[s][boundary-BAR] for s in SYMBOLS}
        weekly = boundary%DAY==0 and datetime.fromtimestamp(boundary, timezone.utc).weekday()==0
        target = None
        if weekly:
            returns = {s:mark[s]/closes[s][boundary-BAR-28*DAY]-1 for s in SYMBOLS}
            best = max(returns,key=returns.get)
            if returns[best]>0:
                target = best
        events[received].append((mark,weekly,target))
    if boundaries != list(range(start,end,BAR)):
        raise ValueError('One or more closed candles lack a receipt before the requested end')
    return events


class Account:
    def __init__(self, fee, slip):
        self.fee, self.slip = fee, slip
        self.factor = (1-fee)*(1-slip)
        self.cash = self.peak = self.candle_peak = 100.
        self.dd = 0.
        self.symbol = self.buy = self.sell = None
        self.halted = False
        self.recent_stop = {}
        self.ledger = []
        self.misses = self.unpriced = self.reentry_skips = 0
        self.current_unpriced = self.longest_unpriced = 0

    def step(self, second, quotes):
        if self.sell and second>=self.sell[0]:
            q = quotes[self.symbol]
            if (q and q[0]//1000000>=self.sell[0] and q[3]>=self.units
                    and self.units*q[1]*(1-self.slip)>=ORDER_MINIMUM):
                credit = self.units*q[1]*self.factor
                reason = self.sell[1]
                pnl = credit-self.debit
                self.cash += credit
                self.ledger.append(dict(symbol=self.symbol,entry=self.entered,exit=second,quantity=self.units,
                                        debit=self.debit,credit=credit,pnl=pnl,reason=reason))
                if pnl<0 and reason in ('position_stop','net_profit_lock'):
                    self.recent_stop[self.symbol] = second
                self.symbol = self.sell = None
        if self.buy and second>=self.buy[0]:
            desired, symbol = self.buy
            self.buy = None
            quote = quotes[symbol]
            if (quote and quote[0]//1000000>=desired and not self.halted
                    and not self.symbol and second==desired):
                price = quote[2]*(1+self.slip)
                step = STEPS[symbol]
                units = math.floor(NOMINAL/price/step)*step
                nominal = units*price
                debit = nominal*(1+self.fee)
                if quote[4]>=units and nominal>=ORDER_MINIMUM and debit<=self.cash:
                    self.symbol,self.units,self.entered = symbol,units,second
                    self.basis,self.close_high,self.debit = price,price,debit
                    self.cash -= debit
            if self.symbol!=symbol or self.entered!=second:
                self.misses += 1
        equity = self.cash
        if self.symbol:
            q = quotes[self.symbol]
            if not q or q[3]<self.units or self.units*q[1]*(1-self.slip)<ORDER_MINIMUM:
                self.unpriced += 1
                self.current_unpriced += 1
                self.longest_unpriced = max(self.longest_unpriced,self.current_unpriced)
                return
            equity += self.units*q[1]*self.factor
        self.current_unpriced = 0
        self.peak = max(self.peak,equity)
        self.dd = max(self.dd,1-equity/self.peak)

    def signal(self, second, mark, weekly, target):
        eligible = weekly and target is not None
        if self.symbol and not self.sell:
            close = mark[self.symbol]
            estimated = self.units*close*self.factor
            equity = self.cash+estimated
            self.candle_peak = max(self.candle_peak,equity)
            if 1-equity/self.candle_peak>=.08:
                self.halted = True
                reason = 'account_drawdown'
            else:
                exits = (self.symbol,) if weekly and target!=self.symbol else ()
                reason = exit_reason('rotation',self.symbol,exits,close,self.basis,0)
                self.close_high = max(self.close_high,close)
                if not reason and self.close_high>=1.12*self.basis and close<=(self.basis+self.close_high)/2:
                    reason = 'profit_protection'
                if (not reason and self.units*self.close_high*self.factor>=1.03*self.debit
                        and estimated<=1.015*self.debit):
                    reason = 'net_profit_lock'
            if reason:
                self.sell = second+1,reason
                eligible = eligible and reason=='rule'
            else:
                eligible = False
        elif self.symbol:
            eligible = False
        else:
            self.candle_peak = max(self.candle_peak,self.cash)
        if eligible and not self.halted:
            if target in self.recent_stop and second<=self.recent_stop[target]+7*DAY:
                self.reentry_skips += 1
            else:
                self.buy = second+1,target

    def result(self):
        net = math.fsum(row['pnl'] for row in self.ledger)
        if not self.symbol and not math.isclose(self.cash-100,net,abs_tol=1e-8):
            raise ValueError('Closed cash ledger disagrees with realized profit')
        return dict(trades=len(self.ledger),wins=sum(t['pnl']>0 for t in self.ledger),
                    realized_net_usdt=net,drawdown_pct=100*self.dd,open_position=self.symbol,
                    pending=bool(self.buy or self.sell),missed_entries=self.misses,
                    unpriced_seconds=self.unpriced,longest_unpriced_sec=self.longest_unpriced,
                    reentry_skips=self.reentry_skips,ledger=self.ledger)


def simulate(books, candles, start, end):
    events = schedule(candles,start,end)
    accounts = {'base':Account(*BASE_COST),'stress':Account(*STRESS_COST)}
    audit = {'clock_jumps':0,'filters':{}}
    previous = None
    max_gap = 0
    for day in read_days(books,start,end,audit):
        gaps = day.longest_gap(previous)
        max_gap = max(max_gap,*(gaps[s] for s in SYMBOLS))
        previous = day
        for i in range(DAY):
            second = day.start+i
            q = {s:day.quotes[s][i] for s in SYMBOLS}
            for a in accounts.values():
                a.step(second,q)
                for event in events.get(second,()):
                    a.signal(second,*event)
    return dict(results={name:a.result() for name,a in accounts.items()},
                max_book_gap_sec=max_gap,book_audit=audit)


def half_size_filters(archive,start,end):
    opening = {}
    verified = {}
    for wall,_,venue,kind,content in sealed_rows(archive):
        second = wall//NS
        if second>=end:
            break
        if venue!='binance' or kind!='filters':
            continue
        row, = content[2]['symbols']
        s = row['symbol']
        if s not in SYMBOLS:
            continue
        f = {r['filterType']:r for r in row['filters']}
        lot, market = f['LOT_SIZE'],f['MARKET_LOT_SIZE']
        minimum = f.get('NOTIONAL',f.get('MIN_NOTIONAL',{}))
        ok = (row['status']=='TRADING' and float(lot['stepSize'])==STEPS[s]
              and float(lot['minQty'])<=STEPS[s] and float(market['minQty'])<=STEPS[s]
              and float(market['stepSize']) in (0.,STEPS[s])
              and float(minimum.get('minNotional','inf'))<=ORDER_MINIMUM)
        if second<=start:
            opening[s] = ok
        else:
            verified[s] = ok and verified.get(s,True)
    return all(opening.get(s) and verified.get(s,True) for s in SYMBOLS)


def daily_intervals(ledger, start, days):
    grouped = defaultdict(list)
    for entry in ledger:
        grouped[entry['exit']//DAY].append(entry['pnl'])
    rows = []
    for day in range(start//DAY,start//DAY+days):
        payoffs = grouped[day]
        rows.append((math.fsum(payoffs),sum(p>0 for p in payoffs),len(payoffs)))
    return intervals(rows)


def status(books,candles):
    start = frozen_start(VERSION)
    books_through, candles_through = (sealed_endpoint(path) for path in (books,candles))
    day = min(books_through,candles_through) if books_through and candles_through else None
    elapsed = max(0,(day-start).days) if day else 0
    result = dict(start_utc_day=start.isoformat(),sealed_days=elapsed)
    if elapsed<30:
        return dict(state='awaiting_days',**result)
    first = (start-EPOCH).days*DAY
    run = simulate(books,candles,first,first+elapsed*DAY)
    completed = run['results']['base']['trades']
    if completed<100:
        return dict(state='awaiting_trades',completed_trades=completed,**result)
    left,right = 30,elapsed
    while left<right:
        middle = (left+right)//2
        prefix = simulate(books,candles,first,first+middle*DAY)
        if prefix['results']['base']['trades']>=100:
            right,run = middle,prefix
        else:
            left = middle+1
    base,stress = [run['results'][name] for name in ('base','stress')]
    valid_filters = half_size_filters(books,first,first+right*DAY)
    passed = (base['wins']*2>base['trades'] and base['realized_net_usdt']>0
              and base['drawdown_pct']<=8 and stress['realized_net_usdt']>0
              and not any(a['open_position'] or a['pending'] or a['longest_unpriced_sec']>=60
                          for a in (base,stress))
              and run['max_book_gap_sec']<60 and run['book_audit']['clock_jumps']==0 and valid_filters)
    return dict(state='reviewed',completed_trades=base['trades'],
                end_utc_exclusive=(start+timedelta(days=right)).isoformat(),
                out_of_sample_intervals=daily_intervals(base['ledger'],first,right),
                observed_half_size_filters=valid_filters,gate_passed=passed,**result,**run)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('books',type=Path)
    parser.add_argument('candles',type=Path)
    args = parser.parse_args()
    print(json.dumps(status(args.books,args.candles),indent=2))
