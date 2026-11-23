"""Minute-price check of the cooldown candidate on both venues."""
import argparse
import http.client
import json
import math
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

from weekly_rotation_delay_audit import ENDPOINT as BINANCE_ENDPOINT, minute_open, reprice
from weekly_rotation_venue_review import ENDPOINT as BITSTAMP_ENDPOINT, MARKETS


PERIOD = '2020-01-01/2026-11-01'


def bitstamp_minute(point):
    symbol,stamp = point
    market = MARKETS[symbol]
    query = urllib.parse.urlencode(dict(step=60,limit=1,end=stamp))
    url = f'{BITSTAMP_ENDPOINT}/{market}/?{query}'
    for i in range(5):
        try:
            with urllib.request.urlopen(url,timeout=18) as response:
                rows = json.load(response)['data']['ohlc']
            if len(rows)!=1 or int(rows[0]['timestamp'])!=stamp:
                raise ValueError(f'Expected an exact minute at {point}')
            return float(rows[0]['open'])
        except urllib.error.HTTPError as exc:
            if i==4 or exc.code not in (429,500,502,503,504):
                raise
            retry_after = exc.headers.get('Retry-After','')
            time.sleep(max(2*(i+1),int(retry_after) if retry_after.isdecimal() else 0))
        except (OSError,http.client.IncompleteRead) as exc:
            if i==4:
                raise RuntimeError(f'Bitstamp minute retrieval failed at {point}') from exc
            time.sleep(2*(i+1))


def cached_reader(venue, reader):
    directory = Path(tempfile.gettempdir())/f'moneyrobert-cooldown-{venue}-1m'
    directory.mkdir(exist_ok=True)

    def cached(point):
        symbol,stamp = point
        path = directory/f'{symbol}-{stamp}.txt'
        price = float(path.read_text(encoding='ascii')) if path.exists() else reader(point)
        if not math.isfinite(price) or price<=0:
            raise ValueError(f'Invalid minute price at {point}')
        if not path.exists():
            path.write_text(str(price),encoding='ascii')
        return price

    return cached


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('bitstamp','binance'))
    parser.add_argument('replay',type=Path)
    args = parser.parse_args()
    data = json.loads(args.replay.read_bytes())
    accounts = {label:r['accounts']['one_week'] for label,r in data[args.venue][PERIOD].items()}
    endpoint,reader,workers = ((BITSTAMP_ENDPOINT,bitstamp_minute,4) if args.venue=='bitstamp' else
                             (BINANCE_ENDPOINT,minute_open,8))
    checks = reprice(accounts,cached_reader(args.venue,reader),workers)
    print(json.dumps(dict(venue=args.venue,period=PERIOD,endpoint=endpoint,**checks),indent=2))
