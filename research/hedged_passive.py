"""Inventory-backed Gate sale after a Binance passive fill, with an immediate-entry control."""
import csv
import math
from collections import defaultdict
from datetime import date
from pathlib import Path

from hf_pressure import CACHE, rows
from independent_round import book


ROOT = Path(__file__).parent
DAYS = ('2024-10-01', '2024-11-01', '2025-10-01', '2025-11-01', '2025-12-01',
        '2026-05-01', '2026-06-01', '2026-07-01', '2026-08-01')
HEDGE_MIN = 10.
EDGE_BUFFER = .0001
ORDER_SECONDS = 20
MAX_RESTORE = 180


def tape(day):
    origin = (date.fromisoformat(day) - date(1970, 1, 1)).days * 86400
    events = defaultdict(list)
    seen = set()
    for row in rows(day, 'DOGEUSDT', 'trades'):
        if row[5] != 'sell' or row[4] in seen:
            continue
        seen.add(row[4])
        arrival = int(row[3]) // 1_000_000 - origin
        exchange = int(row[2]) // 1_000_000 - origin
        if 0 <= exchange <= arrival < 86400:
            events[arrival].append((exchange, float(row[6]), float(row[7])))
    return events


def shares(budget, price):
    return math.floor(budget / price)


def buy(quote, quantity, cash, fee):
    return quote and quote[4] >= quantity and HEDGE_MIN <= quantity * quote[2] <= cash / (1 + fee)


def sell(quote, quantity):
    return quote and quote[3] >= quantity and quantity * quote[1] >= HEDGE_MIN


def opportunity(home, away, quantity, passive, fee):
    entry = home[1] if passive else home[2]
    forward = away[1] * (1 - fee) - entry * (1 + fee)
    valid = 20 <= quantity * entry <= 25 and sell(away, quantity)
    enough = forward >= entry * EDGE_BUFFER
    return valid and enough, forward


def simulate(home, away, events, passive, fee):
    cash = [50., 50.]
    inventory = [0, 0]
    seed = [0., 0.]
    final = [None, None]
    ledger = []
    phase = 'fund'
    peak = 100.
    drawdown = 0.
    eligible = displayed_crosses = attempts = misses = unknown = 0
    peak_forward_bps = None
    fixed = 0
    for sec, pair in enumerate(zip(home, away)):
        local, remote = pair
        ending = sec >= len(home) - 120 or drawdown >= .08
        if phase == 'fund' and local and remote:
            fixed = shares(24.5, max(local[2], remote[2]))
            if all(buy(quote, fixed, cash[index], fee) for index, quote in enumerate(pair)):
                for index, quote in enumerate(pair):
                    seed[index] = fixed * quote[2] * (1 + fee)
                    cash[index] -= seed[index]
                    inventory[index] = fixed
                phase = 'flat'
        elif phase == 'post':
            if not local or limit >= local[2] or cash[0] < target * limit * (1 + fee):
                misses += 1
                phase = 'flat'
            else:
                working = sec + 1
                deadline = working + ORDER_SECONDS - 1
                phase = 'collect'
        elif phase == 'collect':
            observed += sum(amount for stamp, price, amount in events.get(sec, ())
                            if working <= stamp <= deadline and price < limit)
            new_quantity = min(target, math.floor(observed))
            delta = new_quantity - filled
            if delta:
                cash[0] -= delta * limit * (1 + fee)
                inventory[0] += delta
                filled = new_quantity
            if sec >= deadline + 1 or filled >= target:
                if filled:
                    phase = 'hedge'
                else:
                    misses += 1
                    phase = 'flat'
        elif phase == 'take':
            if buy(local, target, cash[0], fee) and 20 <= target * local[2] <= 25:
                cash[0] -= target * local[2] * (1 + fee)
                inventory[0] += target
                filled = target
                phase = 'hedge'
            else:
                misses += 1
                phase = 'flat'
        elif phase == 'hedge':
            if sell(remote, filled) and inventory[1] >= filled:
                cash[1] += filled * remote[1] * (1 - fee)
                inventory[1] -= filled
                deadline = sec + MAX_RESTORE
                phase = 'wait_restore'
            else:
                phase = 'repair'
        elif phase == 'wait_restore':
            available = buy(remote, filled, cash[1], fee) and sell(local, filled)
            estimate = sum(cash) - opening_cash + filled * (local[1] * (1 - fee) - remote[2] * (1 + fee)) if available else None
            if ending or sec >= deadline or (estimate is not None and estimate >= filled * local[1] * EDGE_BUFFER):
                phase = 'restore_buy'
        elif phase == 'restore_buy' and buy(remote, filled, cash[1], fee):
            cash[1] -= filled * remote[2] * (1 + fee)
            inventory[1] += filled
            phase = 'restore_sell'
        elif phase in ('restore_sell', 'repair') and sell(local, filled):
            completed = phase == 'restore_sell'
            cash[0] += filled * local[1] * (1 - fee)
            inventory[0] -= filled
            ledger.append((decision, sec, target, filled, completed, sum(cash) - opening_cash))
            phase = 'flat'
        elif phase in ('flat', 'finish') and ending:
            phase = 'finish'
            for index, quote in enumerate(pair):
                if inventory[index] and sell(quote, inventory[index]):
                    final[index] = quote[1]
                    cash[index] += inventory[index] * quote[1] * (1 - fee)
                    inventory[index] = 0
        if phase != 'fund' and all(not quantity or quote for quote, quantity in zip(pair, inventory)):
            equity = sum(cash) + sum(quantity * quote[1] * (1 - fee)
                                     for quote, quantity in zip(pair, inventory) if quantity)
            peak = max(peak, equity)
            drawdown = max(drawdown, 1 - equity / peak)
        elif any(inventory):
            unknown += 1
        if phase == 'finish' and not any(inventory):
            break
        if phase == 'flat' and sec % 5 == 0 and sec < len(home) - MAX_RESTORE - 150 and not ending and drawdown < .08 and local and remote:
            qty = min(fixed, shares(24.5, local[1 if passive else 2]))
            cross, forward = opportunity(local, remote, qty, passive, fee)
            if 20 <= qty * local[1 if passive else 2] <= 25 and inventory[1] >= qty:
                eligible += 1
                displayed_crosses += cross
                edge_bps = 10000 * forward / local[1 if passive else 2]
                peak_forward_bps = edge_bps if peak_forward_bps is None else max(peak_forward_bps, edge_bps)
                if cross:
                    attempts += 1
                    target, decision, opening_cash = qty, sec, sum(cash)
                    limit, filled, observed = local[1], 0, 0.
                    phase = 'post' if passive else 'take'
    resolved = phase == 'finish' and not any(inventory) and all(final)
    gross = sum(cash) - 100 if resolved else None
    hold = fixed * sum(final) * (1 - fee) - sum(seed) if resolved else None
    incremental = math.fsum(row[-1] for row in ledger)
    if resolved and not math.isclose(gross - hold, incremental, abs_tol=1e-8):
        raise ValueError('Inventory-matched ledger failed to reconcile')
    cycles = [row for row in ledger if row[-2]]
    return (eligible, displayed_crosses, peak_forward_bps, attempts, misses, len(cycles),
            sum(row[-1] > 0 for row in cycles), len(ledger) - len(cycles), incremental, gross, hold,
            drawdown * 100, unknown, phase, *inventory), ledger


def main():
    CACHE.mkdir(exist_ok=True)
    with (ROOT / 'hedged_passive_results.csv').open('w', newline='') as results, (ROOT / 'hedged_passive_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('day', 'entry', 'fee', 'eligible', 'displayed_crosses', 'peak_forward_bps', 'attempts', 'misses',
                         'cycles', 'wins', 'repairs', 'incremental_usdt', 'net_usdt', 'hold_usdt', 'dd_pct',
                         'unpriced_seconds', 'final_phase', 'home_end_doge', 'away_end_doge'))
        journal.writerow(('day', 'entry', 'fee', 'decision_second', 'closed_second', 'requested_qty', 'filled_qty', 'cycled', 'incremental_usdt'))
        for day in DAYS:
            home, away = book(day, 'DOGEUSDT'), book(day, 'DOGE_USDT', 'gate-io')
            sellers = tape(day)
            for passive in (True, False):
                for fee in (.001, .002):
                    summary, ledger = simulate(home, away, sellers, passive, fee)
                    key = (day, 'passive' if passive else 'take', fee)
                    writer.writerow((*key, *summary))
                    journal.writerows((*key, *row) for row in ledger)
                    print(*key, *summary[1:9], summary[-3], flush=True)


if __name__ == '__main__':
    main()
