"""Size weekly spot buys from previously observed daily volatility."""
import math
from statistics import stdev

from weekly_profit_policy import COSTS
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM, STEPS


LOOKBACK_DAYS = 28
REFERENCE_DAILY_VOL = .03
DAILY_RISK_BUDGET = NOMINAL*REFERENCE_DAILY_VOL
MINIMUM_PLANNED_NOTIONAL = ORDER_MINIMUM/((1-REFERENCE_DAILY_VOL)*(1-COSTS['stress'][1]))


def daily_volatility(closes):
    if len(closes)!=LOOKBACK_DAYS+1 or any(close is None or close<=0 for close in closes):
        return None
    return stdev(math.log(right/left) for left,right in zip(closes,closes[1:]))


class VolatilityBudgetPolicy:
    """Use an account's existing exit and cash marking logic for sized entries."""
    def __init__(self, fee, slippage, measure, scale):
        super().__init__(fee,slippage)
        self.measure = measure
        self.scale = scale
        self.selected_cap = None
        self.budget_skips = []
        self.scaled_entries = []

    def signal(self, second, mark, weekly, target):
        super().signal(second,mark,weekly,target)
        if self.buy and self.buy[0]==second+1:
            due,symbol = self.buy
            sigma = self.measure(symbol,due)
            cap = None if sigma is None else min(NOMINAL,DAILY_RISK_BUDGET/max(sigma,REFERENCE_DAILY_VOL))
            if cap is None or cap<MINIMUM_PLANNED_NOTIONAL:
                reason = 'missing_volatility' if cap is None else 'budget_below_exit_buffer'
                self.budget_skips.append(dict(symbol=symbol,boundary=due,sigma=sigma,reason=reason))
                self.buy = None
            else:
                self.selected_cap = cap if self.scale else NOMINAL

    def step(self, second, quotes):
        if self.buy is None or second<self.buy[0] or self.selected_cap==NOMINAL:
            super().step(second,quotes)
            return

        desired,symbol = self.buy
        self.buy = None
        super().step(second,quotes)
        quote = quotes[symbol]
        if not (quote and quote[0]//1_000_000>=desired and not self.halted
                and not self.symbol and second==desired):
            self.misses += 1
            return
        price = quote[2]*(1+self.slip)
        step = STEPS[symbol]
        units = math.floor(self.selected_cap/price/step)*step
        nominal = units*price
        debit = nominal*(1+self.fee)
        if nominal<MINIMUM_PLANNED_NOTIONAL:
            self.budget_skips.append(dict(symbol=symbol,fill_second=second,cap=self.selected_cap,
                                          rounded_notional=nominal,reason='rounding_below_exit_buffer'))
            return
        if quote[4]<units or debit>self.cash:
            self.misses += 1
            return
        self.symbol,self.units,self.entered = symbol,units,second
        self.basis,self.close_high,self.debit = price,price,debit
        self.cash -= debit
        self.scaled_entries.append(dict(symbol=symbol,fill_second=second,cap=self.selected_cap,notional=nominal))
        super().step(second,quotes)

    def result(self):
        return dict(budget_skips=self.budget_skips,scaled_entries=self.scaled_entries,**super().result())
