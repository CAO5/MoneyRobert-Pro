"""Equal-dollar, fully cash-backed DOGE vs 1000SHIB linear perpetual screen."""
import csv
import io
import math
from collections import deque
from datetime import date
from pathlib import Path
from statistics import median

from independent_round import book
from position_divergence import archive


ROOT = Path(__file__).parent
SYMBOLS = ('DOGEUSDT', '1000SHIBUSDT')
DAYS = tuple(f'2021-{month:02}-01' for month in range(8, 12))
HOLD = 1800


def schedule(day):
    origin = (date.fromisoformat(day) - date(1970, 1, 1)).days * 86400
    result = []
    for symbol in SYMBOLS:
        month = day[:7]
        raw = archive(f'futures/um/monthly/fundingRate/{symbol}/{symbol}-fundingRate-{month}.zip')
        stamps = [int(row['calc_time']) // 1000 - origin for row in csv.DictReader(io.StringIO(raw))]
        times = [stamp for stamp in stamps if 0 <= stamp < 86400]
        if times != [0, 28800, 57600]:
            raise ValueError(f'Unexpected funding schedule {symbol}: {times}')
        result.append(set(times))
    return result


def shocks(doge, shib):
    history = deque(maxlen=30)
    result = {}
    eligible = 0
    for sec in range(59, len(doge), 60):
        pair = doge[sec], shib[sec]
        level = math.log(sum(pair[0][1:3]) / sum(pair[1][1:3])) if all(pair) else None
        known = [value for value in history if value is not None]
        if len(history) == 30 and len(known) >= 24 and level is not None and sec % 28800 < 28800 - HOLD - 120:
            eligible += 1
            distance = level - median(known)
            if abs(distance) >= math.log(1.02):
                result[sec] = distance
        history.append(level)
    return eligible, result


def price(quote, side):
    return quote[2 if side > 0 else 1]


def depth(quote, side):
    return quote[4 if side > 0 else 3]


def replay(doge, shib, events, settlements, mode, fee):
    balances = [50., 50.]
    units = [0, 0]
    entries = [0., 0.]
    peak = 100.
    dd = 0.
    missed = unknown = attempts = delayed = 0
    state = 'flat'
    ledger = []
    for sec, quotes in enumerate(zip(doge, shib)):
        if any(units[leg] and sec in settlements[leg] for leg in (0, 1)):
            raise ValueError('Open exposure at settlement; cannot exclude funding')
        if state == 'hold' and sec >= end:
            state = 'sell_first'
        if state in ('open_first', 'open_second'):
            leg = int(state == 'open_second')
            qty = wanted[leg]
            quote = quotes[leg]
            if quote:
                fill = price(quote, qty)
                notional = abs(qty) * fill
            if quote and depth(quote, qty) >= abs(qty) and 20 <= notional <= 25 and notional * (1 + fee) <= balances[leg]:
                units[leg], entries[leg] = qty, fill
                balances[leg] -= notional * fee
                if leg == 0:
                    state = 'open_second'
                else:
                    state = 'hold'
                    end = sec + HOLD
                    paired = True
            else:
                state = 'orphan' if leg else 'flat'
                missed += 1
        elif state in ('sell_first', 'sell_second', 'orphan'):
            leg = int(state == 'sell_second')
            qty = units[leg]
            quote = quotes[leg]
            if quote and depth(quote, -qty) >= abs(qty):
                fill = price(quote, -qty)
                balances[leg] += qty * (fill - entries[leg]) - abs(qty) * fill * fee
                units[leg] = 0
                if state == 'sell_first':
                    state = 'sell_second'
                else:
                    ledger.append((signal, sec, distance, *wanted, paired, balances[0] - before[0], balances[1] - before[1], sum(balances) - sum(before)))
                    state = 'flat'
            else:
                delayed += 1
        if all(not qty or quotes[leg] for leg, qty in enumerate(units)):
            equity = sum(balances)
            for leg, qty in enumerate(units):
                if qty:
                    exit_price = price(quotes[leg], -qty)
                    liquidation = qty * (exit_price - entries[leg]) - abs(qty) * exit_price * fee
                    if balances[leg] + liquidation <= .2 * abs(qty) * exit_price:
                        raise ValueError('Margin headroom too thin for this screen')
                    equity += liquidation
            peak = max(peak, equity)
            dd = max(dd, 1 - equity / peak)
            if dd >= .08 and state == 'hold':
                end = min(end, sec + 1)
            if dd >= .08 and state == 'open_second':
                state = 'orphan'
                missed += 1
        else:
            unknown += 1
        if sec in events and state == 'flat' and dd < .08 and sec + HOLD + 120 < len(doge):
            if not all(quotes):
                raise ValueError('Invalid signal without both quotes')
            attempts += 1
            signal, distance, paired = sec, events[sec], False
            direction = 1 if (distance < 0) == (mode == 'convergence') else -1
            wanted = tuple(side * math.floor(24.5 / price(quote, side)) for side, quote in zip((direction, -direction), quotes))
            before = balances.copy()
            state = 'open_first'
    net = sum(balances) - 100
    if state != 'flat' or any(units) or not math.isclose(net, math.fsum(row[-1] for row in ledger), abs_tol=1e-8):
        raise ValueError('Unresolved exposure or account mismatch')
    completed = [row for row in ledger if row[5]]
    return (attempts, missed, len(ledger) - len(completed), len(completed), sum(row[-1] > 0 for row in completed), net, dd * 100, unknown, delayed), ledger


def main():
    with (ROOT / 'paired_attention_results.csv').open('w', newline='') as results, (ROOT / 'paired_attention_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('day', 'mode', 'fee', 'eligible_minutes', 'signals', 'attempts', 'misses', 'orphan_roundtrips', 'paired_roundtrips', 'paired_wins', 'net_usdt_including_orphans', 'dd_pct', 'unpriced_seconds', 'delayed_legs'))
        journal.writerow(('day', 'mode', 'fee', 'signal_second', 'exit_second', 'log_ratio_gap', 'doge_signed_units', 'shib_signed_units_requested', 'paired', 'doge_net', 'shib_net', 'net_usdt'))
        for day in DAYS:
            settlements = schedule(day)
            doge, shib = (book(day, symbol, 'binance-futures') for symbol in SYMBOLS)
            eligible, events = shocks(doge, shib)
            for mode in ('convergence', 'extension'):
                for fee in (.001, .002):
                    result, ledger = replay(doge, shib, events, settlements, mode, fee)
                    writer.writerow((day, mode, fee, eligible, len(events), *result))
                    journal.writerows((day, mode, fee, *row) for row in ledger)
            print(day, eligible, len(events), flush=True)


if __name__ == '__main__':
    main()
