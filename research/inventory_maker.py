"""Fixed vs inventory-skewed two-sided DOGE quoting with a matched holding baseline."""
import csv
import math
from collections import deque
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass
from pathlib import Path

from independent_round import book
from spread_capture import tape


ROOT = Path(__file__).parent
DAYS = ('2023-03-01', '2023-12-01', '2024-03-01', '2024-08-01', '2024-12-01',
        '2025-02-01', '2025-07-01', '2025-11-01', '2026-03-01', '2026-11-01')
TICK = .00001
HALF_BAND = .0035
SKEW = .002
REFRESH = 30
LIVE_SECONDS = 12
LATE_SECONDS = 2
END_BUFFER = 120


@dataclass
class Order:
    side: int
    price: float
    remaining: int
    start: int
    expiry: int
    evidence: float = 0.

    def fill(self, second, exchange, aggressor, price, amount):
        if (not self.start <= exchange <= self.expiry or second < exchange
                or second - exchange > LATE_SECONDS or amount <= 0 or not self.remaining):
            return 0
        passes = ((self.side == 1 and aggressor == 'sell' and price < self.price)
                  or (self.side == -1 and aggressor == 'buy' and price > self.price))
        if not passes:
            return 0
        self.evidence += amount
        size = min(self.remaining, math.floor(self.evidence))
        self.remaining -= size
        self.evidence -= size
        return size


def limits(quote, deviation, seed, skew):
    center = (quote[1] + quote[2]) / 2 * (1 - skew * deviation / seed)
    bid = math.floor(center * (1 - HALF_BAND) / TICK) * TICK
    ask = math.ceil(center * (1 + HALF_BAND) / TICK) * TICK
    return bid, ask


class Wallet:
    def __init__(self, seed, cost):
        self.seed = seed
        self.quantity = seed
        self.cash = 100. - cost
        self.seed_cost = cost
        self.cycle = 0.
        self.start = None
        self.matched = []

    def exchange(self, sec, side, size, price, fee):
        proceeds_per_unit = -side * price * (1 + side * fee)
        distance = self.quantity - self.seed
        if distance * side < 0 and size >= abs(distance):
            closed = abs(distance)
            self.cycle += closed * proceeds_per_unit
            self.matched.append((self.start, sec, self.cycle))
            self.cycle = 0.
            self.start = None
            excess = size - closed
        else:
            excess = size
        if excess:
            if self.start is None:
                self.start = sec
            self.cycle += excess * proceeds_per_unit
        self.cash += size * proceeds_per_unit
        self.quantity += side * size
        if self.cash < -1e-8 or not 0 <= self.quantity <= 2 * self.seed:
            raise ValueError('Unfunded position')


def replay(quotes, trades, skew, fee):
    wallet = None
    orders = []
    pending = None
    peak = 100.
    max_dd = 0.
    unpriced = posted = maker_fills = 0
    final = None
    for sec, quote in enumerate(quotes):
        if wallet is None:
            if quote and sec < min(300, len(quotes) - END_BUFFER):
                size = math.floor(24.5 / quote[2])
                if quote[4] >= size and 20 <= size * quote[2] <= 25:
                    wallet = Wallet(size, size * quote[2] * (1 + fee))
            if wallet is None:
                continue
        if pending:
            for side, price, size in pending:
                if not quote or (side == 1 and price >= quote[2]) or (side == -1 and price <= quote[1]):
                    continue
                orders.append(Order(side, price, size, sec + 1, sec + LIVE_SECONDS))
                posted += 1
            pending = None
        orders = [order for order in orders if order.remaining and sec <= order.expiry + LATE_SECONDS]
        if orders:
            for exchange, aggressor, price, size in trades.get(sec, ()):
                for order in orders:
                    filled = order.fill(sec, exchange, aggressor, price, size)
                    if filled:
                        wallet.exchange(sec, order.side, filled, order.price, fee)
                        maker_fills += 1
        if wallet.quantity and (not quote or quote[3] < wallet.quantity):
            unpriced += 1
        else:
            equity = wallet.cash + wallet.quantity * quote[1] * (1 - fee) if wallet.quantity else wallet.cash
            peak = max(peak, equity)
            max_dd = max(max_dd, 1 - equity / peak)
        if sec >= len(quotes) - END_BUFFER and not orders and quote and quote[3] >= max(wallet.quantity, wallet.seed):
            final = quote[1] * (1 - fee)
            break
        if sec % REFRESH or max_dd >= .08 or not quote or orders or sec >= len(quotes) - END_BUFFER - REFRESH:
            continue
        bid, ask = limits(quote, wallet.quantity - wallet.seed, wallet.seed, skew)
        if bid <= 0:
            continue
        buy_size = min(2 * wallet.seed - wallet.quantity, math.floor(24.5 / bid),
                       math.floor(wallet.cash / (bid * (1 + fee))))
        sell_size = min(wallet.quantity, math.floor(24.5 / ask))
        pending = [(side, price, size) for side, price, size in ((1, bid, buy_size), (-1, ask, sell_size))
                   if 20 <= price * size <= 25]
    if wallet is None or final is None:
        raise ValueError('Could not fund or close inventory at available quotes')
    net = wallet.cash + wallet.quantity * final - 100
    baseline = wallet.seed * final - wallet.seed_cost
    unmatched = wallet.cycle + (wallet.quantity - wallet.seed) * final
    cycled = math.fsum(note[2] for note in wallet.matched)
    if not math.isclose(net - baseline, cycled + unmatched, abs_tol=1e-8):
        raise ValueError('Trading cycle and remainder did not reconcile')
    return (posted, maker_fills, len(wallet.matched), sum(note[2] > 0 for note in wallet.matched),
            net, baseline, net - baseline, unmatched, 100 * max_dd, unpriced), wallet.matched


def screen(day):
    quotes = book(day, 'DOGEUSDT')
    trades = tape(day)
    results, ledger = [], []
    for name, skew in (('symmetric', 0.), ('inventory_skew', SKEW)):
        for fee in (.001, .002):
            summary, journal = replay(quotes, trades, skew, fee)
            results.append((day, name, fee, *summary))
            ledger.extend((day, name, fee, *trade) for trade in journal)
    return results, ledger


def main():
    with ProcessPoolExecutor(6) as workers:
        outcomes = list(workers.map(screen, DAYS))
    with (ROOT / 'inventory_maker_results.csv').open('w', newline='') as results, (ROOT / 'inventory_maker_cycles.csv').open('w', newline='') as ledger:
        writer, journal = csv.writer(results), csv.writer(ledger)
        writer.writerow(('day', 'mode', 'fee', 'posted', 'maker_fill_updates', 'cycles', 'wins', 'net_usdt',
                         'hold_usdt', 'incremental_usdt', 'unmatched_usdt', 'dd_pct', 'unpriced_seconds'))
        journal.writerow(('day', 'mode', 'fee', 'first_fill_second', 'inventory_restored_second', 'cycle_net_usdt'))
        for summary, notes in outcomes:
            writer.writerows(summary)
            journal.writerows(notes)
            print(summary[0][0], len(notes), flush=True)


if __name__ == '__main__':
    main()
