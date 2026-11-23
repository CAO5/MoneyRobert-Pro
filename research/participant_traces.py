"""Two anonymous, public tape hypotheses. Independent daily paper ledgers only."""
import csv
import heapq
import math
from collections import defaultdict, deque
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book


ROOT = Path(__file__).parent
DAYS = tuple(f'2022-{month:02}-01' for month in range(4, 10))
SECOND = 1_000_000
RULES = ('bid_refill', 'ask_refill', 'buy_schedule', 'sell_schedule')


def stream(day, kind):
    previous = 0
    for row in rows(day, 'DOGEUSDT', kind):
        received, exchange = int(row[3]), int(row[2])
        if received < previous:
            raise ValueError(f'Unordered receipt stream: {kind} {day}')
        previous = received
        if kind == 'trades':
            yield received, 0, exchange, (row[4], row[5], float(row[6]), float(row[7]))
        else:
            yield received, 1, exchange, tuple(map(float, row[4:8]))


def discover(events, origin):
    levels = {}
    clips = defaultdict(lambda: deque(maxlen=4))
    signalled = defaultdict(list)
    previous_quote = previous_exchange = None
    last_id = None
    for received, is_quote, exchange, data in events:
        if not origin <= received < origin + 86400 * SECOND:
            continue
        sec = (received - origin) // SECOND
        if is_quote:
            ask_size, ask, bid, bid_size = data
            if (not 0 < bid < ask or min(ask_size, bid_size) <= 0 or
                    (previous_exchange is not None and exchange < previous_exchange)):
                levels.clear()
                previous_quote = previous_exchange = None
                continue
            for side, price, size in (('sell', bid, bid_size), ('buy', ask, ask_size)):
                old = levels.get(side)
                if (old is None or price != old['price'] or received - old['start'] > 30 * SECOND or
                        received - previous_quote > 2 * SECOND):
                    levels[side] = dict(price=price, start=received, display=size, initial=size,
                                        additions=0., traded=0., prints=0, last=-1, emitted=False)
                    continue
                if old['prints'] and exchange > old['last']:
                    old['additions'] += max(0., size - old['display'])
                old['display'] = size
                if (not old['emitted'] and old['prints'] >= 3 and exchange > old['last'] and
                        old['traded'] >= 2 * old['initial'] and old['traded'] * price >= 2000 and
                        old['additions'] >= .5 * old['initial']):
                    rule = 'bid_refill' if side == 'sell' else 'ask_refill'
                    signalled[rule].append((sec, 600, price, old['traded'], old['prints']))
                    old['emitted'] = True
            previous_quote, previous_exchange = received, exchange
        else:
            trade_id, side, price, size = data
            if side not in ('buy', 'sell') or min(price, size) <= 0 or trade_id == last_id:
                continue
            last_id = trade_id
            old = levels.get(side)
            if (old and price == old['price'] and previous_quote < received <= previous_quote + 2 * SECOND and
                    previous_exchange < exchange and exchange >= old['last']):
                old['traded'] += size
                old['prints'] += exchange != old['last']
                old['last'] = exchange
            if size * price < 150:
                continue
            history = clips[side, size]
            if history and exchange <= history[-1][1]:
                continue
            history.append((received, exchange))
            if len(history) == 4:
                clocks = list(zip(*history))
                spacings = [[later - earlier for earlier, later in zip(clock, clock[1:])] for clock in clocks]
                if all(min(gaps) >= SECOND and max(gaps) <= 60 * SECOND and max(gaps) <= 1.2 * min(gaps) for gaps in spacings):
                    hold = math.ceil(sum(spacings[0]) / SECOND)
                    signalled[side + '_schedule'].append((sec, hold, price, size, 4))
                    history.clear()
    return signalled


def replay(quotes, observations, fee, *, stop_bids=None, profit_protection=None):
    signals = {}
    for observation in observations:
        signals.setdefault(observation[0], observation)
    cash = peak = 100.
    units = misses = unavailable = delayed = attempts = 0
    dd = debit = 0.
    ledger = []
    pending = None
    stop_bid = None
    for sec, quote in enumerate(quotes):
        if pending:
            candidate, quantity = pending
            pending = None
            if quote and quote[4] >= quantity and 20 <= quantity * quote[2] <= 25 and quantity * quote[2] * (1 + fee) <= cash:
                entered, signal, units = sec, candidate, quantity
                debit = units * quote[2] * (1 + fee)
                cash -= debit
                deadline = sec + signal[1]
                entry_ask = quote[2]
                protection_armed = False
                if stop_bids is not None:
                    stop_bid = stop_bids.get(signal[0])
            else:
                misses += 1
        if units and quote:
            equity = cash + units * quote[1] * (1 - fee)
            peak = max(peak, equity)
            dd = max(dd, 1 - equity / peak)
            if dd >= .08:
                deadline = min(deadline, sec + 1)
            if stop_bid is not None and quote[3] >= units and quote[1] <= stop_bid:
                deadline = min(deadline, sec + 1)
            if profit_protection is not None and quote[3] >= units:
                stress_fee, arm_return, floor_return = profit_protection
                net_return = quote[1] * (1 - stress_fee) / (entry_ask * (1 + stress_fee)) - 1
                if protection_armed and net_return <= floor_return:
                    deadline = min(deadline, sec + 1)
                elif net_return >= arm_return:
                    protection_armed = True
        elif units:
            unavailable += 1
        if units and sec >= deadline:
            if quote and quote[3] >= units:
                proceeds = units * quote[1] * (1 - fee)
                cash += proceeds
                ledger.append((*signal, entered, sec, units, debit, proceeds, proceeds - debit))
                units = 0
            else:
                delayed += 1
        if not units and dd < .08 and sec in signals and sec + signals[sec][1] + 62 < len(quotes):
            attempts += 1
            if quote:
                pending = signals[sec], math.floor(24.5 / quote[2])
            else:
                misses += 1
    if units or pending or not math.isclose(cash - 100, math.fsum(row[-1] for row in ledger), abs_tol=1e-8):
        raise ValueError('Unclosed position or cash mismatch')
    return (len(observations), len(signals), attempts, misses, len(ledger), sum(row[-1] > 0 for row in ledger),
            cash - 100, dd * 100, unavailable, delayed), ledger


def main():
    with (ROOT / 'participant_traces_results.csv').open('w', newline='') as totals, (ROOT / 'participant_traces_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(totals), csv.writer(trades)
        writer.writerow(('day', 'rule', 'fee', 'raw_signals', 'signal_seconds', 'attempts', 'misses', 'roundtrips', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('day', 'rule', 'fee', 'signal_second', 'planned_hold', 'trace_price', 'trace_size', 'print_times', 'entry_second', 'exit_second', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        for day in DAYS:
            origin = (date.fromisoformat(day) - date(1970, 1, 1)).days * 86400 * SECOND
            signals = discover(heapq.merge(stream(day, 'quotes'), stream(day, 'trades'), key=lambda event: event[:2]), origin)
            quotes = book(day, 'DOGEUSDT')
            for rule in RULES:
                for fee in (.001, .002):
                    result, ledger = replay(quotes, signals[rule], fee)
                    writer.writerow((day, rule, fee, *result))
                    journal.writerows((day, rule, fee, *row) for row in ledger)
            print(day, {rule: len(signals[rule]) for rule in RULES}, flush=True)


if __name__ == '__main__':
    main()
