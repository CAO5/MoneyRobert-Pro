"""Fixed chronological screen of predicted net profit and win probability."""
import csv
import json
import math
import os
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from hf_pressure import CACHE
from independent_round import book
from participant_traces import replay
from sell_pressure import volume, window


ROOT = Path(__file__).parent
TRAIN = ('2021-04-01', '2021-07-01', '2021-10-01', '2022-01-01', '2022-04-01', '2022-07-01',
         '2022-10-01', '2023-01-01', '2023-04-01', '2023-07-01', '2023-10-01', '2023-12-01')
EVALUATION = {
    '2024': ('2024-01-01', '2024-03-01', '2024-06-01', '2024-08-01', '2024-10-01', '2024-12-01'),
    '2025_2026': ('2025-02-01', '2025-05-01', '2025-08-01', '2025-11-01', '2026-02-01',
                  '2026-05-01', '2026-08-01', '2026-11-01'),
}
HOLD = 600
STRESS = .002
THRESHOLD = 3.
MIN_PROBABILITY = .55


def log_mid(quote):
    return math.log((quote[1] + quote[2]) / 2)


def features(quotes, tape, decision):
    sells, buys, gaps = tape
    horizons = (10, 30, 120, 600)
    now = quotes[decision]
    historical = [quotes[decision - lag] for lag in horizons]
    if not now or not all(historical) or window(gaps, decision, 600):
        return None
    path = quotes[decision - 300:decision + 1:10]
    if not all(path):
        return None
    path_logs = [log_mid(quote) for quote in path]
    mid = path_logs[-1]
    moves = tuple(mid - log_mid(quote) for quote in historical)
    path_range = max(path_logs) - min(path_logs)
    variation = math.sqrt(math.fsum((later - earlier) ** 2 for earlier, later in zip(path_logs, path_logs[1:])))
    spread = math.log(now[2] / now[1])
    depth_balance = math.log(now[3] / now[4])
    depth = math.log1p((now[3] + now[4]) * math.exp(mid))
    signed = []
    activity = []
    for length in (30, 120, 600):
        bought = window(buys, decision, length)
        sold = window(sells, decision, length)
        total = bought + sold
        if total <= 0:
            return None
        activity.append(math.log1p(total / length))
        signed.append((bought - sold) / total)
    return (*moves, path_range, variation, spread, depth_balance, depth, *signed, *activity)


def training_fill(quotes, decision):
    chosen = quotes[decision]
    size = math.floor(24.5 / chosen[2])
    offset = decision + 1
    entry = quotes[offset]
    if not entry or entry[4] < size or not 20 <= size * entry[2] <= 25:
        return None
    for exit_second in range(offset + HOLD, len(quotes)):
        exit_quote = quotes[exit_second]
        if exit_quote and exit_quote[3] >= size:
            gain = 10000 * (exit_quote[1] * (1 - STRESS) / (entry[2] * (1 + STRESS)) - 1)
            return exit_second, gain
    return None


def collect(task):
    day, train = task
    try:
        quotes = book(day, 'DOGEUSDT')
        tape = volume(day, 'binance')
    except (OSError, ValueError) as error:
        raise RuntimeError(f'{day}: cannot load aligned spot data') from error
    observations = []
    next_label = 0
    for sec in range(719, len(quotes) - HOLD - 70, 120):
        if train and sec < next_label:
            continue
        sample = features(quotes, tape, sec)
        if sample is None:
            continue
        if train:
            fill = training_fill(quotes, sec)
            if fill is None:
                continue
            end, target = fill
            observations.append((sample, target))
            next_label = end + 1
        else:
            observations.append((sec, sample))
    return day, observations


def evaluate(task):
    day, observations, scores = task
    quotes = book(day, 'DOGEUSDT')
    outcomes, transactions = [], []
    for name, edges in scores.items():
        events = [(sec, HOLD, float(gain), float(odds), 0.)
                  for (sec, _), (gain, odds) in zip(observations, edges)
                  if name == 'eligible_clock' or (gain > THRESHOLD and odds >= MIN_PROBABILITY)]
        for fee in (.001, STRESS):
            result, journal = replay(quotes, events, fee)
            outcomes.append((day, name, fee, len(observations), *result))
            transactions.extend((day, name, fee, *row) for row in journal)
    return outcomes, transactions


def fit_linear(samples, targets):
    from sklearn.linear_model import LogisticRegression, Ridge
    from sklearn.pipeline import make_pipeline
    from sklearn.preprocessing import StandardScaler

    money = make_pipeline(StandardScaler(), Ridge(alpha=100)).fit(samples, targets.clip(-300, 300))
    odds = make_pipeline(StandardScaler(), LogisticRegression(C=.1, max_iter=300)).fit(samples, targets > 0)
    return money, odds


def fit_tree(samples, targets):
    from sklearn.ensemble import HistGradientBoostingClassifier, HistGradientBoostingRegressor

    options = dict(max_iter=100, max_leaf_nodes=7, min_samples_leaf=75, l2_regularization=1.,
                   learning_rate=.05, early_stopping=False, random_state=20261119)
    money = HistGradientBoostingRegressor(**options).fit(samples, targets.clip(-300, 300))
    odds = HistGradientBoostingClassifier(**options).fit(samples, targets > 0)
    return money, odds


def score(models, observations):
    import numpy as np

    features_array = np.array([row[1] for row in observations])
    predictions = {'eligible_clock': [(0., 0.)] * len(observations)}
    for name, (pnl, win) in models.items():
        predictions[name] = list(zip(pnl.predict(features_array), win.predict_proba(features_array)[:, 1])) if observations else []
    return predictions


def main():
    import numpy as np
    import sklearn

    days = {day: group for group, members in EVALUATION.items() for day in members}
    if set(days) & set(TRAIN) or max(TRAIN) >= min(days):
        raise ValueError('Training must precede evaluation')
    CACHE.mkdir(exist_ok=True)
    with ProcessPoolExecutor(5) as pool:
        jobs = [(day, True) for day in TRAIN] + [(day, False) for day in days]
        dataset = dict(pool.map(collect, jobs))
        rows = [row for day in TRAIN for row in dataset[day]]
        samples, targets = map(np.array, zip(*rows))
        fits = {name: pool.submit(method, samples, targets) for name, method in (('linear', fit_linear), ('shallow_tree', fit_tree))}
        models = {name: job.result() for name, job in fits.items()}
        tasks = [(day, dataset[day], score(models, dataset[day])) for day in days]
        outcomes, transactions = [], []
        for totals, ledger in pool.map(evaluate, tasks):
            outcomes.extend(totals)
            transactions.extend(ledger)
    with (ROOT / 'learned_edges_results.csv').open('w', newline='') as results, (ROOT / 'learned_edges_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('period', 'day', 'model', 'fee', 'eligible', 'raw_signals', 'signal_seconds', 'attempts',
                         'misses', 'trades', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('period', 'day', 'model', 'fee', 'signal_second', 'hold', 'predicted_stress_net_bps',
                          'predicted_stress_win_probability', 'unused', 'entry_second', 'exit_second', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        writer.writerows((days[row[0]], *row) for row in outcomes)
        journal.writerows((days[row[0]], *row) for row in transactions)
    provenance = dict(train={day: len(dataset[day]) for day in TRAIN},
                      evaluated={day: len(dataset[day]) for day in days},
                      train_stress_winners=int((targets > 0).sum()), stress_fee=STRESS,
                      threshold_bps=THRESHOLD, min_probability=MIN_PROBABILITY,
                      scikit_learn=sklearn.__version__, numpy=np.__version__)
    (ROOT / 'learned_edges_run.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf8')
    print('Train observations, stress winners:', len(targets), provenance['train_stress_winners'])
    print('Completed ledger rows:', len(transactions))


if __name__ == '__main__':
    os.environ.setdefault('OMP_NUM_THREADS', '1')
    os.environ.setdefault('OPENBLAS_NUM_THREADS', '1')
    main()
