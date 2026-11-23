"""Compare immediate and confirmed four-hour stops in the same seven-day strategy."""
import argparse
import json
import sys
from functools import partial
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, exit_reason, load
from weekly_profit_cycle import WINDOWS, expectancy_interval, summarize
from weekly_profit_policy import COSTS, MONDAY_OFFSET, WEEK, WeeklyTrend
from weekly_profit_volatility import volatility
from weekly_profit_volatility_policy import VolatilityBudgetPolicy
from weekly_rotation_delayed_account import bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import load_bitstamp


REFERENCE = Path(__file__).with_name('weekly_profit_volatility_results.json')
NORMAL_STOP = .03
EMERGENCY_STOP = 2*NORMAL_STOP


class VolatilityCheckedStop(VolatilityBudgetPolicy,WeeklyTrend):
    def __init__(self,fee,slip,measure,confirm):
        super().__init__(fee,slip,measure,scale=True)
        self.confirm = confirm
        self.last_weak = None
        self.deferred = []

    def rotation_signal(self,second,mark,weekly,target):
        eligible = weekly and target is not None
        if self.symbol and not self.sell:
            close = mark[self.symbol]
            equity = self.cash+self.units*close*self.factor
            self.candle_peak = max(self.candle_peak,equity)
            if 1-equity/self.candle_peak>=.08:
                self.halted = True
                reason = 'account_drawdown'
            else:
                weak = close<=(1-NORMAL_STOP)*self.basis
                confirmed = (not self.confirm or close<=(1-EMERGENCY_STOP)*self.basis
                             or self.last_weak==(self.symbol,self.entered,second-BAR))
                self.last_weak = (self.symbol,self.entered,second) if weak else None
                exits = (self.symbol,) if weekly and target!=self.symbol else ()
                reason = exit_reason('rotation',self.symbol,exits,close,self.basis,0,
                                     stop_distance=NORMAL_STOP,stop_confirmed=confirmed)
                self.close_high = max(self.close_high,close)
                if not reason and self.close_high>=1.12*self.basis and close<=(self.basis+self.close_high)/2:
                    reason = 'profit_protection'
                if not reason and weak and not confirmed and (second+1)%WEEK!=MONDAY_OFFSET:
                    self.deferred.append(dict(symbol=self.symbol,entry=self.entered,signal_time=second+1,close=close))
            if reason:
                self.sell = second+1,reason
            eligible = eligible and reason=='rule'
        elif self.symbol:
            eligible = False
        else:
            self.candle_peak = max(self.candle_peak,self.cash)
        if eligible and not self.halted:
            if target in self.recent_stop and second<=self.recent_stop[target]+WEEK:
                self.reentry_skips += 1
            else:
                self.buy = second+1,target

    def result(self):
        return dict(deferred=self.deferred,**super().result())


def evaluate(venue):
    bitstamp = venue=='bitstamp'
    bars = load_bitstamp()[0] if bitstamp else load('2017-12-01',allow_aligned_gaps=True)
    origin = bars[SYMBOLS[0]][0].time
    price = cached_reader(venue,bitstamp_fill if bitstamp else minute_open)
    before = first_minute(venue)
    reference = json.loads(REFERENCE.read_bytes())['venues'][venue]['results']

    def measure(symbol,boundary):
        return volatility(bars[symbol],(boundary-origin)//BAR)

    results = {}
    for start,end in WINDOWS:
        span = start+'/'+end
        results[span] = {}
        for lane,cost in COSTS.items():
            original = reference[span][lane]['scale_and_skip']
            cases = {}
            for name,confirm in [('immediate',False),('confirmed',True)]:
                case = partial(VolatilityCheckedStop,measure=measure,confirm=confirm)
                account = run(bars,start,end,cost,price,before,account_type=case,
                              report_drawdown_amount=True,settle_at_next_fill=True)
                row = summarize(account,start,end,False)
                row['late_closes'] = sum(t['exit']-t['entry']>WEEK for t in row['ledger'])
                row['passes_numeric_gate'] = (row['late_closes']==0 and row['profit_exceeds_drawdown']
                                              and row['meets_drawdown_cap'])
                if name=='immediate':
                    if row['deferred'] or any(row[k]!=v for k,v in original.items() if k!='paired_weekly_difference_95'):
                        raise ValueError(f'Immediate stop control differs for {venue} {span} {lane}')
                else:
                    differences = [new-old for new,old in zip(row['weekly_pnl_by_entry'],original['weekly_pnl_by_entry'])]
                    row['paired_weekly_difference_95'] = expectancy_interval(differences)
                cases[name] = row
                print(venue,span,lane,name,'completed',file=sys.stderr,flush=True)
            results[span][lane] = cases
    return dict(venue=venue,reference=REFERENCE.name,normal_stop=NORMAL_STOP,emergency_stop=EMERGENCY_STOP,
                confirmation_gap_seconds=BAR,results=results)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
