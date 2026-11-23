"""Independent fiat FX and stablecoin convertibility screens. No order entry."""
import csv
import hashlib
import io
import math
import tempfile
import urllib.parse
import urllib.request
import zipfile
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from html.parser import HTMLParser
from pathlib import Path

from independent_round import book
from participant_traces import replay


ROOT = Path(__file__).parent
CACHE = Path(tempfile.gettempdir()) / 'doge-fx-bars'
DAYS = ('2022-11-01', '2022-12-01', '2023-02-01', '2023-03-01',
        '2023-05-01', '2023-06-01', '2023-08-01', '2023-11-01')
FX = ('usd_weak_lag', 'usd_strong_lag')
PEG = ('usdt_discount', 'usdt_premium')
EST = timezone(timedelta(hours=-5))
DIGESTS = {
    'EURUSD-2022': '5ed361f3d93107f644834cac06870e830f9dc887d5aa5fde4b90c2accdf773f3',
    'USDJPY-2022': 'd290231856453b961d110430a98e2dd6074efea881ed6ebcf3211647d50e70c7',
    'EURUSD-2023': '26e41c4bccfd70f2c76ef7f3ffcba0e3b7097231927a41640f4a7c442815f34a',
    'USDJPY-2023': 'f6b03b2b9e3f7a64d64db68d73b97902e2fb6748bfcf52dfdff9730a09173cfa',
}


class DownloadForm(HTMLParser):
    def __init__(self):
        super().__init__()
        self.selected = False
        self.fields = {}

    def handle_starttag(self, tag, attrs):
        attributes = dict(attrs)
        if tag == 'form':
            self.selected = attributes.get('id') == 'file_down'
        if self.selected and tag == 'input' and attributes.get('type') == 'hidden':
            self.fields[attributes['name']] = attributes.get('value', '')

    def handle_endtag(self, tag):
        if tag == 'form':
            self.selected = False


def archive_bytes(symbol, year):
    key = f'{symbol}-{year}'
    path = CACHE / (key + '.zip')
    if path.exists():
        raw = path.read_bytes()
    else:
        page = f'https://www.histdata.com/download-free-forex-historical-data/?/ascii/1-minute-bar-quotes/{symbol.lower()}/{year}'
        form = DownloadForm()
        with urllib.request.urlopen(page, timeout=20) as response:
            form.feed(response.read().decode())
        if not form.fields.get('tk') or form.fields.get('fxpair') != symbol:
            raise ValueError(f'Annual public download unavailable: {key}')
        request = urllib.request.Request('https://www.histdata.com/get.php', data=urllib.parse.urlencode(form.fields).encode(), headers={'Referer': page})
        with urllib.request.urlopen(request, timeout=45) as response:
            raw = response.read()
    if hashlib.sha256(raw).hexdigest() != DIGESTS[key]:
        raise ValueError(f'Unrecognised archive {path}')
    if not path.exists():
        CACHE.mkdir(exist_ok=True)
        path.write_bytes(raw)
    return raw


def fx_bars():
    wanted = {day.replace('-', '') for day in DAYS}
    output = defaultdict(dict)
    for key in DIGESTS:
        symbol, year = key.split('-')
        raw = archive_bytes(symbol, year)
        with zipfile.ZipFile(io.BytesIO(raw)) as source:
            with source.open(f'DAT_ASCII_{symbol}_M1_{year}.csv') as stream:
                for line in stream:
                    if line[:8].decode() not in wanted:
                        continue
                    stamp, opening, high, low, close, _ = line.decode().strip().split(';')
                    stamp = datetime.strptime(stamp, '%Y%m%d %H%M%S').replace(tzinfo=EST).astimezone(timezone.utc)
                    day = stamp.date().isoformat()
                    if day not in DAYS or not 6 <= stamp.hour < 22:
                        continue
                    minute = 60 * stamp.hour + stamp.minute
                    opening, high, low, close = map(float, (opening, high, low, close))
                    bars = output[day, symbol]
                    if stamp.second != 0 or minute in bars or not 0 < low <= min(close, opening) <= max(close, opening) <= high:
                        raise ValueError(f'Invalid bar {key} {stamp}')
                    bars[minute] = close
    return output


def fx_events(euro, yen, doge):
    events = defaultdict(list)
    valid = 0
    for minute in range(7 * 60, 21 * 60):
        if not all(before in euro and before in yen for before in range(minute - 5, minute + 1)):
            continue
        sec = (minute + 2) * 60 - 1
        earlier, now = doge[sec - 300], doge[sec]
        if not earlier or not now:
            continue
        valid += 1
        euro_move = math.log(euro[minute] / euro[minute - 5])
        yen_move = math.log(yen[minute - 5] / yen[minute])
        local_move = math.log((now[1] + now[2]) / (earlier[1] + earlier[2]))
        if min(euro_move, yen_move) >= .0005 and local_move <= 0:
            rule = FX[0]
        elif max(euro_move, yen_move) <= -.0005 and local_move >= 0:
            rule = FX[1]
        else:
            continue
        events[rule].append((sec, 900, euro_move, yen_move, local_move))
    return events, valid


def peg_events(first, second, reference, local):
    events = defaultdict(list)
    valid = 0
    for sec in range(59, len(local), 60):
        home, away, other = first[sec], second[sec], reference[sec]
        if not (home and away and other and local[sec]) or not .999 <= other[1] < other[2] <= 1.001:
            continue
        valid += 1
        highest_ask = max(home[2], away[2])
        lowest_bid = min(home[1], away[1])
        if highest_ask <= min(.9995, .999 * other[1]):
            rule = PEG[0]
        elif lowest_bid >= max(1.0005, 1.001 * other[2]):
            rule = PEG[1]
        else:
            continue
        events[rule].append((sec, 900, lowest_bid, highest_ask, (other[1] + other[2]) / 2))
    return events, valid


def fiat_equivalent(ledger, tether):
    if not ledger:
        return None, None
    final_cash = 100 + math.fsum(row[-1] for row in ledger)
    for sec in range(len(tether) - 1, len(tether) - 61, -1):
        quote = tether[sec]
        if sec >= ledger[-1][6] and quote and quote[3] >= max(100, final_cash):
            return sec, (final_cash - 100) * quote[1]
    return None, None


def main():
    bars = fx_bars()
    with (ROOT / 'numeraire_shocks_results.csv').open('w', newline='') as result_file, (ROOT / 'numeraire_shocks_trades.csv').open('w', newline='') as trade_file:
        writer, journal = csv.writer(result_file), csv.writer(trade_file)
        writer.writerow(('day', 'rule', 'fee', 'eligible_minutes', 'raw_signals', 'signal_seconds', 'attempts', 'misses', 'roundtrips', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds', 'usd_mark_second', 'usd_vs_same_cash_at_end'))
        journal.writerow(('day', 'rule', 'fee', 'signal_second', 'hold', 'indicator_a', 'indicator_b', 'indicator_c', 'entry_second', 'exit_second', 'units', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        for day in DAYS:
            local = book(day, 'DOGEUSDT')
            tether = book(day, 'USDT-USD', 'kraken')
            forex, fx_valid = fx_events(bars[day, 'EURUSD'], bars[day, 'USDJPY'], local)
            stable, peg_valid = peg_events(tether, book(day, 'USDT-USD', 'coinbase'), book(day, 'USDC-USD', 'kraken'), local)
            for rule in FX + PEG:
                events, valid = (forex, fx_valid) if rule in FX else (stable, peg_valid)
                for fee in (.001, .002):
                    result, ledger = replay(local, events[rule], fee)
                    writer.writerow((day, rule, fee, valid, *result, *fiat_equivalent(ledger, tether)))
                    journal.writerows((day, rule, fee, *row) for row in ledger)
            print(day, fx_valid, peg_valid, flush=True)


if __name__ == '__main__':
    main()
