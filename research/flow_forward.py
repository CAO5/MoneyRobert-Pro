"""Audit a forward archive and replay the shared account across full UTC days."""
import argparse
import csv
import gzip
import hashlib
import json
import math
from collections import Counter, defaultdict
from dataclasses import replace
from datetime import date
from pathlib import Path
from random import Random

from flow_path import HOLD
from flow_portfolio import Account, DOGE, SHOCK, STEPS, SYMBOLS, VOL_WINDOW, discover, group_signals
from flow_sector import FEES
from sell_pressure import prefix


DAY = 86400
NS = 1_000_000_000
WARMUP = (VOL_WINDOW + 1) * SHOCK
EPOCH = date(1970, 1, 1)


def sealed_rows(root):
    with (root / 'manifest.csv').open(newline='', encoding='utf-8') as manifest:
        entries = tuple(csv.DictReader(manifest))
    for index, entry in enumerate(entries):
        path = root / f'{index:06}.jsonl.gz'
        if path.name != entry['file'] or path.stat().st_size != int(entry['bytes']):
            raise ValueError(f'Incorrect archive entry {index}')
        with path.open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != entry['sha256']:
                raise ValueError(f'Checksum failed for {path.name}')
        count = 0
        with gzip.open(path, 'rt', encoding='ascii') as stream:
            for line in stream:
                row = json.loads(line)
                if not count and row[0] != int(entry['first_utc_ns']):
                    raise ValueError(f'Incorrect segment start for {path.name}')
                count += 1
                yield row
        if count != int(entry['rows']) or row[0] != int(entry['last_utc_ns']):
            raise ValueError(f'Incorrect segment length or end for {path.name}')


class DayTape:
    def __init__(self, start):
        self.start = start
        self.quotes = {symbol: [None] * DAY for symbol in SYMBOLS}
        self.turnover = {symbol: [[0.] * DAY for _ in range(3)] for symbol in SYMBOLS}
        self.disconnects = set()
        self.observed = set()
        self.market_observed = {symbol: set() for symbol in SYMBOLS}

    def add(self, wall, kind, content):
        second = wall // NS
        index = second - self.start
        if kind in ('connect', 'disconnect'):
            self.disconnects.add(index)
            for symbol in SYMBOLS:
                self.quotes[symbol][index] = ()
                self.turnover[symbol][2][index] += 1
        if kind == 'portfolio_second':
            stamp, quotes, flows = content
            if stamp != second:
                raise ValueError('Invalid receipt second')
            self.observed.add(index)
            for symbol, quote in quotes.items():
                if quote[0] // 1_000_000 != second:
                    raise ValueError('Quote receipt is outside its recorded second')
                self.quotes[symbol][index] = tuple(quote)
                self.market_observed[symbol].add(index)
            for symbol, (sold, bought, gaps, *_ids) in flows.items():
                self.market_observed[symbol].add(index)
                for target, amount in zip(self.turnover[symbol], (sold, bought, gaps)):
                    if not math.isfinite(amount) or amount < 0:
                        raise ValueError('Invalid turnover bucket')
                    target[index] += amount

    def longest_gap(self, previous=None):
        gaps = {}
        for symbol, observed in self.market_observed.items():
            bounds = [-1, *sorted(observed), DAY]
            longest = max(right - left - 1 for left, right in zip(bounds, bounds[1:]))
            if previous is not None:
                last_observed = max(previous.market_observed[symbol], default=-1)
                longest = max(longest, DAY - last_observed - 1 + bounds[1])
            gaps[symbol] = longest
        return gaps

    def price(self, prior):
        for symbol, quotes in self.quotes.items():
            last = prior.get(symbol)
            for index, quote in enumerate(quotes):
                if quote is not None:
                    last = quote
                cutoff = (self.start + index + 1) * 1_000_000 - 2_000_000
                quotes[index] = last if (last and last[0] >= cutoff and
                                         0 < last[1] < last[2] and min(last[3:]) > 0) else None


def verify_filters(data):
    symbol, = data['symbols']
    filters = {rule['filterType']: rule for rule in symbol['filters']}
    lot = filters['LOT_SIZE']
    market = filters.get('MARKET_LOT_SIZE', lot)
    notional = filters.get('NOTIONAL', filters.get('MIN_NOTIONAL', {}))
    step = STEPS[symbol['symbol']]
    return (symbol['status'] == 'TRADING' and float(lot['stepSize']) == step
            and float(market['stepSize']) in (0., step)
            and float(notional.get('minNotional', '0')) <= 20)


def read_days(root, start, end, audit):
    current = DayTape(start)
    previous_quote = {}
    previous_wall = previous_mono = None
    last_wall = None
    for wall, mono, venue, kind, content in sealed_rows(root):
        if previous_wall is None and wall // NS > start:
            raise ValueError('The sealed archive starts after the requested start')
        if previous_mono is not None:
            if mono < previous_mono or wall < previous_wall:
                raise ValueError('Archived receipt clocks moved backwards')
            if start <= wall // NS < end:
                audit['clock_jumps'] += abs((wall - previous_wall) - (mono - previous_mono)) > 100_000_000
        previous_wall, previous_mono = wall, mono
        last_wall = wall
        second = wall // NS
        if venue == 'binance' and kind == 'filters' and second < end:
            key = content[2]['symbols'][0]['symbol']
            if key in SYMBOLS:
                audit['filters'][key] = verify_filters(content[2]) and audit['filters'].get(key, True)
        while second >= current.start + DAY and current.start < end:
            current.price(previous_quote)
            previous_quote = {symbol: tape[-1] for symbol, tape in current.quotes.items()}
            yield current
            current = DayTape(current.start + DAY)
        if start <= second < end and venue == 'binance':
            current.add(wall, kind, content)
        if second >= end:
            break
    if last_wall is None or last_wall // NS < end:
        raise ValueError('The sealed archive does not cover the requested endpoint')


def intervals(daily):
    """Resample by UTC exit day, preserving same-day trade dependence."""
    random = Random(0)
    if not daily or sum(row[2] for row in daily) == 0:
        return None
    nets, rates = [], []
    for _ in range(2000):
        draw = random.choices(daily, k=len(daily))
        nets.append(math.fsum(item[0] for item in draw))
        count = sum(item[2] for item in draw)
        if count:
            rates.append(sum(item[1] for item in draw) / count)
    def quantiles(values):
        values.sort()
        return [values[int((len(values)-1) * p)] for p in (.025, .975)]
    return {'net_usdt_95': quantiles(nets), 'win_rate_95': quantiles(rates)}


def simulate(root, start, end):
    accounts = {(variant, fee): Account(fee) for variant in ('doge_original', 'doge_scaled', 'portfolio') for fee in FEES}
    audit = {'clock_jumps': 0, 'filters': {}}
    daily = []
    all_scaled_signals = []
    previous = None
    for day in read_days(root, start, end, audit):
        original, scaled = [], []
        for symbol in SYMBOLS:
            quotes, flows = day.quotes[symbol], day.turnover[symbol]
            offset = WARMUP if previous is not None else 0
            if previous is not None:
                quotes = previous.quotes[symbol][-offset:] + quotes
                flows = [old[-offset:] + new for old, new in zip(previous.turnover[symbol], flows)]
            tail = HOLD + 70 if day.start + DAY == end else 0
            fixed, dynamic = discover(symbol, quotes, [prefix(values) for values in flows], tail=tail)
            original.extend(replace(event, sec=event.sec + day.start - offset) for event in fixed if event.sec >= offset)
            scaled.extend(replace(event, sec=event.sec + day.start - offset) for event in dynamic if event.sec >= offset)
        all_scaled_signals.extend(scaled)
        variants = ('doge_original', 'doge_scaled', 'portfolio')
        maps = {variant: defaultdict(list) for variant in variants}
        for event in original:
            maps['doge_original'][event.sec].append(event)
        for event in scaled:
            maps['portfolio'][event.sec].append(event)
            if event.symbol == DOGE:
                maps['doge_scaled'][event.sec].append(event)
        offsets = {key: len(account.trades) for key, account in accounts.items()}
        for index in range(DAY):
            second = day.start + index
            quotes = {symbol: day.quotes[symbol][index] for symbol in SYMBOLS}
            for (variant, fee), account in accounts.items():
                if account.position and index in day.disconnects:
                    account.counts['disconnects_while_holding'] += 1
                account.step(second, quotes, maps[variant].get(second))
        results = {}
        for key, account in accounts.items():
            pnl = [trade[-1] for trade in account.trades[offsets[key]:]]
            results[key] = (math.fsum(pnl), sum(value > 0 for value in pnl), len(pnl))
        daily.append({'date': str(date.fromordinal(EPOCH.toordinal() + day.start // DAY)),
                      'periods_observed': len(day.observed), 'disconnect_markers': len(day.disconnects),
                      'longest_market_gap_seconds': day.longest_gap(previous),
                      'results': results})
        previous = day
    archive_episodes = Counter()
    for group in group_signals(all_scaled_signals):
        symbols = {event.symbol for event in group}
        archive_episodes['all'] += 1
        archive_episodes['multiple_symbols'] += len(symbols) > 1
        archive_episodes['without_doge'] += DOGE not in symbols
    report = {}
    for key, account in accounts.items():
        payouts = [trade[-1] for trade in account.trades]
        net = math.fsum(payouts)
        units_open = account.position.units if account.position else 0.
        if not units_open and not math.isclose(account.cash - 100, net, abs_tol=1e-8):
            raise ValueError(f'Continuous cash reconciliation failed for {key}')
        report[key] = {'trades': len(payouts), 'wins': sum(p > 0 for p in payouts), 'net_usdt': net,
                       'drawdown_pct': account.drawdown * 100, 'unresolved': bool(units_open or account.pending),
                       'daily_resampling': intervals([row['results'][key] for row in daily]),
                       'ledger': account.trades, **account.counts}
    base, stress = (report['portfolio', fee] for fee in FEES)
    enough = len(daily) >= 30 and all(max(row['longest_market_gap_seconds'].values()) < SHOCK for row in daily)
    verified = (enough and base['trades'] >= 100 and base['wins'] * 2 > base['trades']
                and base['net_usdt'] > 0 and base['drawdown_pct'] <= 8 and stress['net_usdt'] > 0
                and not any(r['unresolved'] for r in (base, stress))
                and not any(accounts['portfolio', fee].counts['disconnects_while_holding'] for fee in FEES)
                and audit['clock_jumps'] == 0 and all(audit['filters'].get(symbol) for symbol in SYMBOLS))
    return {'continuous_days': len(daily), 'episodes': dict(archive_episodes), 'audit': audit,
            'cash_control_usdt': 0., 'numerical_gate_passed': verified,
            'trade_columns': ['symbol', 'signal_utc_sec', 'entry_utc_sec', 'exit_utc_sec',
                              'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'],
            'results': {f'{variant}/{fee}': value for (variant, fee), value in report.items()},
            'coverage': [{key: value for key, value in item.items() if key != 'results'} for item in daily]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--start', required=True, type=date.fromisoformat)
    parser.add_argument('--end', required=True, type=date.fromisoformat, help='Exclusive UTC date.')
    args = parser.parse_args()
    if args.end <= args.start:
        parser.error('The exclusive end must follow the start.')
    print(json.dumps(simulate(args.root, (args.start - EPOCH).days * DAY, (args.end - EPOCH).days * DAY), indent=2))


if __name__ == '__main__':
    main()
