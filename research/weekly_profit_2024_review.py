"""Recompute the BTC/ETH comparison with only 2024-and-later prices."""
import argparse
import hashlib
import json
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from datetime import date, timedelta
from functools import partial
from pathlib import Path

from slow_spot_quotes import recent_bars
from slow_spot_replay import BAR, SYMBOLS, epoch, load, utc
from weekly_profit_policy import COSTS
from weekly_profit_start_windows import LENGTHS, RULES, STRIDE_WEEKS, metrics, windows
from weekly_profit_stop_confirmation import VolatilityCheckedStop
from weekly_profit_volatility import volatility
from weekly_rotation_delayed_account import bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import MARKETS, fetch


DATA_START = '2024-01-01'
FIRST = date(2024,2,5)
END = date(2026,11,16)
RECENT_START = '2026-10-20'
DATA_END = '2026-11-19'
ARCHIVE_END = '2026-11-01'
SPLIT = '2026-01-05'


def load_2024(venue):
    first,last = map(epoch,(DATA_START,DATA_END))
    if venue=='binance':
        bars = load(DATA_START,ARCHIVE_END)
        for symbol in SYMBOLS:
            bars[symbol].extend(b for b in recent_bars(symbol) if b.time>=epoch(ARCHIVE_END))
        metadata = dict(source='Binance monthly 4h files plus public REST candles for November')
    else:
        root = Path(tempfile.gettempdir())/'moneyrobert-bitstamp-scoped-2024'
        root.mkdir(exist_ok=True)
        stride = 768*BAR
        bars = {s:[] for s in SYMBOLS}
        keys = [(s,(MARKETS[s],start,min(start+stride,last)))
                for s in SYMBOLS for start in range(first,last,stride)]
        with ThreadPoolExecutor(4) as pool:
            chunks = pool.map(lambda item:fetch(root,item[1]),keys)
            manifest = []
            for (symbol,_),(part,source) in zip(keys,chunks):
                bars[symbol].extend(part)
                manifest.append(source)
        metadata = dict(source='Bitstamp public 4h OHLC',manifest=manifest)
    expected = list(range(first,last,BAR))
    for symbol,series in bars.items():
        if [bar.time for bar in series]!=expected:
            raise ValueError(f'Incomplete scoped candle data for {venue} {symbol}')
    metadata['bar_count_per_symbol'] = len(expected)
    metadata['bars_sha256'] = hashlib.sha256(json.dumps(bars,separators=(',',':')).encode('ascii')).hexdigest()
    return bars,metadata


def evaluate(venue):
    bars,source = load_2024(venue)
    origin = epoch(DATA_START)
    read_price = cached_reader(venue,minute_open if venue=='binance' else bitstamp_fill)
    before = first_minute(venue)

    def measure(symbol,boundary):
        return volatility(bars[symbol],(boundary-origin)//BAR)

    def compare(start,end,detailed=False):
        outcomes = {}
        for name,confirm in RULES.items():
            account_type = partial(VolatilityCheckedStop,measure=measure,confirm=confirm)
            lanes = {}
            for lane,cost in COSTS.items():
                a = run(bars,start,end,cost,read_price,before,account_type=account_type,
                        report_drawdown_amount=True,settle_at_next_fill=True)
                lanes[lane] = metrics(a)
                if detailed:
                    lanes[lane]['ledger'] = a['ledger']
            outcomes[name] = lanes
        return outcomes

    if (epoch(FIRST.isoformat())-origin)//BAR < 169:
        raise ValueError('The scoped study needs 29 known daily closes before trading')
    first,end = FIRST.isoformat(),END.isoformat()
    primary = {start+'/'+stop:compare(start,stop,True) for start,stop in
               ((first,end),(first,SPLIT),(SPLIT,end),(RECENT_START,DATA_END))}
    print(venue,'completed the scoped and recent windows',file=sys.stderr,flush=True)
    rolling = {str(length):[dict(start=start,end=stop,cases=compare(start,stop))
                           for start,stop in windows(length,FIRST,END)] for length in LENGTHS}
    decisions = []
    day = date.fromisoformat(RECENT_START)
    while day.isoformat()<DATA_END:
        if day.weekday()==0:
            index = (epoch(day.isoformat())-origin)//BAR
            ranks = {s:bars[s][index-1].close/bars[s][index-169].close-1 for s in SYMBOLS}
            top = max(ranks,key=ranks.get)
            decisions.append(dict(monday_utc=day.isoformat(),trailing_28d=ranks,
                                  ranked_candidate=top if ranks[top]>0 else None))
        day += timedelta(days=1)
    return dict(venue=venue,currency='USDT' if venue=='binance' else 'USD',start_data_utc=DATA_START,
                end_data_utc_exclusive=DATA_END,first_trading_date_utc=first,source=source,
                stride_weeks=STRIDE_WEEKS,primary=primary,rolling=rolling,recent_monday_signals=decisions)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
