"""Finite funding budget vs turnover, and a read-only paired settlement replay."""
import csv
import io
import json
import math
import tempfile
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

from independent_round import book
from position_divergence import archive


ROOT = Path(__file__).parent
ASSETS = ('DOGEUSDT', 'BTCUSDT', 'ETHUSDT', 'SOLUSDT')
DAYS = tuple(date(2026, month, 1) for month in range(1, 5))
FEES = (.001, .002)
RULES = ('clock', 'prior_positive', 'prior_covers_four_stress_fills')


def midnight(day):
    return int(datetime(day.year, day.month, day.day, tzinfo=timezone.utc).timestamp())


def funding_month(symbol, period):
    name = f'futures/um/monthly/fundingRate/{symbol}/{symbol}-fundingRate-{period}.zip'
    records = {}
    for row in csv.DictReader(io.StringIO(archive(name))):
        timestamp = int(row['calc_time'])
        rate = float(row['last_funding_rate'])
        if timestamp in records or not math.isfinite(rate):
            raise ValueError(f'Duplicate or invalid funding in {name}')
        records[timestamp] = (rate, int(row['funding_interval_hours']))
    return records


def budget():
    start = midnight(date(2022, 1, 1))
    finish = midnight(date(2023, 1, 1))
    series = {}
    with ThreadPoolExecutor(8) as pool:
        for asset in ASSETS:
            months = [f'2022-{month:02}' for month in range(1, 13)]
            raw = [item for part in pool.map(lambda period: funding_month(asset, period), months) for item in part.items()]
            records = sorted((stamp // 1000, rate, hours * 3600) for stamp, (rate, hours) in raw)
            if (records[0][0] != start or records[-1][0] + records[-1][2] != finish or
                    any(stamp % 3600 or interval <= 0 for stamp, _, interval in records) or
                    any(later - earlier not in (first, second) for (earlier, _, first), (later, _, second) in zip(records, records[1:]))):
                raise ValueError(f'Missing, duplicated or off-schedule settlement: {asset}')
            series[asset] = {stamp: max(0., rate) for stamp, rate, _ in records}
    union = sorted(set().union(*series.values()))
    series['best_each_settlement_oracle'] = {stamp: max(rates.get(stamp, 0.) for rates in series.values()) for stamp in union}
    with (ROOT / 'funding_budget_windows.csv').open('w', newline='') as stream:
        writer = csv.writer(stream)
        writer.writerow(('asset', 'start_utc', 'days', 'events', 'max_positive_receipts_usdt',
                         'min_entry_fees_for_100_base', 'min_entry_fees_for_100_stress'))
        for asset, rates in series.items():
            for offset in range(365 - 30 + 1):
                begin = start + offset * 86400
                window = [rate for stamp, rate in rates.items() if begin <= stamp < begin + 30 * 86400]
                writer.writerow((asset, date(2022, 1, 1) + timedelta(days=offset), 30, len(window),
                                 round(25 * math.fsum(window), 9), 100 * 2 * 20 * FEES[0],
                                 100 * 2 * 20 * FEES[1]))


def settlements(day):
    lower = midnight(day - timedelta(days=1)) * 1000
    upper = midnight(day + timedelta(days=1)) * 1000 - 1
    path = Path(tempfile.gettempdir()) / 'doge-position-screen' / f'funding-rest-mark-{day}.json'
    if path.exists():
        data = json.loads(path.read_bytes())
    else:
        url = f'https://fapi.binance.com/fapi/v1/fundingRate?symbol=DOGEUSDT&startTime={lower}&endTime={upper}&limit=1000'
        with urllib.request.urlopen(url, timeout=30) as response:
            data = json.load(response)
        path.parent.mkdir(exist_ok=True)
    events = [(int(row['fundingTime']), float(row['fundingRate']), float(row['markPrice'])) for row in data]
    expected = list(range(lower // 1000, upper // 1000 + 1, 8 * 3600))
    if ([stamp // 1000 for stamp, _, _ in events] != expected or
            any(not math.isfinite(rate) or not math.isfinite(mark) or mark <= 0 for _, rate, mark in events) or
            any(row['symbol'] != 'DOGEUSDT' for row in data)):
        raise ValueError(f'Unexpected or incomplete history: {day}')
    archived = funding_month('DOGEUSDT', str(day)[:7])
    if any(stamp not in archived or archived[stamp][0] != rate for stamp, rate, _ in events if stamp >= midnight(day) * 1000):
        raise ValueError(f'Funding rate mismatch: {day}')
    if not path.exists():
        path.write_text(json.dumps(data), encoding='utf8')
    return events


def replay(day, spot, future, events, rule, fee):
    origin = midnight(day)
    times = {stamp // 1000 - origin: (rate, mark) for stamp, rate, mark in events}
    prior = sorted(sec for sec in times if sec < 0)
    planned = prior[-1] + prior[-1] - prior[-2]
    spot_cash = fut_cash = 50.
    peak = 100.
    dd = 0.
    spot_qty = hedge_qty = wanted = 0
    hedge_entry = funding = paid = before = 0.
    state = 'idle'
    start = target = -1
    missed = unpriced = attempts = 0
    ledger = []
    for sec in range(len(spot)):
        local, perp = spot[sec], future[sec]
        if sec in times:
            rate, mark = times[sec]
            credit = hedge_qty * rate * mark
            funding += credit
            fut_cash += credit
            planned = sec + sec - prior[-1]
            prior.append(sec)
        if state == 'holding' and (sec >= target + 60 or dd >= .08):
            state = 'close_hedge'
        if state == 'buy_spot':
            if local and local[4] >= wanted and 20 <= wanted * local[2] <= 25 and spot_cash >= wanted * local[2] * (1 + fee):
                spot_qty = wanted
                spot_cash -= spot_qty * local[2] * (1 + fee)
                paid += spot_qty * local[2] * fee
                state = 'sell_hedge'
            else:
                missed += 1
                state = 'idle'
        elif state == 'sell_hedge':
            if perp and perp[3] >= wanted and 20 <= wanted * perp[1] <= 25 and fut_cash >= wanted * perp[1] * (1 + fee):
                hedge_entry, hedge_qty = perp[1], wanted
                fut_cash -= hedge_qty * hedge_entry * fee
                paid += hedge_qty * hedge_entry * fee
                state = 'holding'
            else:
                missed += 1
                state = 'sell_orphan'
        elif state == 'close_hedge' and perp and perp[4] >= hedge_qty:
            fut_cash += hedge_qty * (hedge_entry - perp[2] * (1 + fee))
            paid += hedge_qty * perp[2] * fee
            hedge_qty = 0
            state = 'sell_spot'
        elif state in ('sell_spot', 'sell_orphan') and local and local[3] >= spot_qty:
            spot_cash += spot_qty * local[1] * (1 - fee)
            paid += spot_qty * local[1] * fee
            net = spot_cash + fut_cash - before
            ledger.append((start, sec, state == 'sell_spot', funding, paid, net - funding + paid, net))
            spot_qty = 0
            state = 'idle'
        if (not spot_qty or local) and (not hedge_qty or perp):
            equity = spot_cash + fut_cash
            if spot_qty:
                equity += spot_qty * local[1] * (1 - fee)
            if hedge_qty:
                equity += hedge_qty * (hedge_entry - perp[2] * (1 + fee))
            peak = max(peak, equity)
            dd = max(dd, 1 - equity / peak)
        else:
            unpriced += 1
        if state == 'idle' and sec == planned - 61 and planned + 62 < len(spot) and dd < .08:
            last_rate = times[prior[-1]][0]
            if rule == 'clock' or (rule == 'prior_positive' and last_rate > 0) or (rule == RULES[2] and last_rate > .008):
                attempts += 1
                if local and perp:
                    wanted = math.floor(24.5 / max(local[2], perp[1]))
                    start, target, before = sec + 1, planned, spot_cash + fut_cash
                    funding = paid = 0.
                    state = 'buy_spot'
                else:
                    missed += 1
    if spot_qty or hedge_qty or state != 'idle':
        raise ValueError('Unclosed exposure; cannot certify sample')
    net = spot_cash + fut_cash - 100
    if not math.isclose(net, math.fsum(row[-1] for row in ledger), abs_tol=1e-8):
        raise ValueError('Account reconciliation failed')
    complete = [row for row in ledger if row[2]]
    return (attempts, len(complete), sum(row[-1] > 0 for row in complete), missed, len(ledger) - len(complete),
            sum(row[3] for row in ledger), sum(row[4] for row in ledger), net, dd * 100, unpriced), ledger


def main():
    budget()
    with (ROOT / 'funding_capture_results.csv').open('w', newline='') as results, (ROOT / 'funding_capture_trades.csv').open('w', newline='') as trades:
        writer, journal = csv.writer(results), csv.writer(trades)
        writer.writerow(('date', 'rule', 'fee', 'attempts', 'roundtrips', 'wins', 'misses', 'orphan_sales', 'funding_usdt', 'fees_usdt', 'net_usdt', 'dd_pct', 'unpriced_seconds'))
        journal.writerow(('date', 'rule', 'fee', 'entry_second', 'exit_second', 'paired', 'funding_usdt', 'fees_usdt', 'gross_basis_usdt', 'net_usdt'))
        for day in DAYS:
            events = settlements(day)
            spot = book(str(day), 'DOGEUSDT')
            future = book(str(day), 'DOGEUSDT', 'binance-futures')
            for rule in RULES:
                for fee in FEES:
                    result, ledger = replay(day, spot, future, events, rule, fee)
                    writer.writerow((day, rule, fee, *result))
                    journal.writerows((day, rule, fee, *row) for row in ledger)
            print(day, 'settlement replay finished', flush=True)


if __name__ == '__main__':
    main()
