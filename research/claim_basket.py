"""Cash-paid DOGE long and inverse claims as a basket. Neither claim is sold short."""
import csv
import math
from pathlib import Path

from independent_round import book


ROOT = Path(__file__).parent
DAYS = tuple(f'2023-{month:02}-01' for month in range(7, 13))
STEP = .001


def events(long, inverse, underlying):
    signals = {'both_discount': {}, 'both_premium': {}}
    known = quiet = 0
    for sec in range(900, len(long) - 630, 10):
        if 57600 - 630 <= sec < 57600 + 900:
            continue
        if not all(series[at] for series in (long, inverse, underlying) for at in (sec - 900, sec)):
            continue
        known += 1
        before, current = underlying[sec - 900], underlying[sec]
        change = math.log((current[1] + current[2]) / (before[1] + before[2]))
        if abs(change) > .0025:
            continue
        quiet += 1
        drops = [series[sec][2] / series[sec - 900][1] - 1 for series in (long, inverse)]
        rises = [series[sec][1] / series[sec - 900][2] - 1 for series in (long, inverse)]
        if max(drops) < 0 and -.2 < min(drops) and sum(drops) / 2 <= -.01:
            signals['both_discount'][sec] = (change, *drops)
        if min(rises) > 0 and max(rises) < .2 and sum(rises) / 2 >= .01:
            signals['both_premium'][sec] = (change, *rises)
    return known, quiet, signals


def replay(long, inverse, signals, fee):
    cash = peak = 100.
    dd = 0.
    units = [0., 0.]
    entries = [0., 0.]
    state = 'flat'
    ledger = []
    attempts = misses = unseen = delays = 0
    for sec, pair in enumerate(zip(long, inverse)):
        if state == 'holding' and sec >= deadline:
            state = 'close_long'
        if state in ('buy_long', 'buy_inverse'):
            leg = int(state == 'buy_inverse')
            qty, quote = target[leg], pair[leg]
            if quote and 20 <= qty * quote[2] <= 25 and quote[4] >= qty and cash >= qty * quote[2] * (1 + fee):
                cash -= qty * quote[2] * (1 + fee)
                units[leg], entries[leg] = qty, quote[2]
                if leg:
                    paired = True
                    deadline = sec + 600
                    state = 'holding'
                else:
                    state = 'buy_inverse'
            else:
                misses += 1
                state = 'orphan' if leg else 'flat'
        elif state in ('close_long', 'close_inverse', 'orphan'):
            leg = int(state == 'close_inverse')
            qty, quote = units[leg], pair[leg]
            if quote and quote[3] >= qty:
                if not .5 <= quote[1] / entries[leg] <= 1.5:
                    raise ValueError('Unverified repricing or split during exposure')
                cash += qty * quote[1] * (1 - fee)
                units[leg] = 0.
                if state == 'close_long':
                    state = 'close_inverse'
                else:
                    ledger.append((signal, sec, *signals[signal], *target, paired, cash - before_cash))
                    state = 'flat'
            else:
                delays += 1
        if all(not qty or pair[leg] for leg, qty in enumerate(units)):
            equity = cash + sum(qty * pair[leg][1] * (1 - fee) for leg, qty in enumerate(units) if qty)
            peak = max(peak, equity)
            dd = max(dd, 1 - equity / peak)
            if dd >= .08 and state == 'holding':
                deadline = min(deadline, sec + 1)
            if dd >= .08 and state == 'buy_inverse':
                misses += 1
                state = 'orphan'
        else:
            unseen += 1
        if state == 'flat' and sec in signals and sec + 630 < len(long) and dd < .08:
            target = [math.floor(24.5 / quote[2] / STEP) * STEP for quote in pair]
            attempts += 1
            before_cash, signal, paired = cash, sec, False
            state = 'buy_long'
    if state != 'flat' or any(units) or not math.isclose(cash - 100, math.fsum(row[-1] for row in ledger), abs_tol=1e-8):
        raise ValueError('Open shares or cash mismatch')
    completed = [row for row in ledger if row[-2]]
    return (attempts, misses, len(ledger) - len(completed), len(completed), sum(row[-1] > 0 for row in completed),
            cash - 100, 100 * dd, unseen, delays), ledger


def main():
    with (ROOT / 'claim_basket_results.csv').open('w', newline='') as results, (ROOT / 'claim_basket_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('day', 'rule', 'fee', 'valid_points', 'quiet_points', 'signals', 'attempts', 'misses', 'orphan_sales', 'paired_trades', 'paired_wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('day', 'rule', 'fee', 'signal_second', 'exit_second', 'underlying_change', 'long_change', 'inverse_change', 'long_requested', 'inverse_requested', 'paired', 'net_usdt'))
        for day in DAYS:
            long, inverse, underlying = (book(day, symbol, 'gate-io') for symbol in ('DOGE3L_USDT', 'DOGE3S_USDT', 'DOGE_USDT'))
            valid, quiet, signals = events(long, inverse, underlying)
            for rule, found in signals.items():
                for fee in (.001, .002):
                    result, ledger = replay(long, inverse, found, fee)
                    writer.writerow((day, rule, fee, valid, quiet, len(found), *result))
                    journal.writerows((day, rule, fee, *row) for row in ledger)
            print(day, valid, quiet, flush=True)


if __name__ == '__main__':
    main()
