"""Earliest-close count and optimal cash bounds for constant-quantity inventory cycles."""
import csv
import math
from collections import deque

from hedged_passive import DAYS, ROOT
from hf_pressure import CACHE
from independent_round import book


def transfers(home, away, fee, passive_home):
    for local, remote in zip(home, away):
        if not local or not remote:
            yield None
            continue
        home_buy = local[1 if passive_home else 2]
        home_sell = local[2 if passive_home else 1]
        yield (remote[1] * (1 - fee) - home_buy * (1 + fee),
               home_sell * (1 - fee) - remote[2] * (1 + fee))


def bounds(values, duration):
    entry = deque(), deque()
    single = deque(), deque()
    equity = deque(), deque()
    count = 0
    best = 0.
    best_single = None
    for sec, pair in enumerate(values):
        if pair is None:
            continue
        for histories in (entry, single, equity):
            for queue in histories:
                while queue and queue[0][0] < sec - duration:
                    queue.popleft()
        previous_best = best
        strongest_single = [single[direction][0][1] + pair[1 - direction] for direction in (0, 1) if single[direction]]
        if strongest_single:
            best_single = max(strongest_single + ([best_single] if best_single is not None else []))
        possible = [entry[direction][0][1] + pair[1 - direction] for direction in (0, 1) if entry[direction]]
        if possible:
            strongest = max(possible)
            if strongest > 0:
                count += 1
                for queue in entry:
                    queue.clear()
            else:
                for queue, value in zip(entry, pair):
                    push(queue, sec, value)
        else:
            for queue, value in zip(entry, pair):
                push(queue, sec, value)
        for direction in (0, 1):
            if equity[direction]:
                best = max(best, equity[direction][0][1] + pair[1 - direction])
        for queue, value in zip(equity, pair):
            push(queue, sec, previous_best + value)
        for queue, value in zip(single, pair):
            push(queue, sec, value)
    return count, best, best_single


def push(queue, sec, value):
    while queue and queue[-1][1] <= value:
        queue.pop()
    queue.append((sec, value))


def main():
    CACHE.mkdir(exist_ok=True)
    with (ROOT / 'inventory_cycle_bound_results.csv').open('w', newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day', 'home_liquidity', 'fee', 'max_hold_seconds', 'joint_quote_seconds',
                         'max_profitable_cycles_no_overlap', 'oracle_usdt_per_fixed_doge', 'max_single_usdt_per_doge'))
        for day in DAYS:
            home, away = book(day, 'DOGEUSDT'), book(day, 'DOGE_USDT', 'gate-io')
            joint = sum(bool(local and remote) for local, remote in zip(home, away))
            for passive_home in (False, True):
                for fee in (.001, .002):
                    values = list(transfers(home, away, fee, passive_home))
                    for duration in (180, 600):
                        numbers = bounds(values, duration)
                        if not math.isfinite(numbers[1]):
                            raise ValueError('Non-finite oracle pnl')
                        writer.writerow((day, 'assumed_maker' if passive_home else 'taker', fee, duration, joint, *numbers))
            print(day, joint, flush=True)


if __name__ == '__main__':
    main()
