"""Summarize CDFP quality, five-minute outcomes, and walk-forward prediction."""

import argparse
import json
import math
import sqlite3
from bisect import bisect_right
from collections import Counter, defaultdict
from contextlib import closing
from datetime import UTC, datetime
from pathlib import Path
from statistics import fmean

from cdfp import ASSETS, DAY, VERSION


EDGES = (-.6, -.2, .2, .6)
GROUPS = ('[-1,-0.6)', '[-0.6,-0.2)', '[-0.2,0.2)', '[0.2,0.6)', '[0.6,1]')
BASE = ('momentum', 'spot_imbalance', 'perp_imbalance')
MODELS = {
    'cdfp_only': ('cdfp_value',),
    'momentum_only': ('momentum',),
    'spot_only': ('spot_imbalance',),
    'perp_only': ('perp_imbalance',),
    'combined_controls': BASE,
    'combined_with_cdfp': (*BASE, 'cdfp_value'),
}
MIN_TRAIN = 30


def date(stamp):
    return datetime.fromtimestamp(stamp / 1000, UTC).date().isoformat()


def grouped(rows):
    groups = [[] for _ in GROUPS]
    for row in rows:
        groups[bisect_right(EDGES, row['cdfp_value'])].append(row)
    return zip(GROUPS, groups)


def conditional(rows):
    result = {}
    for group, bucket in grouped(rows):
        returns = [row['log_return'] for row in bucket]
        result[group] = {
            'count': len(returns),
            'mean_log_return': fmean(returns) if returns else None,
            'positive_return_fraction': fmean(value > 0 for value in returns) if returns else None,
        }
    return result


def nonoverlap(rows):
    selected = []
    until = -1
    for row in rows:
        if row['price_time'] >= until:
            selected.append(row)
            until = row['label_time']
    return selected


def fit(train, fields):
    """Least squares with centered, orthogonal columns and an intercept."""
    returns = [row['log_return'] for row in train]
    intercept = fmean(returns)
    basis, terms = [], []
    for field in fields:
        mean = fmean(row[field] for row in train)
        column = [row[field] - mean for row in train]
        projections = []
        for previous in basis:
            projection = math.fsum(a * b for a, b in zip(column, previous))
            projections.append(projection)
            column = [a - projection * b for a, b in zip(column, previous)]
        norm = math.sqrt(math.fsum(value * value for value in column))
        if norm < 1e-10:
            continue
        unit = [value / norm for value in column]
        basis.append(unit)
        target = math.fsum(a * (b - intercept) for a, b in zip(unit, returns))
        terms.append((field, mean, projections, norm, target))

    def predict(row):
        components, result = [], intercept
        for field, mean, projections, norm, target in terms:
            value = (row[field] - mean - math.fsum(
                a * b for a, b in zip(projections, components)
            )) / norm
            components.append(value)
            result += value * target
        return result

    return predict


def correlation(rows):
    if len(rows) < 2:
        return None
    scores = [row['cdfp_value'] for row in rows]
    returns = [row['log_return'] for row in rows]
    mean_score, mean_return = fmean(scores), fmean(returns)
    scores = [value - mean_score for value in scores]
    returns = [value - mean_return for value in returns]
    scale = math.sqrt(math.fsum(value ** 2 for value in scores) *
                      math.fsum(value ** 2 for value in returns))
    return math.fsum(a * b for a, b in zip(scores, returns)) / scale if scale else None


def compare(errors, count):
    if not count:
        return {'count': 0, 'mse': {}, 'cdfp_only_mse_reduction_vs': {},
                'incremental_cdfp_mse_reduction': None}
    controls = errors['combined_controls']
    return {
        'count': count,
        'mse': {name: value / count for name, value in errors.items()},
        'cdfp_only_mse_reduction_vs': {
            name: 1 - errors['cdfp_only'] / errors[name] if errors[name] else None
            for name in ('momentum_only', 'spot_only', 'perp_only')
        },
        'incremental_cdfp_mse_reduction': 1 - errors['combined_with_cdfp'] / controls if controls else None,
    }


def walk_forward(rows):
    blocks = defaultdict(list)
    for row in rows:
        blocks[row['available_time'] // DAY * DAY].append(row)
    train, tested, by_day = [], [], []
    totals = Counter()
    for stamp, test in sorted(blocks.items()):
        # A training label must be complete before the test block opens.
        eligible = [row for row in train if row['completed_time'] < stamp]
        if len(eligible) >= MIN_TRAIN:
            errors = {}
            for name, fields in MODELS.items():
                predictor = fit(eligible, fields)
                errors[name] = math.fsum((predictor(row) - row['log_return']) ** 2 for row in test)
            totals.update(errors)
            tested.extend(test)
            by_day.append({'utc_day': date(stamp), 'train_count': len(eligible),
                           'score_return_correlation': correlation(test), **compare(errors, len(test))})
        train.extend(test)
    return {
        **compare(totals, len(tested)),
        'score_return_correlation': correlation(tested),
        'conditional_returns': conditional(tested),
        'utc_day_blocks': by_day,
    }


def summarize(rows):
    count = len(rows)
    checks = {
        'data_gap': lambda row: row['quality_status'] == 'GAP',
        'flow_warmup': lambda row: row['quality_status'] == 'WARMUP',
        'low_volume': lambda row: row['volume_quality'] == 'LOW',
        'volume_unknown': lambda row: row['volume_quality'] == 'UNKNOWN',
        'market_warmup': lambda row: row['market_regime'] == 'WARMUP',
        'extreme_market': lambda row: row['market_regime'] == 'EXTREME',
        'missing_five_minute_label': lambda row: row['cdfp_value'] is not None
        and row['label_status'] != 'COMPLETE',
    }
    exclusions = {}
    for name, blocked in checks.items():
        rejected = sum(blocked(row) for row in rows)
        exclusions[name] = {'count': rejected, 'fraction_of_all_samples': rejected / count if count else None}
    scored = [row for row in rows if row['cdfp_value'] is not None]
    eligible = [row for row in scored if row['quality_status'] == 'OK' and row['volume_quality'] == 'OK'
                and row['market_regime'] == 'NORMAL' and row['label_status'] == 'COMPLETE'
                and all(row[field] is not None for field in BASE)]
    selected = nonoverlap(eligible)
    return {
        'samples': count,
        'scored_samples': len(scored),
        'five_minute_label_status': dict(Counter(row['label_status'] for row in rows)),
        'exclusions': exclusions,
        'score_distribution': {group: len(bucket) for group, bucket in grouped(scored)},
        'evaluation_eligible': len(eligible),
        'nonoverlapping_samples': len(selected),
        'conditional_returns_nonoverlapping': conditional(selected),
        'score_return_correlation_nonoverlapping': correlation(selected),
        'out_of_sample': walk_forward(selected),
    }


def report(database):
    with closing(sqlite3.connect(database.resolve().as_uri() + '?mode=ro', uri=True)) as db:
        db.row_factory = sqlite3.Row
        rows = db.execute(
            """SELECT s.asset, s.available_time, s.price_time, s.cdfp_value, s.momentum,
                      s.spot_imbalance, s.perp_imbalance, s.quality_status, s.volume_quality, s.market_regime,
                      l.status AS label_status, l.log_return, l.price_time AS label_time, l.completed_time
               FROM samples s JOIN labels l USING (asset, factor_version, factor_time)
               WHERE s.factor_version=? AND l.minutes=5
               ORDER BY s.asset, s.available_time, s.factor_time""", (VERSION,),
        )
        by_asset = {asset: [] for asset in ASSETS}
        for row in rows:
            by_asset[row['asset']].append(row)
    assets = {asset: summarize(by_asset[asset]) for asset in ASSETS}
    return {
        'factor_version': VERSION,
        'primary_horizon_minutes': 5,
        'min_training_samples_per_asset': MIN_TRAIN,
        'status': 'REVIEWABLE' if all(item['out_of_sample']['count'] for item in assets.values())
        else 'AWAITING_SAMPLE',
        'assets': assets,
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('database', type=Path)
    args = parser.parse_args()
    print(json.dumps(report(args.database), indent=2, allow_nan=False))
