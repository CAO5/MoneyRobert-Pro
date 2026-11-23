"""Replay seven-day BTC/ETH trades with sealed receipt-time quotes and cash risk."""
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path

from flow_forward import DAY, EPOCH, read_days
from flow_forward_review import MINIMUM_DAYS, MINIMUM_TRADES, frozen_start, sealed_endpoint
from slow_spot_replay import SYMBOLS
from weekly_profit_policy import COSTS, WEEK, WeeklyTrend
from weekly_rotation_forward import half_size_filters, schedule


VERSION = Path(__file__).with_name('weekly_profit_forward_version.json')
MICROSECONDS = 1_000_000
EXIT_LEAD_SEC = 60
MAX_QUOTE_GAP_SEC = 60
MAX_DRAWDOWN_PCT = 8.
INITIAL_USDT = 100.


class QuotedWeeklyTrend(WeeklyTrend):
    def __init__(self, fee, slip):
        super().__init__(fee,slip)
        self.entry_quote_us = None
        self.marked_equity = self.cash
        self.quote_peak = self.cash
        self.cash_drawdown = self.quote_drawdown_pct = 0.
        self.longest_holding_us = self.late_exits = 0
        self.overdue = False

    def signal(self, second, mark, weekly, target):
        self.rotation_signal(second,mark,weekly,target)
        if weekly:
            self.settle_week(second,weekly,target)

    def mark(self, equity):
        self.quote_peak = max(self.quote_peak,equity)
        self.cash_drawdown = max(self.cash_drawdown,self.quote_peak-equity)
        self.quote_drawdown_pct = max(self.quote_drawdown_pct,100*(1-equity/self.quote_peak))

    def step(self, second, quotes):
        previous_symbol = self.symbol
        previous_entry_us = self.entry_quote_us
        ledger_size = len(self.ledger)
        if self.symbol and self.sell is None and second >= self.entered+WEEK-EXIT_LEAD_SEC:
            self.sell = self.entered+WEEK-EXIT_LEAD_SEC,'weekly_expiry'
        super().step(second,quotes)
        if len(self.ledger)>ledger_size:
            exit_us = quotes[previous_symbol][0]
            holding = exit_us-previous_entry_us
            self.ledger[-1].update(entry_quote_us=previous_entry_us,exit_quote_us=exit_us)
            self.longest_holding_us = max(self.longest_holding_us,holding)
            self.late_exits += holding>WEEK*MICROSECONDS
            if self.symbol:
                self.mark(self.cash+self.debit)
        if self.symbol and self.entered==second:
            self.entry_quote_us = quotes[self.symbol][0]
        elif not self.symbol:
            self.entry_quote_us = None
        self.overdue = (self.entry_quote_us is not None and
                        (second+1)*MICROSECONDS-1-self.entry_quote_us >= WEEK*MICROSECONDS)
        self.marked_equity = None if self.current_unpriced else self.cash
        if self.marked_equity is not None:
            if self.symbol:
                self.marked_equity += self.units*quotes[self.symbol][1]*self.factor
            self.mark(self.marked_equity)

    def result(self):
        outcome = super().result()
        outcome.update(drawdown_pct=self.quote_drawdown_pct,drawdown_usdt=self.cash_drawdown,
                       exit_deadline_breaches=self.late_exits+int(self.overdue),
                       max_closed_holding_us=self.longest_holding_us,
                       liquidatable_net_usdt=(None if self.marked_equity is None
                                              else self.marked_equity-INITIAL_USDT))
        return outcome


def simulate(books, candles, start, end):
    events = schedule(candles,start,end)
    accounts = {name:QuotedWeeklyTrend(*cost) for name,cost in COSTS.items()}
    audit = {'clock_jumps':0,'filters':{}}
    quote_gap = dict.fromkeys(SYMBOLS,0)
    max_gap = 0
    days = 0
    for day in read_days(books,start,end,audit):
        for i in range(DAY):
            second = day.start+i
            quotes = {symbol:day.quotes[symbol][i] for symbol in SYMBOLS}
            for symbol,quote in quotes.items():
                quote_gap[symbol] = quote_gap[symbol]+1 if quote is None else 0
                max_gap = max(max_gap,quote_gap[symbol])
            for account in accounts.values():
                account.step(second,quotes)
            for event in events.get(second,()):
                for account in accounts.values():
                    account.signal(second,*event)
        days += 1
        if days>=MINIMUM_DAYS and len(accounts['base'].ledger)>=MINIMUM_TRADES:
            break
    return dict(results={name:account.result() for name,account in accounts.items()},
                scored_days=days,max_book_gap_sec=max_gap,book_audit=audit)


def qualifies(outcome):
    liquidation = outcome['liquidatable_net_usdt']
    return (liquidation is not None
            and min(liquidation,outcome['realized_net_usdt'])>outcome['drawdown_usdt']
            and outcome['drawdown_pct']<=MAX_DRAWDOWN_PCT
            and not outcome['exit_deadline_breaches'] and not outcome['missed_entries']
            and outcome['longest_unpriced_sec']<MAX_QUOTE_GAP_SEC)


def status(version=VERSION):
    start = frozen_start(version)
    registration = json.loads(version.read_text(encoding='utf-8'))
    if datetime.fromisoformat(registration['frozen_utc']).astimezone(timezone.utc).date()>=start:
        raise ValueError('The prospective window must follow the UTC registration day')
    books,candles = (Path(registration[key]) for key in ('book_archive','candle_archive'))
    through = [sealed_endpoint(archive) for archive in (books,candles)]
    elapsed = max(0,(min(through)-start).days) if all(through) else 0
    progress = dict(start_utc_day=start.isoformat(),sealed_days=elapsed)
    if elapsed<MINIMUM_DAYS:
        return dict(state='awaiting_days',**progress)
    first = (start-EPOCH).days*DAY
    report = simulate(books,candles,first,first+elapsed*DAY)
    count = report['results']['base']['trades']
    if count<MINIMUM_TRADES:
        return dict(state='awaiting_trades',completed_trades=count,**progress,**report)
    days = report['scored_days']
    filters = half_size_filters(books,first,first+days*DAY)
    passed = (filters and report['book_audit']['clock_jumps']==0
              and report['max_book_gap_sec']<MAX_QUOTE_GAP_SEC
              and all(qualifies(account) for account in report['results'].values()))
    return dict(state='reviewed',completed_trades=count,gate_passed=passed,
                end_utc_exclusive=(start+timedelta(days=days)).isoformat(),
                observed_half_size_filters=filters,**progress,**report)


if __name__=='__main__':
    print(json.dumps(status(),indent=2))
