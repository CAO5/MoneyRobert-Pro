"""Anonymous DOGE leveraged-claim activity and scheduled pressure; never trades the claims."""
import csv
import gzip
import hashlib
import io
import math
import tempfile
import time
import urllib.request
import zipfile
from collections import defaultdict
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book
from participant_traces import replay


ROOT = Path(__file__).parent
CACHE = Path(tempfile.gettempdir()) / 'doge-reset-input'
MONTHS = tuple(f'2022{month:02}' for month in range(1, 13))
DAYS = tuple(f'2023-{month:02}-01' for month in range(1, 7))
RULES = ('reset_up', 'reset_down_rebound', 'placebo_up', 'placebo_down_rebound', 'claim_bull', 'claim_bear')
DAY = 86400


def midnight(day):
    return (date.fromisoformat(day) - date(1970, 1, 1)).days * DAY


def input_file(month, kind):
    if kind == 'bars':
        name = f'DOGEUSDT-1m-{month[:4]}-{month[4:]}.zip'
        url = f'https://s3-ap-northeast-1.amazonaws.com/data.binance.vision/data/spot/monthly/klines/DOGEUSDT/1m/{name}'
    else:
        symbol = 'DOGE_USDT' if kind == 'index' else f'DOGE3{kind}_USDT'
        name = f'{symbol}-{month}.csv.gz'
        folder = 'futures_usdt/mark_prices' if kind == 'index' else 'spot/deals'
        url = f'https://download.gatedata.org/{folder}/{month}/{name}'
    path = CACHE / name
    if not path.exists():
        CACHE.mkdir(exist_ok=True)
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=45) as response:
                    raw = response.read()
                if kind == 'bars':
                    with urllib.request.urlopen(url + '.CHECKSUM', timeout=20) as response:
                        if hashlib.sha256(raw).hexdigest() != response.read().decode().split()[0]:
                            raise ValueError(f'Archive digest mismatch: {name}')
                else:
                    gzip.decompress(raw)
                path.write_bytes(raw)
                break
            except OSError:
                if attempt == 2:
                    raise
                time.sleep(1 + attempt)
    return path


def levels(samples):
    selected = {}
    for stamp, value in samples:
        cutoff = int(stamp) // 60 * 60 + 60
        if cutoff % DAY not in (28800 - 360, 28800, 57600 - 360, 57600) or cutoff - stamp > 30:
            continue
        if not math.isfinite(value) or value <= 0:
            raise ValueError('Invalid index value')
        if cutoff not in selected or stamp > selected[cutoff][0]:
            selected[cutoff] = stamp, value
    return {cutoff: value for cutoff, (_, value) in selected.items()}


def index_month(month):
    with gzip.open(input_file(month, 'index'), 'rt') as stream:
        return levels((float(row[0]), float(row[1])) for row in csv.reader(stream))


def scheduled(ref, recent, clocks):
    result = defaultdict(dict)
    eligible = defaultdict(int)
    for clock in clocks:
        if clock - DAY not in ref or clock - 360 not in recent:
            continue
        prefix = 'reset' if clock % DAY == 57600 else 'placebo'
        eligible[prefix] += 1
        move = recent[clock - 360] / ref[clock - DAY] - 1
        if move > .01:
            result[prefix + '_up'][clock - 300] = (360, move, 0., 0.)
        if move < -.01:
            result[prefix + '_down_rebound'][clock + 60] = (600, move, 0., 0.)
    return result, eligible


def blocks(deals):
    buckets = defaultdict(lambda: [0., 0., 0])
    seen = set()
    for stamp, ident, value, buy in deals:
        if ident in seen:
            continue
        if not math.isfinite(value) or value <= 0 or buy not in (True, False):
            raise ValueError('Invalid claim print')
        seen.add(ident)
        bucket = buckets[int(stamp) // 900 * 900]
        bucket[0] += value
        bucket[1] += value if buy else 0
        bucket[2] += 1
    return buckets


def archive_buy(code):
    return {1: True, 2: False}[int(code)]


def file_blocks(month, side):
    def deals(stream):
        for row in csv.reader(stream):
            yield float(row[0]), int(row[1]), float(row[2]) * float(row[3]), archive_buy(row[4])
    with gzip.open(input_file(month, side), 'rt') as stream:
        return blocks(deals(stream))


def claims(long_blocks, short_blocks):
    result = defaultdict(dict)
    eligible = 0
    for stamp in long_blocks.keys() & short_blocks.keys():
        long_total, long_buy, long_count = long_blocks[stamp]
        short_total, short_buy, short_count = short_blocks[stamp]
        if min(long_count, short_count) < 3 or min(long_total, short_total) < 1000:
            continue
        eligible += 1
        left, right = long_buy / long_total, short_buy / short_total
        rule = 'claim_bull' if left >= .7 and right <= .3 else 'claim_bear' if left <= .3 and right >= .7 else None
        if rule:
            result[rule][stamp + 960] = (600, left, right, min(long_total, short_total))
    return result, eligible


def market_bars(month):
    result = {}
    with zipfile.ZipFile(input_file(month, 'bars')) as source:
        with source.open(source.namelist()[0]) as raw:
            for row in csv.reader(io.TextIOWrapper(raw, encoding='ascii')):
                sec = int(row[0]) // 1000
                opening, high, low, close = map(float, row[1:5])
                if sec % 60 or sec in result or not 0 < low <= min(opening, close) <= max(opening, close) <= high:
                    raise ValueError(f'Invalid minute: {sec}')
                result[sec] = (opening, high, low, close)
    return result


def coarse(bars, events, fee, start, end):
    cash = peak = 100.
    dd = 0.
    ledger = []
    missing = missed = eligible = 0
    last = start - 1
    for stamp, observation in sorted(events.items()):
        hold = observation[0]
        if not start <= stamp < end - hold - 60:
            continue
        eligible += 1
        if stamp <= last or dd >= .08:
            continue
        if stamp not in bars or stamp - 60 not in bars:
            missed += 1
            continue
        price = bars[stamp][0]
        units = math.floor(24.5 / bars[stamp - 60][3])
        debit = units * price * (1 + fee)
        if not 20 <= units * price <= 25 or cash < debit:
            missed += 1
            continue
        cash -= debit
        deadline = stamp + hold
        for sec in range(stamp, end, 60):
            if sec not in bars:
                missing += 1
                continue
            opening, high, low, _ = bars[sec]
            if sec >= deadline:
                credit = units * opening * (1 - fee)
                cash += credit
                peak = max(peak, cash)
                dd = max(dd, 1 - cash / peak)
                ledger.append((stamp, sec, units, debit, credit, credit - debit))
                last = sec
                break
            peak = max(peak, cash + units * high * (1 - fee))
            dd = max(dd, 1 - (cash + units * low * (1 - fee)) / peak)
            if dd >= .08:
                deadline = min(deadline, sec + 60)
        else:
            raise ValueError('Unclosed position across period boundary')
    if not math.isclose(cash - 100, math.fsum(row[-1] for row in ledger), abs_tol=1e-8):
        raise ValueError('Cash mismatch')
    return (eligible, missed, len(ledger), sum(row[-1] > 0 for row in ledger), cash - 100, 100 * dd, missing), ledger


def received_claims(day, side):
    def deals():
        for row in rows(day, f'DOGE3{side}_USDT', 'trades', 'gate-io'):
            if row[5] not in ('buy', 'sell'):
                raise ValueError('Unexpected aggressor')
            yield int(row[3]) / 1e6, int(row[4]), float(row[6]) * float(row[7]), row[5] == 'buy'
    return blocks(deals())


def main():
    with (ROOT / 'leveraged_claims_results.csv').open('w', newline='') as results, (ROOT / 'leveraged_claims_trades.csv').open('w', newline='') as trades, (ROOT / 'leveraged_claims_coverage.csv').open('w', newline='') as coverage:
        writer, journal = csv.writer(results), csv.writer(trades)
        audit = csv.writer(coverage)
        audit.writerow(('sample', 'period', 'eligible_reset_clocks', 'eligible_placebo_clocks', 'eligible_claim_blocks', 'underlying_minute_bars_missing', 'fresh_underlying_quote_seconds'))
        writer.writerow(('sample', 'period', 'rule', 'fee', 'signals', 'misses', 'roundtrips', 'wins', 'net_usdt', 'dd_pct', 'missing_mark_intervals'))
        journal.writerow(('sample', 'period', 'rule', 'fee', 'entry_utc', 'exit_utc', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        prior = {}
        for half in (0, 1):
            months = MONTHS[6 * half:6 * half + 6]
            bars, current = {}, {}
            valid_blocks = 0
            events = defaultdict(dict)
            for month in months:
                bars.update(market_bars(month))
                current.update(index_month(month))
                formed, valid = claims(file_blocks(month, 'L'), file_blocks(month, 'S'))
                valid_blocks += valid
                for rule, observed in formed.items():
                    events[rule].update(observed)
            start = midnight('2022-01-01' if not half else '2022-07-01')
            end = midnight('2022-07-01' if not half else '2023-01-01')
            clocks = [base + offset for base in range(start, end, DAY) for offset in (28800, 57600)]
            reset, valid_clocks = scheduled(prior | current, current, clocks)
            events.update(reset)
            period = f'2022-H{half + 1}'
            audit.writerow(('minute_proxy', period, valid_clocks['reset'], valid_clocks['placebo'], valid_blocks, (end - start) // 60 - len(bars), None))
            for rule in RULES:
                for fee in (.001, .002):
                    result, ledger = coarse(bars, events[rule], fee, start, end)
                    key = ('minute_proxy', period, rule, fee)
                    writer.writerow((*key, *result))
                    journal.writerows((*key, *row) for row in ledger)
            prior = current
            print(period, 'complete', flush=True)
        for day in DAYS:
            origin = midnight(day)
            current = levels((int(row[3]) / 1e6, float(row[9])) for row in rows(day, 'DOGE_USDT', 'derivative_ticker', 'gate-io-futures') if row[9])
            previous_month = date.fromordinal(date.fromisoformat(day).toordinal() - 1).strftime('%Y%m')
            reference = index_month(previous_month)
            events, valid_clocks = scheduled(reference, current, (origin + 28800, origin + 57600))
            claim, valid_blocks = claims(received_claims(day, 'L'), received_claims(day, 'S'))
            events.update(claim)
            quotes = book(day, 'DOGEUSDT')
            audit.writerow(('receipt_quotes', day, valid_clocks['reset'], valid_clocks['placebo'], valid_blocks, None, sum(quote is not None for quote in quotes)))
            for rule in RULES:
                signals = [(stamp - origin - 1, *params) for stamp, params in events[rule].items() if origin < stamp < origin + DAY]
                for fee in (.001, .002):
                    result, ledger = replay(quotes, signals, fee)
                    key = ('receipt_quotes', day, rule, fee)
                    writer.writerow((*key, result[0], *result[3:9]))
                    journal.writerows((*key, origin + row[5], origin + row[6], *row[7:]) for row in ledger)
            print(day, 'complete', flush=True)


if __name__ == '__main__':
    main()
