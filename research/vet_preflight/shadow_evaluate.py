"""Audit eligibility or run the frozen VET five-minute variance research comparison."""

import argparse
import csv
import json
import math
from collections import defaultdict
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from statistics import fmean

DAY = 86_400_000
FLOOR = 1e-12
MIN_DAILY = 30
MIN_TRAIN_BLOCKS = 7
MIN_TRAIN_SAMPLES = 300
MIN_TEST_BLOCKS = 3
ASSETS = ('BTC', 'ETH', 'SOL')
MODELS = {'B0': (0, 1, 2), 'B1': (0, 1, 2, 3), 'B2': (0, 1, 2, 4),
          'B3': (0, 1, 2, 3, 4), 'B4': (0, 1, 2, 3, 4, 5)}


@dataclass(frozen=True)
class Sample:
    origin: int
    target: int
    completed: int
    features: tuple
    outcome: float


def read(path, asof):
    rows = defaultdict(list)
    with path.open(newline='', encoding='utf-8-sig') as source:
        for row in csv.DictReader(source):
            origin, target, completed = (int(row[key]) for key in ('origin_ts', 'target_ts', 'completed_at'))
            if not int(row['available_at']) < origin or target != origin + 300_000 or completed < target:
                raise ValueError('Invalid five-minute label chronology')
            if row['quality_status'] != 'OK' or row['label_status'] != 'COMPLETE' or completed > asof:
                continue
            prior = [float(row[key]) for key in ('past_1m_rv', 'past_5m_rv', 'past_30m_rv')]
            signals = [float(row[key]) for key in ('a', 'l_depth', 'vet')]
            outcome = float(row['future_5m_variance'])
            if not all(math.isfinite(x) and x >= 0 for x in (*prior, *signals, outcome)):
                raise ValueError('Non-finite or negative research input')
            x = tuple(math.log(max(FLOOR, rv ** 2)) for rv in prior) + tuple(signals)
            rows[row['asset']].append(Sample(origin, target, completed, x, outcome))
    return rows


def held_out(rows, asof):
    blocks = defaultdict(list)
    last_target = -1
    for row in sorted(rows, key=lambda row: row.origin):
        day = row.origin // DAY * DAY
        if row.origin >= last_target and day + DAY <= asof:
            blocks[day].append(row)
            last_target = row.target
    blocks = {k: v for k, v in blocks.items() if len(v) >= MIN_DAILY}
    result = []
    for stamp, test in sorted(blocks.items()):
        prior = [[row for row in block if row.completed < stamp and row.target < stamp]
                 for day, block in blocks.items() if day < stamp]
        prior = [block for block in prior if len(block) >= MIN_DAILY]
        train = [row for block in prior for row in block]
        if len(prior) >= MIN_TRAIN_BLOCKS and len(train) >= MIN_TRAIN_SAMPLES:
            result.append((stamp, train, test))
    return result, len(blocks)


def fit(rows, fields):
    # Fit only the training portion of each purged fold by orthogonal least squares.
    responses = [math.log(max(FLOOR, row.outcome)) for row in rows]
    average = fmean(responses)
    basis, terms = [], []
    for field in fields:
        mean = fmean(row.features[field] for row in rows)
        column = [row.features[field] - mean for row in rows]
        projections = []
        for unit in basis:
            weight = math.fsum(a * b for a, b in zip(column, unit))
            projections.append(weight)
            column = [a - weight * b for a, b in zip(column, unit)]
        scale = math.sqrt(math.fsum(x * x for x in column))
        if scale <= 1e-10:
            continue
        unit = [x / scale for x in column]
        basis.append(unit)
        slope = math.fsum(x * (y - average) for x, y in zip(unit, responses))
        terms.append((field, mean, projections, scale, slope))

    def prediction(sample):
        value = average
        coordinates = []
        for field, mean, weights, scale, slope in terms:
            coordinate = (sample.features[field] - mean - math.fsum(
                a * b for a, b in zip(weights, coordinates)
            )) / scale
            coordinates.append(coordinate)
            value += slope * coordinate
        return max(FLOOR, math.exp(value))
    return prediction


def evaluation(folds):
    result = []
    for stamp, train, test in folds:
        losses = {}
        for model, fields in MODELS.items():
            predict = fit(train, fields)
            forecasts = [predict(row) for row in test]
            losses[model] = fmean(math.log(hat) + row.outcome / hat for row, hat in zip(test, forecasts))
        result.append({'utc_test_day': datetime.fromtimestamp(stamp / 1000, UTC).date().isoformat(),
                       'training_samples': len(train), 'test_samples': len(test),
                       'mean_qlike': losses, 'b4_improvement_vs_b3': losses['B3'] - losses['B4']})
    return result


def assess(path, asof, run_models=False):
    rows = read(path, asof)
    results = {}
    grouped_folds = {}
    for asset in ASSETS:
        folds, blocks = held_out(rows[asset], asof)
        grouped_folds[asset] = folds
        results[asset] = {
            'quality_valid_five_minute_labels': len(rows[asset]), 'closed_utc_days_with_minimum': blocks,
            'eligible_test_blocks': len(folds), 'ready': len(folds) >= MIN_TEST_BLOCKS,
        }
    ready = all(row['ready'] for row in results.values())
    if run_models and ready:
        for asset in ASSETS:
            folds = evaluation(grouped_folds[asset])
            improvements = [fold['b4_improvement_vs_b3'] for fold in folds]
            results[asset].update(folds=folds, mean_paired_day_improvement=fmean(improvements),
                                  days_b4_improved=sum(value > 0 for value in improvements))
    return {'ready': ready, 'fit_executed': run_models and ready, 'assets': results}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('exported_samples', type=Path)
    parser.add_argument('--as-of-ms', type=int, default=int(datetime.now(UTC).timestamp() * 1000))
    parser.add_argument('--evaluate', action='store_true', help='Fit only after the frozen readiness gate')
    args = parser.parse_args()
    print(json.dumps(assess(args.exported_samples, args.as_of_ms, args.evaluate), indent=2, allow_nan=False))
