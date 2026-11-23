"""DOGE stablecoin-settled perpetual basis, with separate cash collateral and FX fees."""
import csv
import math
from concurrent.futures import ThreadPoolExecutor
from datetime import date, timedelta
from pathlib import Path

from funding_budget import funding_month
from independent_round import book


ROOT = Path(__file__).parent
SYMBOLS = ('DOGEUSDT', 'DOGEUSDC')
DAYS = tuple(f'2025-{month:02}-01' for month in range(2, 8))
MONTHS = tuple(f'{year}-{month:02}' for year, months in ((2024, range(7, 13)), (2025, range(1, 7))) for month in months)
HOLD = 900
DAY = 86400
FEES = (.001, .002)
MIN_EDGE = .0005


def epoch(day):
    return (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY


def funding_windows():
    with ThreadPoolExecutor(6) as pool:
        work = {(symbol, month): pool.submit(funding_month, symbol, month) for symbol in SYMBOLS for month in MONTHS}
        tables = {key: job.result() for key, job in work.items()}
    start, end = epoch('2024-07-01'), epoch('2025-07-01')
    calendar = set(range(start, end, 28800))
    records = {}
    for symbol in SYMBOLS:
        pairs = [(stamp // 1000, rate) for month in MONTHS for stamp, (rate, hours) in tables[symbol, month].items() if hours == 8]
        series = dict(pairs)
        if len(pairs) != len(series) or set(series) != calendar:
            raise ValueError(f'Funding calendar is incomplete or changes frequency for {symbol}')
        records[symbol] = series
    with (ROOT / 'collateral_carry_bound.csv').open('w', newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('window_start_utc', 'settlements', 'oracle_positive_matched_funding_per_25_usdt', 'base_min_entry_fees_100_pairs', 'stress_min_entry_fees_100_pairs'))
        for offset in range(365 - 30 + 1):
            origin = start + offset * DAY
            times = range(origin, origin + 30 * DAY, 28800)
            positive = math.fsum(abs(records[SYMBOLS[0]][stamp] - records[SYMBOLS[1]][stamp]) * 25 for stamp in times)
            writer.writerow((date(2024, 7, 1) + timedelta(days=offset), len(times), positive, 100 * 40 * FEES[0], 100 * 40 * FEES[1]))


def settlements(day):
    result = set()
    for symbol in SYMBOLS:
        rates = funding_month(symbol, day[:7])
        times = {stamp // 1000 - epoch(day) for stamp, (_, interval) in rates.items()
                 if epoch(day) <= stamp // 1000 < epoch(day) + DAY and interval == 8}
        if times != {0, 28800, 57600}:
            raise ValueError(f'Unexpected funding interval for {day} {symbol}')
        result.update(times)
    return result


def price_depth(quote, side):
    return (quote[2], quote[4]) if side > 0 else (quote[1], quote[3])


def entry_signal(first, second, fx, fee, mode):
    first_bid, first_ask = first[1:3]
    second_bid, second_ask = second[1:3]
    fx_bid, fx_ask = fx[1:3]
    mid_fx = (fx_bid + fx_ask) / 2
    reference = (first_bid + first_ask + (second_bid + second_ask) * mid_fx) / 4
    exits = (first_ask - first_bid + (second_ask * fx_ask - second_bid * fx_bid)) / 2
    required = 4 * fee + exits / reference + MIN_EDGE
    if mode == 'same_collateral':
        fx_bid = fx_ask = 1.
    gap_long_usdc = (first_bid - second_ask * fx_ask) / reference
    gap_short_usdc = (second_bid * fx_bid - first_ask) / reference
    if max(gap_long_usdc, gap_short_usdc) < required:
        return None
    return (1, gap_long_usdc) if gap_long_usdc > gap_short_usdc else (-1, gap_short_usdc)


def trial(usdt, usdc, fx, funding, fee, mode):
    wallets = [50., 0.]
    units = [0, 0]
    prices = [0., 0.]
    state = 'fund'
    peak, dd = 100., 0.
    counts = dict(eligible=0, attempts=0, entry_misses=0, delayed=0, unpriced=0)
    notes = []
    usd_start = None
    result = None
    for sec, (stable, tether, conversion) in enumerate(zip(usdc, usdt, fx)):
        quotes = (stable, tether)
        if sec in funding and any(units):
            raise ValueError('Unpriced funding crossed on open contracts')
        if state == 'fund' and conversion and conversion[4] * conversion[2] >= 50.:
            wallets = [50 / (conversion[2] * (1 + fee)), 50.]
            usd_start = wallets[0]
            state = 'flat'
        elif state in ('open_usdc', 'open_usdt'):
            leg = int(state == 'open_usdt')
            qty = order if leg == 0 else -order
            quote = quotes[leg]
            currency = ((conversion[1] + conversion[2]) / 2 if conversion else None) if leg == 0 else 1.
            if quote and currency:
                executed, liquidity = price_depth(quote, qty)
                value = abs(qty) * executed
                if liquidity >= abs(qty) and 20 <= value * currency <= 25 and value * (1 + fee) <= wallets[leg]:
                    units[leg], prices[leg] = qty, executed
                    wallets[leg] -= value * fee
                    state = 'hold' if leg == 1 else 'open_usdt'
                    if leg == 1:
                        end = sec + HOLD
                        paired = True
                else:
                    state = 'repair' if leg else 'flat'
                    counts['entry_misses'] += 1
            else:
                state = 'repair' if leg else 'flat'
                counts['entry_misses'] += 1
        elif state in ('close_usdc', 'close_usdt', 'repair'):
            leg = int(state == 'close_usdt')
            qty, quote = units[leg], quotes[leg]
            if quote:
                price, liquidity = price_depth(quote, -qty)
            if quote and liquidity >= abs(qty):
                wallets[leg] += qty * (price - prices[leg]) - abs(qty) * price * fee
                units[leg] = 0
                if state == 'close_usdc':
                    state = 'close_usdt'
                else:
                    notes.append((signal_sec, sec, order, edge, paired, wallets[0] - starting[0], wallets[1] - starting[1]))
                    state = 'flat'
            else:
                counts['delayed'] += 1
        if state == 'fund':
            continue
        if conversion and (not units[0] or stable) and (not units[1] or tether):
            marks = wallets.copy()
            for leg, qty in enumerate(units):
                if qty:
                    price, _ = price_depth(quotes[leg], -qty)
                    marks[leg] += qty * (price - prices[leg]) - abs(qty) * price * fee
                    if marks[leg] <= abs(qty) * price / 2:
                        raise ValueError('Inadequate collateral headroom')
            equity = marks[0] * conversion[1] * (1 - fee) + marks[1]
            peak = max(peak, equity)
            dd = max(dd, 1 - equity / peak)
        elif any(units):
            counts['unpriced'] += 1
        if state == 'hold' and (sec + 1 >= end or dd >= .08):
            state = 'close_usdc'
        if state == 'open_usdt' and dd >= .08:
            counts['entry_misses'] += 1
            state = 'repair'
        if state == 'flat' and sec >= len(usdt) - 120 and conversion and conversion[3] >= max(usd_start, wallets[0]):
            our_exit = conversion[1] * (1 - fee)
            baseline = usd_start * our_exit + 50.
            net = wallets[0] * our_exit + wallets[1]
            gain = net - baseline
            calibrated = [(*row, row[-2] * our_exit + row[-1]) for row in notes]
            if not math.isclose(gain, math.fsum(row[-1] for row in calibrated), abs_tol=1e-8):
                raise ValueError('Wallet and inventory baseline do not reconcile')
            result = net - 100., gain, calibrated
            break
        if (state == 'flat' and dd < .08 and sec % 5 == 0 and
                sec + HOLD + 120 < len(usdt) and all((stable, tether, conversion)) and
                not any(sec < moment <= sec + HOLD + 120 for moment in funding)):
            counts['eligible'] += 1
            signal = entry_signal(tether, stable, conversion, fee, mode)
            if signal:
                side, edge = signal
                size_model = max(price_depth(stable, side)[0] * conversion[2], price_depth(tether, -side)[0])
                order = side * math.floor(24.5 / size_model)
                signal_sec, paired = sec, False
                starting = wallets.copy()
                state = 'open_usdc'
                counts['attempts'] += 1
    if result is None:
        raise ValueError('Unable to close account in time')
    net, gain, ledger = result
    rounds = [row for row in ledger if row[4]]
    return (*counts.values(), len(rounds), sum(row[-1] > 0 for row in rounds), len(ledger) - len(rounds), net, gain, dd * 100), ledger


def main():
    funding_windows()
    with (ROOT / 'collateral_basis_results.csv').open('w', newline='') as results, (ROOT / 'collateral_basis_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('day', 'mode', 'fee', 'eligible', 'attempts', 'misses', 'delayed', 'unpriced', 'pairs', 'wins', 'repairs', 'net_usdt', 'incremental_usdt', 'dd_pct'))
        journal.writerow(('day', 'mode', 'fee', 'signal_sec', 'close_sec', 'usdc_qty_signed', 'quoted_edge', 'paired', 'usdc_change', 'usdt_change', 'incremental_usdt'))
        for day in DAYS:
            funding = settlements(day)
            tether, stable = (book(day, symbol, 'binance-futures') for symbol in SYMBOLS)
            conversion = book(day, 'USDCUSDT')
            for mode in ('adjusted', 'same_collateral'):
                for fee in FEES:
                    result, ledger = trial(tether, stable, conversion, funding, fee, mode)
                    writer.writerow((day, mode, fee, *result))
                    journal.writerows((day, mode, fee, *row) for row in ledger)
            print(day, 'complete', flush=True)


if __name__ == '__main__':
    main()
