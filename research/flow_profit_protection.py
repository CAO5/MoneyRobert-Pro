"""Compare a fixed profit protection rule with the original DOGE ten-minute exit."""
from concurrent.futures import ProcessPoolExecutor

from flow_path import flow_path
from flow_sector import FEES, PHASES, write
from independent_round import book
from participant_traces import replay


# Evaluate both ledgers against the same double-fee threshold before issuing an exit.
PROTECTION = (.002, .002, .0005)


def study(day):
    quotes = book(day, 'DOGEUSDT')
    signals, _ = flow_path(day, quotes)
    original = signals['balanced_drop']
    summary, pairs, separate = [], [], []
    for fee in FEES:
        original_result, original_trades = replay(quotes, original, fee)
        full_result, full_trades = replay(quotes, original, fee, profit_protection=PROTECTION)
        original_by_signal = {trade[0]: trade for trade in original_trades}
        matched_observations = [event for event in original if event[0] in original_by_signal]
        matched_result, matched_trades = replay(quotes, matched_observations, fee, profit_protection=PROTECTION)
        if len(matched_trades) != len(original_trades):
            raise ValueError(f'Could not reproduce all paired entries for {day}')
        key = (PHASES[day], day, fee)
        for protected in matched_trades:
            before = original_by_signal[protected[0]]
            if protected[:6] != before[:6] or protected[7:9] != before[7:9] or protected[6] > before[6]:
                raise ValueError(f'Paired entry or exit mismatch for {day}, signal {before[0]}')
            delta = protected[-1] - before[-1]
            pairs.append((*key, before[0], before[5], before[6], protected[6], before[8],
                          before[-1], protected[-1], max(delta, 0.), max(-delta, 0.), delta))
        for variant, result in (('original', original_result), ('paired', matched_result), ('full', full_result)):
            summary.append((*key, variant, *result[1:]))
        separate.extend((*key, *trade) for trade in full_trades)
    return summary, pairs, separate


def main():
    with ProcessPoolExecutor(8) as pool:
        runs = list(pool.map(study, PHASES))
    summary, pairs, separate = zip(*runs)
    key = ('phase', 'day', 'fee')
    write('flow_profit_protection_results.csv',
          (*key, 'variant', 'signals', 'attempts', 'misses', 'trades', 'wins', 'net_usdt', 'dd_pct',
           'unpriced_seconds', 'delayed_exit_seconds'), summary)
    write('flow_profit_protection_pairs.csv',
          (*key, 'signal_sec', 'entry_sec', 'original_exit_sec', 'protected_exit_sec', 'debit_usdt',
           'original_net_usdt', 'protected_net_usdt', 'saved_usdt', 'forgone_usdt', 'delta_usdt'), pairs)
    write('flow_profit_protection_trades.csv',
          (*key, 'signal_sec', 'hold', 'shock', 'net_flow_share', 'turnover', 'entry_sec', 'exit_sec',
           'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'), separate)
    print(f'{len(runs)} days replayed, {sum(map(len, pairs))} pairs across both fees', flush=True)


if __name__ == '__main__':
    main()
