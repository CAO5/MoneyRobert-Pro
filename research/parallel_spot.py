"""Parallel historical screens for quote recovery and market-factor disagreement."""
import argparse
import csv
import math
import time
from collections import defaultdict, deque
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor, as_completed
from datetime import date
from pathlib import Path
from statistics import median

from asymmetric_risk import DAYS as RISK_DAYS, CO_MOVEMENT, SEMIVARIANCE, aligned_events, variance_events
from depth_shape import DAYS as SHAPE_DAYS, CONCENTRATION, IMBALANCE, signals as depth_signals, snapshots
from derivative_pressure import (DAYS as CONTRACT_DAYS, INVENTORY_RULES, INVERSE_RULES,
                                 RECEIPT_DAYS, RECEIPT_RULES, inventory_events, inverse_events,
                                 observed_interest, receipt_events, received_interest)
from execution_anchors import DAYS as ANCHOR_DAYS, SCALE_RULES, VWAP_RULES, anchors, scale_events
from external_clocks import CLOCK_DAYS, CLOCK_RULES, GRID_DAYS, equities_clock, quote_grid
from flow_path import DAYS as PATH_DAYS, FLOW_RULES, JUMP_RULES, flow_path, jumps
from hf_pressure import CACHE, source
from independent_round import book
from liquidity_revisions import DAYS as REVISION_DAYS, funding, repriced, spot_trades, withdrawn
from microstructure_runs import DAYS as MICRO_DAYS, BOOK_RULES, RUN_RULES, aggressor_runs, order_flow
from participant_traces import replay
from trade_transmission import DAYS as TRANSMISSION_DAYS, PRINT_RULES, USD_RULES, dollar_events, dollar_volume, print_recovery


ROOT = Path(__file__).parent
DAYS = ('2024-08-01', '2024-09-01', '2024-12-01', '2026-09-01', '2026-10-01', '2026-11-01')
HOLD_SECONDS = 600
MARKETS = {
    'quote_recovery': (('DOGEUSDT', 'bybit-spot', 'quotes'),),
    'market_residual': (('BTCUSDT', 'binance', 'quotes'), ('LTCUSDT', 'binance', 'quotes')),
    'depth_withdrawal': (('DOGEUSDT', 'binance', 'trades'),),
    'funding_revision': (('DOGEUSDT', 'binance-futures', 'derivative_ticker'),),
    'btc_quote_grid': (('DOGEBTC', 'binance', 'quotes'), ('BTCUSDT', 'binance', 'quotes')),
    'equity_clock': (),
    'flow_path': (('DOGEUSDT', 'binance', 'trades'),),
    'jump_path': (),
    'order_flow': (),
    'aggressor_runs': (('DOGEUSDT', 'binance', 'trades'),),
    'volume_anchor': (('DOGEUSDT', 'binance', 'trades'),),
    'scale_pullback': (),
    'contract_unwind': (),
    'inverse_basis': (('DOGEUSDT', 'binance-futures', 'quotes'), ('DOGEUSD_PERP', 'binance-delivery', 'quotes'), ('USDCUSDT', 'binance', 'quotes')),
    'joint_unwind': (('DOGEUSDT', 'bybit', 'derivative_ticker'),),
    'dollar_lead': (('DOGE-USD', 'coinbase', 'quotes'), ('DOGE-USD', 'coinbase', 'trades')),
    'large_print': (('DOGEUSDT', 'binance', 'trades'),),
    'standing_depth': (('DOGEUSDT', 'binance', 'book_snapshot_5'),),
    'depth_concentration': (('DOGEUSDT', 'binance', 'book_snapshot_5'),),
    'downside_coupling': (('BTCUSDT', 'binance', 'quotes'),),
    'downside_variance': (),
}
FAMILIES = {
    'quote_recovery': ('spread_stress', 'local_recovers', 'both_recover'),
    'market_residual': ('negative_residual', 'positive_residual', 'market_up_control'),
    'depth_withdrawal': ('ask_withdrawn', 'bid_withdrawn', 'ask_consumed'),
    'funding_revision': ('higher_funding', 'lower_funding', 'sign_flip'),
    'btc_quote_grid': ('up_confirmed', 'up_cash_lags', 'down_cash_lags'),
    'equity_clock': CLOCK_RULES,
    'flow_path': FLOW_RULES,
    'jump_path': JUMP_RULES,
    'order_flow': BOOK_RULES,
    'aggressor_runs': RUN_RULES,
    'volume_anchor': VWAP_RULES,
    'scale_pullback': SCALE_RULES,
    'contract_unwind': INVENTORY_RULES,
    'inverse_basis': INVERSE_RULES,
    'joint_unwind': RECEIPT_RULES,
    'dollar_lead': USD_RULES,
    'large_print': PRINT_RULES,
    'standing_depth': IMBALANCE,
    'depth_concentration': CONCENTRATION,
    'downside_coupling': CO_MOVEMENT,
    'downside_variance': SEMIVARIANCE,
}
FAMILY_DAYS = {'quote_recovery': DAYS, 'market_residual': DAYS,
               'depth_withdrawal': REVISION_DAYS, 'funding_revision': REVISION_DAYS,
               'btc_quote_grid': GRID_DAYS, 'equity_clock': CLOCK_DAYS,
               'flow_path': PATH_DAYS, 'jump_path': PATH_DAYS,
               'order_flow': MICRO_DAYS, 'aggressor_runs': MICRO_DAYS,
               'volume_anchor': ANCHOR_DAYS, 'scale_pullback': ANCHOR_DAYS,
               'contract_unwind': CONTRACT_DAYS, 'inverse_basis': CONTRACT_DAYS,
               'joint_unwind': RECEIPT_DAYS,
               'dollar_lead': TRANSMISSION_DAYS, 'large_print': TRANSMISSION_DAYS,
               'standing_depth': SHAPE_DAYS, 'depth_concentration': SHAPE_DAYS,
               'downside_coupling': RISK_DAYS, 'downside_variance': RISK_DAYS}


def midpoint(quote):
    return (quote[1] + quote[2]) / 2


def spread_bps(quote):
    return 10000 * (quote[2] - quote[1]) / midpoint(quote)


def quote_recovery(home, away):
    signals = defaultdict(list)
    samples = ([spread_bps(quote) if quote else None for quote in home],
               [spread_bps(quote) if quote else None for quote in away])
    available = 120
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for sec in range(120, len(home) - HOLD_SECONDS - 130, 5):
        if sec < available or not home[sec] or not away[sec]:
            continue
        histories = [[width for width in series[sec - 120:sec - 10] if width is not None] for series in samples]
        if min(map(len, histories)) < 80:
            continue
        counts['eligible'] += 1
        typical = [median(history) for history in histories]
        stressed = [sum(width is not None and width >= max(base * 2.5, base + 1)
                        for width in series[sec - 9:sec + 1]) >= 8
                    for series, base in zip(samples, typical)]
        if not stressed[0]:
            continue
        counts['shocks'] += 1
        counts['paired'] += stressed[1]
        signals['spread_stress'].append((sec, HOLD_SECONDS, *typical, float(stressed[1])))
        available = sec + HOLD_SECONDS + 60
        later = sec + 60
        settled = [sum(width is not None and width <= base * 1.25
                       for width in series[later - 9:later + 1]) >= 8
                   for series, base in zip(samples, typical)]
        if not home[later] or not away[later]:
            continue
        counts['observed_recovery'] += 1
        if settled[0]:
            signals['local_recovers'].append((later, HOLD_SECONDS, *typical, float(stressed[1])))
        if all(stressed) and all(settled):
            signals['both_recover'].append((later, HOLD_SECONDS, *typical, 1.))
    return signals, counts


def slope(history):
    if len(history) < 100:
        return None
    total_market = math.fsum(market for market, _ in history)
    total_doge = math.fsum(doge for _, doge in history)
    market_mean, doge_mean = total_market / len(history), total_doge / len(history)
    variance = math.fsum((market - market_mean) ** 2 for market, _ in history)
    if not variance:
        return None
    return math.fsum((market - market_mean) * (doge - doge_mean) for market, doge in history) / variance


def market_residual(home, bitcoin, litecoin):
    points = [tuple(math.log(midpoint(quote)) for quote in triple) if all(triple) else None
              for triple in zip(home[59::60], bitcoin[59::60], litecoin[59::60])]
    history = deque()
    signals = defaultdict(list)
    counts = dict(eligible=0, shocks=0, paired=0, observed_recovery=0)
    for minute in range(6, len(points) - 12):
        training_end = minute - 5
        older, newer = points[training_end - 1:training_end + 1]
        if older and newer:
            changes = [after - before for before, after in zip(older, newer)]
            history.append((training_end, (changes[1] + changes[2]) / 2, changes[0]))
        while history and history[0][0] <= training_end - 120:
            history.popleft()
        if minute % 5 or not points[minute] or not newer:
            continue
        beta = slope([(market, doge) for _, market, doge in history])
        if beta is None or not 0 < beta <= 4:
            continue
        observed, btc, ltc = (recent - old for recent, old in zip(points[minute], newer))
        market = (btc + ltc) / 2
        residual = observed - beta * market
        counts['eligible'] += 1
        sec = minute * 60 + 59
        record = (sec, HOLD_SECONDS, residual, market, beta)
        if min(btc, ltc) >= 0 and market >= .001:
            signals['market_up_control'].append(record)
        if min(btc, ltc) >= 0 and residual <= -.005:
            signals['negative_residual'].append(record)
        if max(btc, ltc) <= 0 and residual >= .005:
            signals['positive_residual'].append(record)
    return signals, counts


def screen(task):
    families, day = task
    home = book(day, 'DOGEUSDT')
    depth = snapshots(day) if any(family in ('standing_depth', 'depth_concentration') for family in families) else None
    return {family: screen_family(family, day, home, depth) for family in families}


def screen_family(family, day, home, depth):
    if family == 'quote_recovery':
        events, counts = quote_recovery(home, book(day, 'DOGEUSDT', 'bybit-spot'))
    elif family == 'market_residual':
        events, counts = market_residual(home, book(day, 'BTCUSDT'), book(day, 'LTCUSDT'))
    elif family == 'depth_withdrawal':
        events, counts = withdrawn(home, *spot_trades(day))
    elif family == 'funding_revision':
        events, counts = repriced(home, funding(day))
    elif family == 'btc_quote_grid':
        events, counts = quote_grid(home, book(day, 'BTCUSDT'), book(day, 'DOGEBTC'))
    elif family == 'equity_clock':
        events, counts = equities_clock(day, home)
    elif family == 'flow_path':
        events, counts = flow_path(day, home)
    elif family == 'jump_path':
        events, counts = jumps(home)
    elif family == 'order_flow':
        events, counts = order_flow(day, home)
    elif family == 'aggressor_runs':
        events, counts = aggressor_runs(day, home)
    elif family == 'volume_anchor':
        events, counts = anchors(day, home)
    elif family == 'scale_pullback':
        events, counts = scale_events(home)
    elif family == 'contract_unwind':
        events, counts = inventory_events(home, observed_interest(day))
    elif family == 'inverse_basis':
        events, counts = inverse_events(home, book(day, 'DOGEUSDT', 'binance-futures'),
                                        book(day, 'DOGEUSD_PERP', 'binance-delivery'), book(day, 'USDCUSDT'))
    elif family == 'joint_unwind':
        events, counts = receipt_events(home, received_interest(day), observed_interest(day))
    elif family == 'dollar_lead':
        events, counts = dollar_events(home, book(day, 'DOGE-USD', 'coinbase'), dollar_volume(day))
    elif family == 'large_print':
        events, counts = print_recovery(day, home)
    elif family == 'downside_coupling':
        events, counts = aligned_events(home, book(day, 'BTCUSDT'))
    elif family == 'downside_variance':
        events, counts = variance_events(home)
    else:
        events, counts = depth_signals(home, depth, family)
    summary, journal = [], []
    for rule in FAMILIES[family]:
        for fee in (.001, .002):
            result, ledger = replay(home, events[rule], fee)
            key = (day, rule, fee)
            summary.append((*key, *counts.values(), *result))
            journal.extend((*key, *row) for row in ledger)
    return summary, journal


def write_family(family, blocks, days, suffix):
    summary, trades = zip(*(blocks[day] for day in days))
    with (ROOT / f'{family}{suffix}_results.csv').open('w', newline='') as results, (ROOT / f'{family}{suffix}_trades.csv').open('w', newline='') as ledger:
        writer, journal = csv.writer(results), csv.writer(ledger)
        writer.writerow(('day', 'rule', 'fee', 'eligible', 'shocks', 'paired_shocks', 'observable_after',
                         'raw_signals', 'signal_seconds', 'attempts', 'misses', 'trades', 'wins', 'net_usdt',
                         'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('day', 'rule', 'fee', 'signal_second', 'hold', 'value_a', 'value_b', 'value_c',
                          'entry_second', 'exit_second', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        for group in summary:
            writer.writerows(group)
        for group in trades:
            journal.writerows(group)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--workers', type=int, default=4, choices=range(1, 7))
    parser.add_argument('--family', action='append', choices=FAMILIES)
    parser.add_argument('--days', nargs='+', type=date.fromisoformat)
    parser.add_argument('--batch', default='')
    options = parser.parse_args()
    if bool(options.days) != bool(options.batch) or options.batch and not options.batch.isascii():
        parser.error('Custom days require an ASCII batch name.')
    if options.batch and not options.batch.replace('_', '').isalnum():
        parser.error('Use only letters, numbers, and underscores in the batch name.')
    CACHE.mkdir(exist_ok=True)
    families = tuple(dict.fromkeys(options.family or ('quote_recovery', 'market_residual')))
    custom = tuple(day.isoformat() for day in sorted(set(options.days or ())))
    calendar = {family: custom or FAMILY_DAYS[family] for family in families}
    suffix = '_' + options.batch if options.batch else ''
    collected = defaultdict(dict)
    errors = []
    started = time.perf_counter()
    with ThreadPoolExecutor(6) as downloads, ProcessPoolExecutor(options.workers) as workers:
        inputs = {}
        all_days = sorted({day for family in families for day in calendar[family]})
        dependencies = {}
        for day in all_days:
            for family in families:
                if day not in calendar[family]:
                    continue
                market_inputs = (('DOGEUSDT', 'binance', 'quotes'), *MARKETS[family])
                keys = [(day, symbol, venue, kind) for symbol, venue, kind in market_inputs]
                for key in keys:
                    if key not in inputs:
                        date_key, symbol, venue, kind = key
                        inputs[key] = downloads.submit(source, (date_key, symbol, kind), venue)
                dependencies[family, day] = keys
        available = defaultdict(list)
        for (family, day), keys in dependencies.items():
            try:
                for key in keys:
                    inputs[key].result()
            except Exception as error:
                errors.append((family, day, error))
                continue
            available[day].append(family)
        jobs = {}
        for day, day_families in available.items():
            shared_depth = tuple(family for family in day_families if family in ('standing_depth', 'depth_concentration'))
            bundles = [(family,) for family in day_families if family not in shared_depth]
            if shared_depth:
                bundles.append(shared_depth)
            for bundle in bundles:
                jobs[workers.submit(screen, (bundle, day))] = (bundle, day)
        for job in as_completed(jobs):
            bundle, day = jobs[job]
            try:
                result = job.result()
            except Exception as error:
                errors.extend((family, day, error) for family in bundle)
                continue
            for family, (summary, trades) in result.items():
                collected[family][day] = summary, trades
                print(family, day, len(trades), flush=True)
                if len(collected[family]) == len(calendar[family]):
                    write_family(family, collected[family], calendar[family], suffix)
                    print(family, 'saved', flush=True)
    print(f'{time.perf_counter() - started:.2f}s using {options.workers} replay workers', flush=True)
    if errors:
        for family, day, error in errors:
            print(f'{family} {day}: {error!r}', flush=True)
        raise SystemExit(1)


if __name__ == '__main__':
    main()
