"""Measure first retests of an accepted breakout and of a failed breakdown."""
import argparse
import csv
import math
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from independent_round import book


ROOT = Path(__file__).parent
BATCHES = {
    'early': ('2021-05-01', '2021-07-01', '2021-11-01', '2022-02-01', '2022-05-01', '2022-09-01',
              '2022-12-01', '2023-03-01', '2023-05-01', '2023-07-01', '2023-09-01', '2023-12-01'),
    'later': ('2024-03-01', '2024-06-01', '2024-08-01', '2024-10-01', '2024-12-01', '2025-02-01',
              '2025-04-01', '2025-06-01', '2025-08-01', '2025-12-01', '2026-03-01', '2026-09-01'),
}
RULES = ('breakout_retest', 'failed_breakdown')
STRESS_FEE = .002
HOLD = 600
BLOCK = 1800
REFERENCE = 900


def midpoint(quote):
    return (quote[1] + quote[2]) / 2


def acceptable(entry, stop, target):
    cost = entry * (1 + STRESS_FEE)
    worst = cost - stop * (1 - STRESS_FEE)
    reward = target * (1 - STRESS_FEE) - cost
    return stop < entry < target and reward >= 1.2 * worst


def opportunities(quotes):
    chosen = defaultdict(list)
    eligible = defaultdict(int)
    for start in range(0, len(quotes) - BLOCK + 1, BLOCK):
        reference = quotes[start:start + REFERENCE:5]
        known = [midpoint(quote) for quote in reference if quote]
        if len(known) < .95 * len(reference):
            continue
        low, high = min(known), max(known)
        if not .008 <= high / low - 1 <= .035:
            continue
        for rule in RULES:
            eligible[rule] += 1
        stage = {'breakout_retest': 0, 'failed_breakdown': 0}
        accepted = 0
        offer_second = start + REFERENCE
        for sec in range(offer_second, min(start + BLOCK, len(quotes) - HOLD - 75), 5):
            quote = quotes[sec]
            if not quote:
                accepted = 0
                for rule in RULES:
                    if 0 < stage[rule] < 3:
                        stage[rule] = 3
                continue
            bid, ask = quote[1:3]
            mode = 'breakout_retest'
            if stage[mode] == 0:
                accepted = accepted + 1 if bid >= high * 1.002 else 0
                if accepted == 2:
                    stage[mode] = 1
            elif stage[mode] in (1, 2):
                if bid < high * .9985:
                    stage[mode] = 3
                elif stage[mode] == 1 and ask <= high * 1.001:
                    stage[mode] = 2
                elif stage[mode] == 2 and bid >= high * 1.0015:
                    stage[mode] = 3
                    stop, target = high * .9985, high + (high - low)
                    if acceptable(ask, stop, target):
                        chosen[mode].append((sec, stop, target))
            mode = 'failed_breakdown'
            if stage[mode] == 0 and ask <= low * .9975:
                stage[mode] = 1
            elif stage[mode] == 1 and bid >= low * 1.0015:
                stage[mode] = 3
                stop, target = low * .9985, high
                if acceptable(ask, stop, target):
                    chosen[mode].append((sec, stop, target))
    return chosen, eligible


def replay(quotes, observations, fee, mode):
    cash = peak = 100.
    drawdown = 0.
    quantity = delayed = missing = misses = attempts = 0
    exit_reason = None
    exit_after = None
    candidates = {row[0]: row[1:] for row in observations}
    ledger = []
    for sec, quote in enumerate(quotes):
        if quantity:
            if quote and quote[3] >= quantity:
                value = quantity * quote[1] * (1 - fee)
                equity = cash + value
                peak = max(peak, equity)
                drawdown = max(drawdown, 1 - equity / peak)
                if exit_after is not None and sec >= exit_after:
                    cash += value
                    ledger.append((signal, entry, sec, quantity, cost, value, value - cost, exit_reason, stop, target))
                    quantity = 0
                    exit_reason = exit_after = None
            else:
                missing += 1
            if quantity and exit_after is not None and sec >= exit_after:
                delayed += 1
            if quantity and exit_after is None:
                if drawdown >= .08:
                    exit_reason = 'account_stop'
                elif mode == 'bracket' and quote and quote[1] <= stop:
                    exit_reason = 'stop'
                elif mode == 'bracket' and quote and quote[1] >= target:
                    exit_reason = 'target'
                elif sec >= entry + HOLD - 1:
                    exit_reason = 'timeout'
                if exit_reason:
                    exit_after = sec + 1
        if not quantity and drawdown < .08 and sec - 1 in candidates:
            attempts += 1
            signal = sec - 1
            stop, target = candidates[signal]
            lot = math.floor(24.5 / quotes[signal][2])
            if (not quote or quote[4] < lot or not 20 <= lot * quote[2] <= 25 or
                    not acceptable(quote[2], stop, target) or lot * quote[2] * (1 + fee) > cash):
                misses += 1
                continue
            cost = lot * quote[2] * (1 + fee)
            cash -= cost
            entry, quantity = sec, lot
            if quote[3] >= quantity:
                equity = cash + quantity * quote[1] * (1 - fee)
                peak = max(peak, equity)
                drawdown = max(drawdown, 1 - equity / peak)
                if drawdown >= .08:
                    exit_reason, exit_after = 'account_stop', sec + 1
                elif mode == 'bracket' and quote[1] <= stop:
                    exit_reason, exit_after = 'stop', sec + 1
            else:
                missing += 1
    if quantity or not math.isclose(cash - 100, math.fsum(item[6] for item in ledger), abs_tol=1e-9):
        raise ValueError('Exposure left open or ledger imbalance')
    return (len(observations), attempts, misses, len(ledger), sum(item[6] > 0 for item in ledger),
            cash - 100, drawdown * 100, missing, delayed), ledger


def screen(day):
    quotes = book(day, 'DOGEUSDT')
    chosen, eligible = opportunities(quotes)
    results, trades = [], []
    for rule in RULES:
        for mode in ('bracket', 'timeout_only'):
            for fee in (.001, .002):
                result, ledger = replay(quotes, chosen[rule], fee, mode)
                key = (day, rule, mode, fee)
                results.append((*key, eligible[rule], *result))
                trades.extend((*key, *row) for row in ledger)
    return day, results, trades


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('period', choices=BATCHES)
    parser.add_argument('--workers', type=int, default=6, choices=range(1, 7))
    options = parser.parse_args()
    with ProcessPoolExecutor(options.workers) as pool:
        outputs = list(pool.map(screen, BATCHES[options.period]))
    stem = ROOT / f'structural_retest_{options.period}'
    with stem.with_name(stem.name + '_results.csv').open('w', newline='') as results, stem.with_name(stem.name + '_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        header = ('day', 'rule', 'exit_rule', 'fee')
        writer.writerow((*header, 'eligible_ranges', 'signals', 'attempts', 'misses', 'trades', 'wins',
                         'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow((*header, 'signal_second', 'entry_second', 'exit_second', 'qty', 'debit_usdt', 'credit_usdt',
                          'net_usdt', 'reason', 'stop', 'target'))
        for day, result, ledger in outputs:
            writer.writerows(result)
            journal.writerows(ledger)
            print(day, len(ledger), flush=True)


if __name__ == '__main__':
    main()
