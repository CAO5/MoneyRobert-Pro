"""Replay a public Binance order-floor snapshot as a historical sensitivity scenario."""
import argparse
import json
import math
import sys
from functools import partial
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, epoch, load
from weekly_profit_cycle import WINDOWS, summarize
from weekly_profit_policy import COSTS, WEEK, WeeklyTrend
from weekly_profit_volatility import volatility
from weekly_profit_volatility_policy import DAILY_RISK_BUDGET, REFERENCE_DAILY_VOL
from weekly_rotation_delayed_account import first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM, STEPS
from weekly_rotation_reentry_delay import cached_reader


REFERENCE = Path(__file__).with_name('weekly_profit_volatility_results.json')
USDT_FLOOR_CHANGE = epoch('2023-08-31')+3*3600


def floor_at(second,after_change):
    return after_change if second>=USDT_FLOOR_CHANGE else ORDER_MINIMUM


class FloorAccount(WeeklyTrend):
    def __init__(self, fee, slip, measure, later_planning_floor, later_sell_floor):
        super().__init__(fee,slip)
        self.measure = measure
        self.later_planning_floor = later_planning_floor
        self.later_sell_floor = later_sell_floor
        self.budget_skips = []
        self.scaled_entries = []
        self.floor_blocks = []

    def signal(self,second,mark,weekly,target):
        super().signal(second,mark,weekly,target)
        if self.buy and self.buy[0]==second+1:
            boundary,symbol = self.buy
            sigma = self.measure(symbol,boundary)
            self.entry_floor = floor_at(boundary,self.later_planning_floor)
            self.buffer = self.entry_floor/((1-REFERENCE_DAILY_VOL)*(1-COSTS['stress'][1]))
            self.cap = None if sigma is None else min(NOMINAL,DAILY_RISK_BUDGET/max(sigma,REFERENCE_DAILY_VOL))
            if self.cap is None or self.cap<self.buffer:
                reason = 'missing_volatility' if self.cap is None else 'budget_below_exit_buffer'
                self.budget_skips.append(dict(symbol=symbol,boundary=boundary,sigma=sigma,reason=reason))
                self.buy = None

    def step(self,second,quotes):
        sell_floor = floor_at(second,self.later_sell_floor)
        if self.sell and second>=self.sell[0]:
            symbol = self.symbol
            quote = quotes[symbol]
            if quote and quote[0]//1_000_000>=self.sell[0] and quote[3]>=self.units:
                sell_notional = self.units*quote[1]*(1-self.slip)
                if sell_notional>=sell_floor:
                    credit = self.units*quote[1]*self.factor
                    reason = self.sell[1]
                    pnl = credit-self.debit
                    self.cash += credit
                    self.ledger.append(dict(symbol=symbol,entry=self.entered,exit=second,quantity=self.units,
                                            debit=self.debit,credit=credit,pnl=pnl,reason=reason))
                    if pnl<0 and reason in ('position_stop','net_profit_lock'):
                        self.recent_stop[symbol] = second
                    self.symbol = self.sell = None
                else:
                    self.floor_blocks.append(dict(symbol=symbol,fill_second=second,sell_notional=sell_notional))

        if self.buy and second>=self.buy[0]:
            scaled = self.cap<NOMINAL
            if scaled:
                self.mark_equity(quotes,sell_floor)
            if not self.buy_position(second,quotes) and scaled:
                return
        self.mark_equity(quotes,sell_floor)

    def buy_position(self,second,quotes):
        desired,symbol = self.buy
        self.buy = None
        quote = quotes[symbol]
        if not (quote and quote[0]//1_000_000>=desired and not self.halted
                and not self.symbol and second==desired):
            self.misses += 1
            return False
        price = quote[2]*(1+self.slip)
        step = STEPS[symbol]
        units = math.floor(self.cap/price/step)*step
        nominal = units*price
        debit = nominal*(1+self.fee)
        if self.cap<NOMINAL and nominal<self.buffer:
            self.budget_skips.append(dict(symbol=symbol,fill_second=second,cap=self.cap,
                                          rounded_notional=nominal,reason='rounding_below_exit_buffer'))
            return False
        if quote[4]<units or debit>self.cash or nominal<self.entry_floor:
            self.misses += 1
            return False
        self.symbol,self.units,self.entered = symbol,units,second
        self.basis,self.close_high,self.debit = price,price,debit
        self.cash -= debit
        if self.cap<NOMINAL:
            self.scaled_entries.append(dict(symbol=symbol,fill_second=second,cap=self.cap,notional=nominal))
        return True

    def mark_equity(self,quotes,sell_floor):
        equity = self.cash
        if self.symbol:
            quote = quotes[self.symbol]
            if (not quote or quote[3]<self.units
                    or self.units*quote[1]*(1-self.slip)<sell_floor):
                self.unpriced += 1
                self.current_unpriced += 1
                self.longest_unpriced = max(self.longest_unpriced,self.current_unpriced)
                return
            equity += self.units*quote[1]*self.factor
        self.current_unpriced = 0
        self.peak = max(self.peak,equity)
        self.dd = max(self.dd,1-equity/self.peak)

    def result(self):
        return dict(budget_skips=self.budget_skips,scaled_entries=self.scaled_entries,
                    floor_blocks=self.floor_blocks,**super().result())


def market_floor(snapshot):
    floors = []
    for symbol in SYMBOLS:
        row = snapshot['symbols'][symbol]
        filters = row['filters']
        rule = filters['NOTIONAL']
        if (row['status']!='TRADING' or not rule['applyMinToMarket']
                or float(filters['LOT_SIZE']['stepSize'])!=STEPS[symbol]):
            raise ValueError(f'Unsupported public filter snapshot for {symbol}')
        floors.append(float(rule['minNotional']))
    if not 0<floors[0]==floors[1]<ORDER_MINIMUM:
        raise ValueError('Expected a shared public order minimum below the model minimum')
    return floors[0]


def evaluate(snapshot):
    floor = market_floor(snapshot)
    cases = {'reference':(ORDER_MINIMUM,ORDER_MINIMUM),
             'exit_only':(ORDER_MINIMUM,floor), 'entry_and_exit':(floor,floor)}
    bars = load('2017-12-01',allow_aligned_gaps=True)
    origin = bars[SYMBOLS[0]][0].time
    price = cached_reader('binance',minute_open)
    pre_fill = first_minute('binance')
    reference = json.loads(REFERENCE.read_bytes())['venues']['binance']['results']

    def measure(symbol,boundary):
        return volatility(bars[symbol],(boundary-origin)//BAR)

    output = {}
    for start,end in WINDOWS:
        span = start+'/'+end
        output[span] = {}
        for lane,cost in COSTS.items():
            results = {}
            for name,(entry_floor,sell_floor) in cases.items():
                account = run(bars,start,end,cost,price,pre_fill,
                              account_type=partial(FloorAccount,measure=measure,later_planning_floor=entry_floor,
                                                   later_sell_floor=sell_floor),
                              report_drawdown_amount=True,settle_at_next_fill=True)
                result = summarize(account,start,end,False)
                result['late_closes'] = sum(t['exit']-t['entry']>WEEK for t in result['ledger'])
                result['passes_numeric_gate'] = (result['late_closes']==0 and result['profit_exceeds_drawdown']
                                                 and result['meets_drawdown_cap'])
                if name=='reference':
                    old = reference[span][lane]['scale_and_skip']
                    if any(result[k]!=v for k,v in old.items() if k!='paired_weekly_difference_95'):
                        raise ValueError(f'Order-floor control changed for {span} {lane}')
                results[name] = result
                print(span,lane,name,'completed',file=sys.stderr,flush=True)
            output[span][lane] = results
    return dict(snapshot=snapshot,model_order_minimum=ORDER_MINIMUM,quoted_order_minimum=floor,
                change_utc_sec=USDT_FLOOR_CHANGE,scenarios_after_change=cases,results=output)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('snapshot',type=Path,help='Public filter snapshot or an archived result containing it.')
    artifact = json.loads(parser.parse_args().snapshot.read_bytes())
    print(json.dumps(evaluate(artifact.get('snapshot',artifact)),indent=2))
