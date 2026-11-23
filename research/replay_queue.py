"""Replay four pre-registered DOGE hypotheses using public quotes only."""
import argparse
import csv
import math
import time
from concurrent.futures import ProcessPoolExecutor
from functools import partial
from pathlib import Path

from hf_pressure import CACHE
from independent_round import book
from participant_traces import replay


DAYS = tuple(f'2025-{month:02}-01' for month in range(2, 8))
FEES = (.001, .002)
DAY = 86400
BUDGET = 25.
TRIANGLE_SYMBOLS = {
    'binance': ('DOGEUSDT', 'DOGEUSDC', 'USDCUSDT'),
    'okex': ('DOGE-USDT', 'DOGE-USDC', 'USDC-USDT'),
}
PATHS = (
    ('usdt_doge_usdc_usdt', ((0, True), (1, False), (2, False))),
    ('usdt_usdc_doge_usdt', ((2, True), (1, True), (0, False))),
)
ROOT = Path(__file__).parent
RESULT_COLUMNS = ('day', 'direction', 'venue', 'rule', 'fee', 'status', 'eligible_seconds',
                  'precondition_seconds', 'fractional_edges', 'signals', 'attempts', 'misses',
                  'trades', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delays_or_unwinds',
                  'unresolved', 'detail')
TRADE_COLUMNS = ('day', 'direction', 'venue', 'rule', 'fee', 'signal_second', 'entry_second',
                 'exit_second', 'outcome', 'debit_usdt', 'credit_usdt', 'net_usdt')


def mid(quote):
    return (quote[1] + quote[2]) / 2


def locate(home, doge_usdc, usdc_usdt, bybit, okx):
    signals = {direction: [] for direction in ('Q2', 'Q3', 'Q4')}
    eligible = dict.fromkeys(signals, 0)
    preconditions = dict.fromkeys(signals, 0)
    ask_since = 0
    previous_ask = None
    last_following = -2
    for sec, quote in enumerate(home[:-93]):
        ask = quote[2] if quote else None
        if ask != previous_ask:
            ask_since = sec
        previous_ask = ask
        if sec < 15 or not quote:
            continue
        local_mid = mid(quote)
        record = (sec, 30, local_mid, 0., 0.)
        if doge_usdc and usdc_usdt and all(
                (home[sec-10], doge_usdc[sec], doge_usdc[sec-10], usdc_usdt[sec], usdc_usdt[sec-10])):
            eligible['Q2'] += 1
            implied_bid = doge_usdc[sec][1] * usdc_usdt[sec][1]
            old_bid = doge_usdc[sec-10][1] * usdc_usdt[sec-10][1]
            if implied_bid / old_bid >= 1.0025:
                preconditions['Q2'] += 1
                if abs(local_mid / mid(home[sec-10]) - 1) <= .0005 and implied_bid / ask >= 1.0008:
                    signals['Q2'].append(record)
        if home[sec-5]:
            eligible['Q3'] += 1
            if sec - ask_since >= 15:
                preconditions['Q3'] += 1
                if quote[1] / home[sec-5][1] >= 1.0005 and ask / quote[1] <= 1.0008:
                    if sec > last_following + 1:
                        signals['Q3'].append(record)
                    last_following = sec
        if bybit and okx and all((home[sec-5], bybit[sec], bybit[sec-5], okx[sec], okx[sec-5])):
            eligible['Q4'] += 1
            old_local = mid(home[sec-5])
            if local_mid / old_local <= .9975:
                preconditions['Q4'] += 1
                if all(abs(mid(away[sec]) / mid(away[sec-5]) - 1) <= .0008 and
                       abs(mid(away[sec-5]) / old_local - 1) <= .001 and away[sec][1] / ask >= 1.0015
                       for away in (bybit, okx)):
                    signals['Q4'].append(record)
    return eligible, preconditions, signals


def convert(amount, quote, buy, fee):
    if quote is None:
        return None
    _, bid, ask, bid_size, ask_size = quote
    quantity = math.floor(amount / ask if buy else amount)
    price, depth = (ask, ask_size) if buy else (bid, bid_size)
    if quantity > depth or quantity * price < 5:
        return None
    return (quantity if buy else quantity * bid) * (1 - fee)


def triangular(books, path, fee):
    cash = peak = 100.
    dd = 0.
    eligible = preconditions = fractional_edges = signals = attempts = misses = unpriced = unwinds = unresolved = 0
    fee_factor = (1 - fee) ** len(path)
    next_signal = 0
    journal = []
    for signal in range(DAY - 5):
        quotes = tuple(market[signal] for market in books)
        if not all(quotes):
            continue
        eligible += 1
        gross = math.prod(1 / quotes[index][2] if buy else quotes[index][1] for index, buy in path)
        if gross < 1.0002:
            continue
        preconditions += 1
        if gross * fee_factor < 1.0002:
            continue
        fractional_edges += 1
        projected = BUDGET
        for index, buy in path:
            projected = convert(projected, quotes[index], buy, fee)
            if projected is None:
                break
        if projected is None or projected < BUDGET * 1.0002:
            continue
        signals += 1
        if signal < next_signal or dd >= .08 or cash < BUDGET:
            continue

        attempts += 1
        amount = BUDGET
        recovery_market = None
        recovering = False
        proceeds = None
        for sec in range(signal + 1, DAY):
            if recovery_market is not None:
                mark = convert(amount, books[recovery_market][sec], False, fee)
                if mark is None:
                    unpriced += 1
                else:
                    equity = cash - BUDGET + mark
                    peak = max(peak, equity)
                    dd = max(dd, 1 - equity / peak)
                if recovering:
                    if mark is None:
                        continue
                    proceeds = mark
                    unwinds += 1
                    break
                if dd >= .08:
                    recovering = True
                    continue

            leg = sec - signal - 1
            index, buy = path[leg]
            updated = convert(amount, books[index][sec], buy, fee)
            if updated is None:
                if leg == 0:
                    misses += 1
                    break
                recovering = True
                continue
            amount = updated
            if leg == 2:
                proceeds = amount
                break
            recovery_market = 0 if (index, buy) in ((0, True), (1, True)) else 2
        next_signal = sec + 1
        if proceeds is None:
            if recovery_market is None:
                continue
            proceeds = 0.
            unresolved += 1
        outcome = 'unresolved' if proceeds == 0 else ('unwind' if recovering else 'cycle')
        cash += proceeds - BUDGET
        peak = max(peak, cash)
        dd = max(dd, 1 - cash / peak)
        journal.append((signal, signal+1, sec, outcome, BUDGET, proceeds, proceeds - BUDGET))

    if not math.isclose(cash - 100, math.fsum(row[-1] for row in journal), abs_tol=1e-8):
        raise ValueError('Triangular cash reconciliation failed')
    metrics = (eligible, preconditions, fractional_edges, signals, attempts, misses, len(journal) - unresolved,
               sum(row[-1] > 0 for row in journal), cash - 100, dd * 100, unpriced, unwinds, unresolved)
    return metrics, journal


def read_market(day, symbol, venue, cached_only, failures):
    prefix = '' if venue == 'binance' else f'{venue}-'
    try:
        if cached_only and not (CACHE / f'{prefix}{day}-{symbol}-quotes.csv.gz').is_file():
            raise FileNotFoundError('Quote file absent from cache')
        return book(day, symbol, venue)
    except (OSError, EOFError, ValueError) as error:
        failures[venue, symbol] = f'{venue}/{symbol} {error}'
        return None


def run_day(day, cached_only):
    failures, books, results, trades = {}, {}, [], []

    def read(venue, symbol):
        key = venue, symbol
        if key not in books:
            books[key] = read_market(day, symbol, venue, cached_only, failures)
        return books[key]

    def save(direction, venue, rule, fee, markets, simulate):
        key = day, direction, venue, rule, fee
        unavailable = [failures[market] for market in markets if market in failures]
        status = 'missing_quotes' if unavailable else 'replayed'
        metrics = ('',) * (len(RESULT_COLUMNS) - 7)
        if not unavailable:
            try:
                metrics, journal = simulate()
                trades.extend((*key, *row) for row in journal)
            except ValueError as error:
                status, unavailable = 'unresolved_replay', [str(error)]
        results.append((*key, status, *metrics, ' | '.join(unavailable)))

    home = read('binance', 'DOGEUSDT')
    doge_usdc = read('binance', 'DOGEUSDC')
    usdc_usdt = read('binance', 'USDCUSDT')
    bybit = read('bybit-spot', 'DOGEUSDT')
    okx = read('okex', 'DOGE-USDT')
    eligible, preconditions, signals = locate(home or [None] * DAY, doge_usdc, usdc_usdt, bybit, okx)

    dependencies = {
        'Q2': (('binance', 'DOGEUSDT'), ('binance', 'DOGEUSDC'), ('binance', 'USDCUSDT')),
        'Q3': (('binance', 'DOGEUSDT'),),
        'Q4': (('binance', 'DOGEUSDT'), ('bybit-spot', 'DOGEUSDT'), ('okex', 'DOGE-USDT')),
    }
    for direction, markets in dependencies.items():
        for fee in FEES:
            def simulate():
                score, journal = replay(home, signals[direction], fee)
                _, count, attempts, misses, complete, wins, net, dd, gaps, delays = score
                return (eligible[direction], preconditions[direction], '', count, attempts, misses,
                        complete, wins, net, dd, gaps, delays, 0), [
                    (row[0], row[5], row[6], 'closed', *row[-3:]) for row in journal]

            save(direction, 'binance', 'long', fee, markets, simulate)
    for venue, symbols in TRIANGLE_SYMBOLS.items():
        triangle = tuple(read(venue, symbol) for symbol in symbols)
        markets = tuple((venue, symbol) for symbol in symbols)
        for rule, path in PATHS:
            for fee in FEES:
                save('Q1', venue, rule, fee, markets, partial(triangular, triangle, path, fee))
    return results, trades


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cached-only', action='store_true')
    parser.add_argument('--workers', type=int, default=6, choices=range(1, 7))
    args = parser.parse_args()
    CACHE.mkdir(exist_ok=True)
    started = time.perf_counter()
    with ProcessPoolExecutor(args.workers) as pool:
        workload = partial(run_day, cached_only=args.cached_only)
        blocks = list(pool.map(workload, DAYS))
    names = ('replay_queue_results.csv', 'replay_queue_trades.csv')
    for name, header, records in zip(names, (RESULT_COLUMNS, TRADE_COLUMNS), zip(*blocks)):
        with (ROOT / name).open('w', newline='', encoding='utf-8') as output:
            writer = csv.writer(output)
            writer.writerow(header)
            for day in records:
                writer.writerows(day)
    print(f'{len(DAYS)} dates, {args.workers} workers, {time.perf_counter() - started:.2f} seconds', flush=True)


if __name__ == '__main__':
    main()
