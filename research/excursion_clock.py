"""Compare downward and upward crossing times over the same price band. Public spot quotes only."""
import csv
import math
from pathlib import Path

from independent_round import book


ROOT = Path(__file__).parent
STEPS = {'DOGEUSDT': 1., 'BTCUSDT': .00001, 'SOLUSDT': .001}
DAYS = ('2024-01-01', '2024-02-01', '2024-03-01')


def crossings(quotes):
    result = {}
    for block in range(0, len(quotes), 600):
        anchor = None
        lower_touch = None
        for sec in range(block, min(block + 600, len(quotes))):
            quote = quotes[sec]
            if not quote:
                anchor = lower_touch = None
                continue
            mid = (quote[1] + quote[2]) / 2
            if anchor is None:
                anchor, upper_touch = mid, sec
                continue
            if lower_touch is None:
                if mid >= anchor:
                    upper_touch = sec
                elif mid <= anchor * .998:
                    descent, lower_touch = sec - upper_touch, sec
            elif mid <= anchor * .998:
                lower_touch = sec
            elif mid >= anchor:
                ascent = sec - lower_touch
                if min(ascent, descent) >= 3:
                    result[sec] = (descent, ascent)
                break
    return result


def replay(quotes, signals, step, rule, fee):
    cash = peak = 100.
    drawdown = units = 0.
    pending = None
    entry = basis = 0
    seen = misses = unpriced = 0
    sells = []
    for sec, quote in enumerate(quotes):
        if pending:
            signal, wanted = pending
            pending = None
            if quote and quote[4] >= wanted and 20 <= wanted * quote[2] <= 25 and wanted * quote[2] * (1 + fee) <= cash:
                entry, units, durations = sec, wanted, signals[signal]
                basis = units * quote[2] * (1 + fee)
                cash -= basis
            else:
                misses += 1
        if units and (sec >= entry + 300 or drawdown >= .08) and quote and quote[3] >= units:
            proceeds = units * quote[1] * (1 - fee)
            sells.append((entry, sec, *durations, units, basis, proceeds, proceeds - basis))
            cash += proceeds
            units = 0
        if not units or quote:
            equity = cash + (units * quote[1] * (1 - fee) if units else 0)
            peak = max(peak, equity)
            drawdown = max(drawdown, 1 - equity / peak)
        else:
            unpriced += 1
        if not units and sec in signals and sec + 310 < len(quotes) and drawdown < .08:
            descent, ascent = signals[sec]
            if (rule == 'fast_recovery' and 2 * ascent <= descent) or (rule == 'slow_recovery' and 2 * descent <= ascent):
                seen += 1
                pending = (sec, math.floor(24.5 / quote[2] / step) * step)
    if units or pending or not math.isclose(cash - 100, math.fsum(item[-1] for item in sells), abs_tol=1e-8):
        raise ValueError('Unresolved position or cash discrepancy')
    return (len(signals), seen, len(sells), sum(item[-1] > 0 for item in sells), cash - 100, drawdown * 100, misses, unpriced), sells


def main():
    with (ROOT / 'excursion_clock_results.csv').open('w', newline='') as results, (ROOT / 'excursion_clock_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('day', 'symbol', 'rule', 'fee', 'observations', 'signals', 'trades', 'wins', 'net_usdt', 'dd_pct', 'entry_misses', 'unpriced_seconds'))
        journal.writerow(('day', 'symbol', 'rule', 'fee', 'entry_second', 'exit_second', 'down_seconds', 'up_seconds', 'quantity', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        for day in DAYS:
            for symbol, step in STEPS.items():
                quotes = book(day, symbol)
                signals = crossings(quotes)
                for rule in ('fast_recovery', 'slow_recovery'):
                    for fee in (.001, .002):
                        result, ledger = replay(quotes, signals, step, rule, fee)
                        writer.writerow((day, symbol, rule, fee, *result))
                        journal.writerows((day, symbol, rule, fee, *row) for row in ledger)
                print(day, symbol, len(signals), flush=True)


if __name__ == '__main__':
    main()
