"""Observe complete weeks on sealed Binance quotes using the registered trading rules."""
import hashlib
import json
import math
from datetime import datetime, timedelta, timezone
from pathlib import Path

from flow_forward import DAY, EPOCH, read_days
from flow_forward_review import frozen_start, sealed_endpoint
from slow_spot_replay import STEPS, SYMBOLS, utc
from weekly_account_score import INITIAL, summarize, week_row
from weekly_profit_policy import COSTS, WEEK
from weekly_profit_volatility_forward import VERSION as TRADING_VERSION
from weekly_profit_volatility_forward import MAX_QUOTE_GAP_SEC, QuotedVolatilityBudget, QuotedWeeklyTrend, with_volatility
from weekly_rotation_delayed_account import Bounds
from weekly_rotation_forward import NOMINAL, ORDER_MINIMUM


VERSION = Path(__file__).with_name('weekly_account_forward_version.json')
ENTRY_WINDOW = 60
MICROSECONDS = 1_000_000


class WeeklyMarks:
    def __init__(self, *args):
        super().__init__(*args)
        self.rows = []
        self.marked_equity = INITIAL
        self.week = Bounds()
        self.all_weeks = Bounds()
        self.opening = INITIAL

    def mark(self, equity):
        super().mark(equity)
        if self.opening is not None:
            self.week.mark(equity, equity)
        self.all_weeks.mark(equity, equity)

    def close_week(self, start):
        closing = self.marked_equity
        if self.opening is None or closing is None:
            self.rows.append(dict(week=utc(start), opening_equity=self.opening,
                                  closing_equity=closing, net=None))
        else:
            self.rows.append(week_row(start, self.opening, closing, self.week))
        self.opening = closing
        self.week = Bounds()
        if closing is not None:
            self.week.peak = closing

    def weekly_result(self):
        missing = sum(row['net'] is None for row in self.rows)
        return dict(unpriced_weeks=missing, weeks=self.rows,
                    score=summarize(self.rows, self.all_weeks) if self.rows and not missing else None)


class ObservedFixed(WeeklyMarks, QuotedWeeklyTrend):
    pass


class ObservedScaled(WeeklyMarks, QuotedVolatilityBudget):
    pass


class PassiveAccount:
    def __init__(self, symbol, start, fee, slip):
        self.symbol, self.start, self.fee, self.slip = symbol, start, fee, slip
        self.cash = INITIAL
        self.units = 0.
        self.entry = None
        self.unpriced_seconds = 0

    def mark(self, equity):
        pass

    def step(self, second, quotes):
        quote = quotes[self.symbol]
        if not self.units and second < self.start+ENTRY_WINDOW and quote and quote[0]//MICROSECONDS == second:
            price = quote[2]*(1+self.slip)
            step = STEPS[self.symbol]
            units = math.floor(NOMINAL/price/step)*step
            if units*price >= ORDER_MINIMUM and units <= quote[4]:
                self.units = units
                self.cash -= units*price*(1+self.fee)
                self.entry = second
        self.marked_equity = self.cash
        if self.units:
            gross = self.units*quote[1]*(1-self.slip) if quote and self.units <= quote[3] else 0
            if gross < ORDER_MINIMUM:
                self.marked_equity = None
                self.unpriced_seconds += 1
            else:
                self.marked_equity += gross*(1-self.fee)
        if self.marked_equity is not None:
            self.mark(self.marked_equity)


class ObservedPassive(WeeklyMarks, PassiveAccount):
    def weekly_result(self):
        result = super().weekly_result()
        if self.entry is None:
            result['score'] = None
        return dict(**result, entry_second=self.entry, missed_entry=self.entry is None,
                    unpriced_seconds=self.unpriced_seconds)


def observe(days, events, start, end):
    active = {}
    controls = {}
    for lane, cost in COSTS.items():
        active[lane] = dict(fixed=ObservedFixed(*cost), scaled=ObservedScaled(*cost))
        controls[lane] = {s:ObservedPassive(s, start, *cost) for s in SYMBOLS}
    books = [account for cases in (active, controls) for lane in cases.values() for account in lane.values()]
    gaps = dict.fromkeys(SYMBOLS, 0)
    max_gap = 0
    for day in days:
        for i in range(DAY):
            second = day.start+i
            quotes = {s:day.quotes[s][i] for s in SYMBOLS}
            for s, quote in quotes.items():
                gaps[s] = 0 if quote else gaps[s]+1
            max_gap = max(max_gap, *gaps.values())
            for account in books:
                account.step(second, quotes)
            for mark, weekly, target, boundary, sigma in events.get(second, ()):
                for pair in active.values():
                    pair['fixed'].signal(second, mark, weekly, target)
                    pair['scaled'].signal(second, mark, weekly, target, boundary, sigma)
        if (day.start+DAY-start) % WEEK == 0:
            for account in books:
                account.close_week(day.start+DAY-WEEK)

    if any(len(a.rows) != (end-start)//WEEK for a in books):
        raise ValueError('Forward simulation did not consume every complete week')
    costs = {}
    for lane in COSTS:
        passive = {s:a.weekly_result() for s, a in controls[lane].items()}
        candidates = {}
        for name, account in active[lane].items():
            observed = account.weekly_result()
            score = observed['score']
            comparisons = None
            if score is not None:
                comparisons = {'cash':score['mean_week_net']}
                comparisons.update({s:score['mean_week_net']-case['score']['mean_week_net']
                                    for s, case in passive.items() if case['score'] is not None})
            candidates[name] = dict(**observed, execution=account.result(),
                                    mean_week_net_advantage=comparisons)
        costs[lane] = dict(candidates=candidates, passive=passive)
    return dict(max_quote_gap_sec=max_gap, costs=costs)


def status(version=VERSION):
    start = frozen_start(version)
    registration = json.loads(version.read_text(encoding='utf-8'))
    decision_date = datetime.fromisoformat(registration['frozen_utc']).astimezone(timezone.utc).date()
    if start.weekday() or start <= decision_date or start < frozen_start(TRADING_VERSION):
        raise ValueError('The first full week must follow registration and the trading start date')
    trading = json.loads(TRADING_VERSION.read_text(encoding='utf-8'))
    books, candles = (Path(trading[key]) for key in ('book_archive', 'candle_archive'))
    inspected = (sealed_endpoint(books), sealed_endpoint(candles))
    sealed = min(inspected) if all(inspected) else start
    complete_weeks = max(0, (sealed-start).days//7)
    result = dict(start_utc_day=start.isoformat(), complete_weeks=complete_weeks,
                  rules_version_sha256=hashlib.sha256(TRADING_VERSION.read_bytes()).hexdigest())
    if not complete_weeks:
        return dict(state='awaiting_first_complete_week', **result)
    first, end = (start-EPOCH).days*DAY, (start-EPOCH).days*DAY+complete_weeks*WEEK
    audit = {'clock_jumps':0, 'filters':{}}
    results = observe(read_days(books, first, end, audit), with_volatility(candles, first, end), first, end)
    coverage = audit['clock_jumps'] == 0 and results['max_quote_gap_sec'] < MAX_QUOTE_GAP_SEC
    return dict(state='weekly_observations', **result, **results, archive_audit=audit, coverage_ok=coverage)


if __name__ == '__main__':
    print(json.dumps(status(), indent=2, allow_nan=False))
