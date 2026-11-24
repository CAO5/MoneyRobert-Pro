"""Regression checks for CDFP receipt timing, coverage, and evaluation."""

import math
import sqlite3
import unittest
from collections import deque

from cdfp import ASSETS, BIN, DAY, GRACE, MARKETS, MIN_HISTORY, SCHEMA, Study, common
from cdfp_report import nonoverlap, walk_forward


class StudyTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(':memory:')
        self.db.row_factory = sqlite3.Row
        self.db.executescript(SCHEMA)
        multipliers = {name: .1 if name.endswith('-SWAP') else 1.0 for name in MARKETS}
        self.study = Study(self.db, multipliers, 0, clock=lambda: 0)
        self.ids = dict.fromkeys(MARKETS, 0)
        for market in MARKETS:
            self.observe('trades-all', {
                'event': 'subscribe', 'arg': {'instId': market},
            }, 1_000)

    def tearDown(self):
        self.db.close()

    def observe(self, kind, data, now):
        wall = now * 1_000_000
        self.study.observe(kind, data, wall, wall)

    def trade(self, market, event, now, *, price=100, size=1, side='buy', skip_id=False):
        self.ids[market] += 2 if skip_id else 1
        self.observe('trades-all', {'arg': {'instId': market}, 'data': [{
            'instId': market, 'ts': str(event), 'tradeId': str(self.ids[market]),
            'px': str(price), 'sz': str(size), 'side': side, 'source': '0',
        }]}, now)

    def close_bin(self, end):
        self.observe('pong', {'instruments': MARKETS}, end + 1)
        self.study.tick(end + GRACE)

    def sample(self, end):
        return self.db.execute(
            "SELECT * FROM samples WHERE asset='BTC' AND factor_time=?", (end,),
        ).fetchone()

    def baseline(self):
        for end in range(BIN, 8 * BIN, BIN):
            if end == 5 * BIN:
                self.trade('BTC-USDT', end - 100, end - 50)
                self.trade('BTC-USDT-SWAP', end - 100, end - 40, size=60)
                self.trade('BTC-USDT-SWAP', end - 100, end - 30, size=40, side='sell')
            self.close_bin(end)

    def test_formula_healthy_empty_bucket_and_late_gap(self):
        self.assertEqual(common(.9, .1), .1)
        self.assertEqual(common(-.9, -.1), -.1)
        self.assertEqual(common(-.9, .1), 0)
        self.baseline()
        self.assertEqual(self.sample(6 * BIN)['quality_status'], 'GAP')
        row = self.sample(7 * BIN)
        self.assertEqual(row['quality_status'], 'OK')
        self.assertEqual(row['market_regime'], 'WARMUP')
        self.assertEqual(row['same_direction_bins'], 1)
        self.assertAlmostEqual(row['cdfp_value'], .2 / 6)
        self.assertAlmostEqual(row['spot_imbalance'], 1 / 6)
        self.assertAlmostEqual(row['perp_imbalance'], .2 / 6)
        self.trade('BTC-USDT-SWAP', 7 * BIN - 500, 7 * BIN + GRACE + 1)
        row = self.sample(7 * BIN)
        self.assertIsNone(row['cdfp_value'])
        self.assertAlmostEqual(row['issued_cdfp_value'], .2 / 6)
        self.assertEqual(row['quality_status'], 'GAP')
        self.assertIsNotNone(row['gap_detected_time'])

    def test_disconnect_leaves_unknown_bins_and_requires_six_clean_bins(self):
        self.baseline()
        self.observe('disconnect', {'instruments': ('BTC-USDT-SWAP',)}, 76_000)
        self.close_bin(8 * BIN)
        self.observe('trades-all', {'event': 'subscribe', 'arg': {'instId': 'BTC-USDT-SWAP'}}, 86_000)
        for end in range(9 * BIN, 16 * BIN, BIN):
            self.close_bin(end)
        self.assertEqual(self.sample(14 * BIN)['quality_status'], 'GAP')
        self.assertEqual(self.sample(15 * BIN)['cdfp_value'], 0)

    def test_forward_labels_follow_actual_available_and_start_times(self):
        self.baseline()
        end = 7 * BIN
        available = self.sample(end)['available_time']
        self.assertEqual(available, end + GRACE)
        self.trade('BTC-USDT', available - 2, available + 1, price=80)
        self.assertIsNone(self.sample(end)['price_time'])
        start = available + 3
        self.trade('BTC-USDT', start - 1, start, price=100)
        self.assertEqual(self.sample(end)['price_time'], start)
        prices = {1: 105, 5: 50, 15: 120}
        for minutes, price in prices.items():
            due = start + minutes * 60_000
            while self.study.end + GRACE < due:
                self.close_bin(self.study.end)
            self.trade('BTC-USDT', due - 1, due, price=300)
            self.trade('BTC-USDT', due + 1, due + 2, price=price)
        labels = self.db.execute(
            "SELECT * FROM labels WHERE asset='BTC' AND factor_time=? ORDER BY minutes", (end,),
        ).fetchall()
        for row in labels:
            self.assertEqual(row['status'], 'COMPLETE')
            self.assertEqual(row['completed_time'], row['target_time'] + 2)
            self.assertAlmostEqual(row['log_return'], math.log(prices[row['minutes']] / 100))

    def test_regime_thresholds_are_from_prior_observations(self):
        end = self.study.started + DAY
        ends = range(end - MIN_HISTORY * BIN, end, BIN)
        jumps, volatility = self.study.histories['BTC']
        for stamp in ends:
            jumps.add(stamp, .02)
            volatility.add(stamp, .03)
        self.study.prices['BTC'] = deque(((end - index * BIN, 100) for index in range(90, 0, -1)),
                                        maxlen=91)
        regime, momentum, _, _, jump_cut, rv_cut = self.study.price_state('BTC', end, 101)
        self.assertEqual(regime, 'NORMAL')
        self.assertAlmostEqual(momentum, math.log(1.01))
        self.assertEqual((jump_cut, rv_cut), (.02, .03))
        regime, *_, jump_cut, rv_cut = self.study.price_state('BTC', end + BIN, 150)
        self.assertEqual(regime, 'EXTREME')
        self.assertEqual((jump_cut, rv_cut), (.02, .03))

    def test_low_volume_preserves_the_raw_score(self):
        self.baseline()
        self.study.started = self.study.end - DAY
        for market in self.study.markets.values():
            for stamp in range(self.study.end - MIN_HISTORY * BIN, self.study.end, BIN):
                market.volume.add(stamp, 1_000)
        self.close_bin(self.study.end)
        row = self.sample(8 * BIN)
        self.assertEqual(row['quality_status'], 'OK')
        self.assertEqual(row['volume_quality'], 'LOW')
        self.assertAlmostEqual(row['cdfp_value'], .2 / 6)


class ReportTests(unittest.TestCase):
    def test_nonoverlap_and_purged_walk_forward(self):
        def row(stamp, index, label_time=None):
            score = (index % 11 - 5) / 5
            end = stamp + 300_000 if label_time is None else label_time
            return dict(available_time=stamp, price_time=stamp, label_time=end, completed_time=end,
                        log_return=score / 100, cdfp_value=score, momentum=(index % 5) / 100,
                        spot_imbalance=(index % 3) / 5, perp_imbalance=(index % 7) / 7)

        early = [row(600_000 * index, index) for index in range(32)]
        late = row(DAY - 100_000, 44)
        next_day = [row(DAY + 600_000 * index, index) for index in range(1, 12)]
        overlapping = row(100_000, 0)
        selected = nonoverlap([early[0], overlapping, *early[1:], late, *next_day])
        self.assertNotIn(overlapping, selected)
        results = walk_forward(selected)
        self.assertEqual(results['count'], len(next_day))
        self.assertEqual(results['utc_day_blocks'][0]['train_count'], len(early))
        self.assertAlmostEqual(results['incremental_cdfp_mse_reduction'], 1.0)


if __name__ == '__main__':
    unittest.main()
