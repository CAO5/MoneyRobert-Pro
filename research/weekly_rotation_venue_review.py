"""Replay the previously fixed BTC/ETH rules on a second public spot venue."""
import hashlib
import http.client
import json
import math
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, Bar, epoch, load, rotation_decisions, simulate, utc
from weekly_rotation_entry_review import entry_rules
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


MARKETS = dict(zip(SYMBOLS, ('btcusd', 'ethusd')))
WINDOWS = [('2017-10-01','2018-01-01'), ('2018-01-01','2020-03-01'),
           ('2020-01-01','2026-11-01'), ('2022-01-01','2026-11-01'), ('2026-01-01','2026-11-01')]
ENDPOINT = 'https://www.bitstamp.net/api/v2/ohlc'


def fetch(root, key):
    market, start, stop = key
    path = root / f'{market}-{start}-{stop}.json'
    limit = (stop-start)//BAR
    query = urllib.parse.urlencode(dict(step=BAR, limit=limit, end=stop-BAR))
    url = f'{ENDPOINT}/{market}/?{query}'
    if path.exists():
        payload = path.read_bytes()
    else:
        for retry in range(3):
            try:
                with urllib.request.urlopen(url, timeout=20) as response:
                    payload = response.read()
                break
            except urllib.error.HTTPError as exc:
                if retry==2 or exc.code not in (429, 500, 502, 503, 504):
                    raise
                delay = exc.headers.get('Retry-After', '')
                time.sleep(max(2*(retry+1), int(delay) if delay.isdecimal() else 0))
            except (OSError, http.client.IncompleteRead):
                if retry==2:
                    raise
                time.sleep(2*(retry+1))
    data = json.loads(payload)['data']
    if data['pair'].replace('/','').lower()!=market:
        raise ValueError(f'Wrong market in {path.name}')
    bars = []
    for row in data['ohlc']:
        stamp = int(row['timestamp'])
        if not start <= stamp < stop:
            continue
        bar = Bar(stamp, *(float(row[k]) for k in ('open','high','low','close')))
        if (stamp%BAR or not all(math.isfinite(p) for p in bar[1:]) or
                not 0<bar.low<=min(bar.open,bar.close)<=max(bar.open,bar.close)<=bar.high):
            raise ValueError(f'Invalid OHLC in {path.name}')
        bars.append(bar)
    times = [b.time for b in bars]
    if not bars or times != sorted(set(times)) or len(bars)>limit:
        raise ValueError(f'Empty, unordered or duplicate bars in {path.name}')
    if not path.exists():
        path.write_bytes(payload)
    return bars, dict(file=path.name,url=url,sha256=hashlib.sha256(payload).hexdigest(),
                      first_utc=utc(start),end_utc_exclusive=utc(stop),rows=len(bars))


def load_bitstamp():
    directory = Path(tempfile.gettempdir())/'moneyrobert-bitstamp-4h'
    directory.mkdir(exist_ok=True)
    first, stop = epoch('2017-09-01'), epoch('2026-11-01')
    stride = 768*BAR
    keys = [(market,start,min(start+stride,stop)) for market in MARKETS.values()
            for start in range(first,stop,stride)]
    tapes = {market:{} for market in MARKETS.values()}
    manifest = []
    with ThreadPoolExecutor(4) as pool:
        requests = pool.map(lambda key: fetch(directory,key),keys)
        for (market,_,_), (bars,item) in zip(keys,requests):
            manifest.append(item)
            tapes[market].update((b.time,b) for b in bars)
    expected = range(first,stop,BAR)
    missing = {market:[utc(t) for t in expected if t not in tape] for market,tape in tapes.items()}
    common = set.intersection(*(set(series) for series in tapes.values()))
    result = {symbol:[tapes[market][t] if t in common else Bar(t,None,None,None,None)
                      for t in expected] for symbol,market in MARKETS.items()}
    return result, dict(cache=str(directory),scheduled_bars_per_symbol=len(expected),
                        source_gaps=missing,manifest=manifest)


def review(bars, windows):
    weekly = rotation_decisions(bars)
    reclaim = entry_rules(bars,weekly)['pullback_reclaim']
    adds = {i:picks[0] for i,(picks,_) in reclaim.items() if picks}
    half = dict(profit_protection=True, entry_target=12.25, minimum_order_notional=10.)
    cases = {'original':{}, 'half':half, 'staged':{**half,'add_signals':adds},
             'early_profit_lock':{**half,'net_profit_lock':(.03,.015)}}
    signals = {'rotation':weekly}
    return {
        start+'/'+end: {
            name: {label:annotate(simulate(bars,signals,'rotation',start,end,cost,**params))
                   for label,cost in [('base',BASE_COST),('stress',STRESS_COST)]}
            for name,params in cases.items()
        }
        for start,end in windows
    }


def drawdown_witness(bars, account):
    """Only use observed equity marks with known temporal ordering."""
    fee, slip = BASE_COST
    multiplier = (1-slip)*(1-fee)
    cash = peak = 100.
    peak_stamp = None
    witness = dict(pct=0., peak_bar_utc=None, low_bar_utc=None)
    origin = bars[SYMBOLS[0]][0].time

    def mark_peak(equity, stamp):
        nonlocal peak, peak_stamp
        if equity > peak:
            peak, peak_stamp = equity, stamp

    def lower(equity, stamp):
        nonlocal witness
        value = 100*(1-equity/peak)
        if value > witness['pct']:
            witness = dict(pct=value, peak_bar_utc=utc(peak_stamp) if peak_stamp else None,
                           low_bar_utc=utc(stamp), peak_equity=peak, low_equity=equity)

    for trade in account['ledger']:
        series = bars[trade['symbol']]
        legs = {row['time']:row for row in trade['buy_legs']}
        units = 0.
        first, stop = [(trade[k]-origin)//BAR for k in ('entry','exit')]
        for b in series[first:stop]:
            if b.open is None:
                continue
            if b.time in legs:
                mark_peak(cash+units*b.open*multiplier,b.time)
                leg = legs[b.time]
                cash -= leg['debit']
                units += leg['quantity']
            lower(cash+units*b.low*multiplier,b.time)
            mark_peak(cash+units*b.high*multiplier,b.time)
        cash += trade['credit']
        lower(cash,trade['exit'])
        mark_peak(cash,trade['exit'])
    if not math.isclose(cash-100,account['net'],abs_tol=1e-8):
        raise ValueError('Drawdown ledger does not reconcile')
    return witness


if __name__ == '__main__':
    bitstamp,audit = load_bitstamp()
    binance = load('2017-12-01',allow_aligned_gaps=True)
    result = dict(bitstamp=review(bitstamp,WINDOWS),bitstamp_source=audit,binance=review(binance,WINDOWS[1:]))
    period = '2020-01-01/2026-11-01'
    result['staged_drawdown_witness'] = {
        name: drawdown_witness(data,result[name][period]['staged']['base'])
        for name,data in [('bitstamp',bitstamp),('binance',binance)]}
    print(json.dumps(result,indent=2))
