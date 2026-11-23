"""Compare dollar-market order flow and repricing after large spot prints."""
import math
from collections import defaultdict
from datetime import date

from hf_pressure import rows
from liquidity_revisions import spot_trades
from sell_pressure import prefix, window


DAYS = tuple('2021-07-01 2021-11-01 2022-03-01 2022-06-01 2023-02-01 2023-12-01 2024-03-01 2024-08-01 2024-12-01 2025-02-01 2025-05-01 2025-11-01 2026-03-01 2026-07-01 2026-09-01 2026-11-01'.split())
USD_RULES = ('dollar_buys_lead', 'dollar_sells_lead', 'dollar_buys_confirmed')
PRINT_RULES = ('sell_print_recovers', 'sell_print_stays_low', 'buy_print_advance')
HOLD = 600
DAY = 86400


def midpoint(quote):
    return (quote[1] + quote[2]) / 2


def dollar_volume(day):
    start = (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY
    buy, sell, gaps = ([0.] * DAY for _ in range(3))
    maximum = -1
    last = -1
    seen = set()
    for row in rows(day, 'DOGE-USD', 'trades', 'coinbase'):
        stamp = int(row[3])
        second = stamp // 1_000_000 - start
        if stamp < last:
            raise ValueError('Unordered trade feed')
        last = stamp
        if not 0 <= second < DAY:
            continue
        ident = int(row[4])
        if ident in seen:
            continue
        seen.add(ident)
        if maximum >= 0 and ident != maximum + 1:
            gaps[second] = 1
        maximum = max(maximum, ident)
        notional = float(row[6]) * float(row[7])
        if notional <= 0 or not math.isfinite(notional) or row[5] not in ('buy', 'sell'):
            raise ValueError('Invalid dollar-market trade')
        (buy if row[5] == 'buy' else sell)[second] += notional
    return tuple(prefix(series) for series in (buy, sell, gaps))


def dollar_events(local, dollar, flow):
    buy, sell, gaps = flow
    events = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(330, len(local) - HOLD - 70, 30):
        if not all(quotes[index] for quotes in (local, dollar) for index in (sec - 30, sec)):
            continue
        if window(gaps, sec, 330):
            continue
        bought, sold = (window(series, sec, 30) for series in (buy, sell))
        activity = bought + sold
        usual = (window(buy, sec - 30, 300) + window(sell, sec - 30, 300)) / 10
        counts['eligible'] += 1
        if activity < max(10000, usual * 3):
            continue
        imbalance = (bought - sold) / activity
        remote = math.log(midpoint(dollar[sec]) / midpoint(dollar[sec - 30]))
        home = math.log(midpoint(local[sec]) / midpoint(local[sec - 30]))
        record = (sec, HOLD, imbalance, remote, home)
        if imbalance >= .6 and remote >= .0025:
            counts['shocks'] += 1
            if abs(home) <= .001:
                events['dollar_buys_lead'].append(record)
                counts['paired'] += 1
            elif home >= .0025:
                events['dollar_buys_confirmed'].append(record)
        elif imbalance <= -.6 and remote <= -.0025 and abs(home) <= .001:
            counts['shocks'] += 1
            events['dollar_sells_lead'].append(record)
    return events, counts


def print_events(quotes, tape, gaps):
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    next_second = 5
    for sec in sorted(tape):
        decision = sec + 8
        if sec < next_second or decision + HOLD + 70 >= len(quotes):
            continue
        if not quotes[sec - 1] or not quotes[decision]:
            continue
        anchor = midpoint(quotes[sec - 1])
        counts['eligible'] += 1
        arrivals = defaultdict(lambda: [0., 0.])
        for _, side, price, quantity in tape[sec]:
            money = price * quantity
            if money >= 25000:
                extreme, amount = arrivals[side]
                extreme = price if not amount else min(extreme, price) if side == 'sell' else max(extreme, price)
                arrivals[side] = [extreme, amount + money]
        if len(arrivals) != 1 or gaps[decision + 1] != gaps[sec - 1]:
            continue
        side, (trade_price, amount) = next(iter(arrivals.items()))
        impact = trade_price / anchor - 1
        if (side == 'sell' and impact > -.0015) or (side == 'buy' and impact < .0015):
            continue
        observed = [midpoint(quote) for quote in quotes[sec:decision + 1] if quote]
        if len(observed) < 7:
            continue
        travel = min(observed) / anchor - 1 if side == 'sell' else max(observed) / anchor - 1
        if abs(travel) < .001 or travel * impact <= 0:
            continue
        counts['shocks'] += 1
        close = midpoint(quotes[decision]) / anchor - 1
        recovery = 1 - close / travel
        record = (decision, HOLD, impact, recovery, amount)
        if side == 'sell':
            if recovery >= .5:
                counts['observed_recovery'] += 1
                signals['sell_print_recovers'].append(record)
            elif recovery <= .25:
                signals['sell_print_stays_low'].append(record)
        elif recovery <= .25:
            signals['buy_print_advance'].append(record)
        next_second = decision + 1
    return signals, counts


def print_recovery(day, quotes):
    return print_events(quotes, *spot_trades(day))
