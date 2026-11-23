"""Replay identical DOGE breakout logic at several delays using archived trades."""
import argparse
import csv
import hashlib
import http.client
import io
import json
import math
import tempfile
import urllib.error
import urllib.request
import zipfile
from bisect import bisect_left
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timezone
from functools import lru_cache
from pathlib import Path
from time import sleep
from urllib.parse import urlencode

from long_short_expectancy import API, BAR, COSTS, DAY, epoch, evaluate, feature_series, inputs, request_json, utc


SYMBOL = 'DOGEUSDT'
DELAYS = (1, 5, 15, 30, 60)
SOURCE = 'https://data.binance.vision/data/futures/um/daily/aggTrades'
BASELINE = Path(__file__).with_name('long_short_expectancy_results.json')
MAX_PRINT_WAIT_MS = 5_000


def download(url):
    for attempt in range(3):
        try:
            with urllib.request.urlopen(url, timeout=35) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 502, 503, 504) or attempt == 2:
                raise
            retry = error.headers.get('Retry-After', '')
            sleep(int(retry) if retry.isdecimal() else 2*(attempt+1))
        except (OSError, http.client.IncompleteRead):
            if attempt == 2:
                raise
            sleep(2*(attempt+1))


def target_times(day):
    midnight = epoch(day)
    return [(anchor+delay)*1000 for anchor in range(midnight, midnight+DAY, BAR) for delay in DELAYS]


def archive_day(day, targets):
    url = f'{SOURCE}/{SYMBOL}/{SYMBOL}-aggTrades-{day}.zip'
    expected = download(url+'.CHECKSUM').decode('ascii').split()[0]
    raw = download(url)
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError(f'Trade archive checksum mismatch on {day}')
    found = {}
    previous = 0
    midnight = epoch(day)*1000
    with zipfile.ZipFile(io.BytesIO(raw)) as zipped:
        name, = zipped.namelist()
        with io.TextIOWrapper(zipped.open(name),encoding='ascii',newline='') as text:
            rows = csv.reader(text)
            head = next(rows)
            if head[0].isdigit():
                rows = iter_with_first(head, rows)
            elif head[:6] != ['agg_trade_id','price','quantity','first_trade_id','last_trade_id','transact_time']:
                raise ValueError(f'Unexpected aggregate-trade fields on {day}: {head}')
            for row in rows:
                timestamp = int(row[5])
                if not midnight <= timestamp < midnight+DAY*1000 or timestamp < previous:
                    raise ValueError(f'Nonmonotone or out-of-window prints on {day}')
                previous = timestamp
                while len(found) < len(targets) and targets[len(found)] <= timestamp:
                    target = targets[len(found)]
                    found[target] = [timestamp, float(row[1])] if timestamp-target <= MAX_PRINT_WAIT_MS else None
                if len(found) == len(targets):
                    break
    return dict(source=url, sha256=expected, points=found)


def iter_with_first(row, rest):
    yield row
    yield from rest


def continuous_day(day, targets):
    """Use exact series timestamps if the latest daily ZIP is not published yet."""
    url = f'{API}/continuousKlines'
    found = {}
    for i in range(0, len(targets), len(DELAYS)):
        group = targets[i:i+len(DELAYS)]
        origin = group[0]-DELAYS[0]*1000
        query = urlencode(dict(pair=SYMBOL, contractType='PERPETUAL', interval='1s',
                               startTime=origin, endTime=origin+65_999, limit=66))
        raw = request_json(f'{url}?{query}')
        active = [(row[0], float(row[1])) for row in raw if int(row[8]) > 0]
        times = [t for t,_ in active]
        for target in group:
            j = bisect_left(times,target)
            found[target] = list(active[j]) if j < len(active) and times[j]-target <= MAX_PRINT_WAIT_MS else None
    return dict(source=url, points=found)


@lru_cache(maxsize=None)
def prices(day, refresh=False):
    cache = Path(tempfile.gettempdir())/'moneyrobert-doge-seconds'
    cache.mkdir(exist_ok=True)
    file = cache/f'{day}.json'
    if file.exists() and not refresh:
        return json.loads(file.read_bytes())
    targets = target_times(day)
    observed = continuous_day(day, targets) if day >= '2026-08-01' else archive_day(day, targets)
    observed['points'] = {str(t):observed['points'].get(t) for t in targets}
    if not any(observed['points'].values()):
        raise ValueError(f'No validated archive samples on {day}')
    file.write_text(json.dumps(observed),encoding='ascii')
    return observed


def recorded_execution(manifests, symbol, fill):
    if symbol != SYMBOL:
        raise ValueError(f'The stored print series only supports {SYMBOL}')
    day = utc(fill)[:10]
    if day not in manifests:
        manifests[day] = prices(day)
    observed = manifests[day]['points'].get(str(fill*1000))
    if observed is None:
        manifests[day] = prices(day,True)
        observed = manifests[day]['points'].get(str(fill*1000))
    if observed is None or not 0 <= observed[0]-fill*1000 <= MAX_PRINT_WAIT_MS or not 0 < observed[1] < math.inf:
        raise ValueError(f'Missing eligible print for {symbol} at {utc(fill)}')
    return observed[1]


def report(stop, periods=('2026','recent_30d')):
    baseline = json.loads(BASELINE.read_bytes())
    if baseline['end_utc_exclusive'] != stop:
        raise ValueError('Match the end date of the baseline research')
    ranges = {name:tuple(epoch(stamp[:10]) for stamp in baseline['range_utc'][name])
              for name in periods}
    control = baseline['scenarios']['breakout']
    days = {utc(t[key])[:10] for period in ranges for accounts in control[period].values() for name,case in accounts.items()
            if name.startswith('doge_') for t in case['ledger'] for key in ('entry','exit')
            if key == 'entry' or t['reason'] != 'window_end'}
    manifests = {}
    with ThreadPoolExecutor(10) as pool:
        for day, observed in zip(sorted(days),pool.map(prices,sorted(days))):
            manifests[day] = observed

    def execution(symbol, fill):
        return recorded_execution(manifests,symbol,fill)

    markets,limits,curve,_ = inputs(stop)
    bars, rates, _ = markets[SYMBOL]
    signals = {SYMBOL:feature_series(bars,rates,'breakout')[0]}
    cases = {}
    for name,(start,end) in ranges.items():
        cases[name] = {}
        for lane,cost in COSTS.items():
            cases[name][lane] = {str(delay):evaluate({SYMBOL:bars},{SYMBOL:rates},signals,limits,curve,start,end,
                                                     cost,(1,-1),(SYMBOL,),execution,delay_seconds=delay)
                                 for delay in DELAYS}
        for lanes in cases[name].values():
            for outcome in lanes.values():
                accepted = [t[key]*1000 for t in outcome['ledger'] for key in ('entry','exit')
                            if key=='entry' or t['reason']!='window_end']
                waits = [manifests[utc(t//1000)[:10]]['points'][str(t)][0]-t for t in accepted]
                outcome.update(max_used_wait_ms=max(waits,default=None), fills_waited_over_two_seconds=sum(t>2000 for t in waits))
    return dict(end_utc_exclusive=stop, delay_seconds=DELAYS, max_print_wait_ms=MAX_PRINT_WAIT_MS,
                note='First published trade or traded one-second open at or after the delay, up to 5 seconds later, plus identical costs. '
                     'Order timestamps and funding attribution use the scheduled boundary; no L1 fill guarantee.',
                days={day:{k:v for k,v in values.items() if k != 'points'} for day,values in sorted(manifests.items())},
                samples_sha256=hashlib.sha256(json.dumps(manifests,sort_keys=True).encode('ascii')).hexdigest(),cases=cases)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--end',type=date.fromisoformat,default=datetime.now(timezone.utc).date())
    parser.add_argument('--output',type=Path)
    parser.add_argument('--recent-only',action='store_true')
    args = parser.parse_args()
    periods = ('recent_30d',) if args.recent_only else ('2026','recent_30d')
    payload = json.dumps(report(args.end.isoformat(),periods),indent=2,allow_nan=False)+'\n'
    if args.output:
        args.output.write_text(payload,encoding='utf-8')
    else:
        print(payload,end='')
