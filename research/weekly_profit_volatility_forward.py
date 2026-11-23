"""Compare the fixed and volatility-sized seven-day rules over the same quote tape."""
import json
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

from flow_forward import DAY, EPOCH, NS, read_days, sealed_rows
from flow_forward_review import MINIMUM_DAYS, MINIMUM_TRADES, frozen_start, sealed_endpoint
from slow_spot_replay import BAR, SYMBOLS
from weekly_profit_forward import MAX_QUOTE_GAP_SEC, MICROSECONDS, QuotedWeeklyTrend, qualifies
from weekly_profit_policy import COSTS
from weekly_profit_volatility_policy import LOOKBACK_DAYS, VolatilityBudgetPolicy, daily_volatility
from weekly_rotation_forward import ORDER_MINIMUM, half_size_filters, schedule


VERSION = Path(__file__).with_name('weekly_profit_volatility_forward_version.json')


class QuotedVolatilityBudget(VolatilityBudgetPolicy, QuotedWeeklyTrend):
    def __init__(self, fee, slip):
        super().__init__(fee,slip,lambda *_:self.sigma,scale=True)
        self.sigma = None

    def signal(self, second, mark, weekly, target, boundary, sigma):
        self.sigma = sigma
        attempts = len(self.budget_skips)
        super().signal(second,mark,weekly,target)
        if len(self.budget_skips)>attempts:
            skip = self.budget_skips[-1]
            skip['order_due_sec'] = skip.pop('boundary')
            skip['boundary'] = boundary


def with_volatility(candles,start,end):
    """Attach data available at each original candle receipt to its unchanged signal."""
    original = schedule(candles,start,end)
    closes = {symbol:{} for symbol in SYMBOLS}
    events = defaultdict(list)
    for wall,_,venue,kind,content in sealed_rows(candles):
        if wall//NS>=end:
            break
        if venue!='binance' or kind not in ('rotation_warmup','rotation_candle'):
            continue
        boundary,responses = content
        for symbol,response in responses.items():
            for row in response['rows']:
                opened = row[0]//1000
                if opened%DAY==DAY-BAR:
                    closes[symbol][opened+BAR] = float(row[4])
        received = (wall+NS-1)//NS
        if kind!='rotation_candle' or not start<=boundary<end or received>=end:
            continue
        index = len(events[received])
        mark,weekly,target = original[received][index]
        sigma = None
        if weekly and target:
            days = range(boundary-LOOKBACK_DAYS*DAY,boundary+DAY,DAY)
            sigma = daily_volatility([closes[target].get(day) for day in days])
        events[received].append((mark,weekly,target,boundary,sigma))
    if {stamp:len(rows) for stamp,rows in original.items()}!={stamp:len(rows) for stamp,rows in events.items()}:
        raise ValueError('Candle events changed between reads')
    return events


def exit_blocker(account,second,quotes):
    if not account.sell or second<account.sell[0]:
        return None
    quote = quotes[account.symbol]
    if not quote or quote[0]//MICROSECONDS<account.sell[0]:
        return 'unavailable_fresh_bid'
    if quote[3]<account.units:
        return 'insufficient_bid_quantity'
    if account.units*quote[1]*(1-account.slip)<ORDER_MINIMUM:
        return 'below_simulated_sell_minimum'
    raise ValueError('A pending sale with an executable bid was left unfilled')


def simulate(books,candles,start,end):
    events = with_volatility(candles,start,end)
    accounts = {name:{lane:account_type(*cost) for lane,cost in COSTS.items()}
                for name,account_type in [('fixed',QuotedWeeklyTrend),('scaled',QuotedVolatilityBudget)]}
    blocks = {name:{lane:Counter() for lane in COSTS} for name in accounts}
    accounts_and_blocks = [(account,blocks[name][lane]) for name,lanes in accounts.items()
                           for lane,account in lanes.items()]
    audit = {'clock_jumps':0,'filters':{}}
    quote_gap = dict.fromkeys(SYMBOLS,0)
    max_gap = days = 0
    for day in read_days(books,start,end,audit):
        for index in range(DAY):
            second = day.start+index
            quotes = {s:day.quotes[s][index] for s in SYMBOLS}
            for symbol,quote in quotes.items():
                quote_gap[symbol] = quote_gap[symbol]+1 if quote is None else 0
                max_gap = max(max_gap,quote_gap[symbol])
            for account,counts in accounts_and_blocks:
                account.step(second,quotes)
                reason = exit_blocker(account,second,quotes)
                if reason:
                    counts[reason] += 1
            for mark,weekly,target,boundary,sigma in events.get(second,()):
                for account in accounts['fixed'].values():
                    account.signal(second,mark,weekly,target)
                for account in accounts['scaled'].values():
                    account.signal(second,mark,weekly,target,boundary,sigma)
        days += 1
        if days>=MINIMUM_DAYS and all(len(lanes['base'].ledger)>=MINIMUM_TRADES for lanes in accounts.values()):
            break
    results = {name:{lane:dict(account.result(),exit_block_seconds=dict(blocks[name][lane]))
                     for lane,account in lanes.items()} for name,lanes in accounts.items()}
    return dict(results=results,scored_days=days,max_book_gap_sec=max_gap,book_audit=audit)


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
    counts = {case:lanes['base']['trades'] for case,lanes in report['results'].items()}
    if min(counts.values())<MINIMUM_TRADES:
        return dict(state='awaiting_trades',completed_trades=counts,**progress,**report)
    days = report['scored_days']
    filters = half_size_filters(books,first,first+days*DAY)
    coverage = (filters and report['book_audit']['clock_jumps']==0
                and report['max_book_gap_sec']<MAX_QUOTE_GAP_SEC)
    passes = {case:coverage and all(qualifies(account) for account in lanes.values())
              for case,lanes in report['results'].items()}
    return dict(state='reviewed',completed_trades=counts,gate_passed=passes,
                end_utc_exclusive=(start+timedelta(days=days)).isoformat(),
                observed_half_size_filters=filters,**progress,**report)


if __name__=='__main__':
    print(json.dumps(status(),indent=2))
