"""Recompute the delayed cooldown account under fixed fee scenarios."""
import argparse
import json
import math
import sys
from pathlib import Path

from slow_spot_replay import load
from weekly_rotation_delayed_account import WINDOWS, bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_venue_review import load_bitstamp


CASES = {
    'discount': (.00075, .0005),
    'discount_double_cost': (.0015, .001),
    'zero_fee': (0., .0005),
    'zero_fee_double_slippage': (0., .001),
}
CONTROL = Path(__file__).with_name('weekly_rotation_delayed_account_results.json')


def evaluate(venue):
    market = load_bitstamp()[0] if venue=='bitstamp' else load('2017-12-01',allow_aligned_gaps=True)
    reader = cached_reader(venue,bitstamp_fill if venue=='bitstamp' else minute_open)
    bounds = first_minute(venue)
    reference = json.loads(CONTROL.read_text(encoding='utf-8'))['results'][venue]
    out = {}
    for start,end in WINDOWS:
        scope = start+'/'+end
        out[scope] = {'control':reference[scope]}
        for name,(fee,slip) in CASES.items():
            result = run(market,start,end,(fee,slip),reader,bounds)
            if result['pending'] or result['open_position']:
                raise ValueError(f'Unresolved account in {venue} {scope} {name}')
            result['fees_quote'] = math.fsum(
                fee*(trade['debit']/(1+fee)+trade['credit']/(1-fee)) for trade in result['ledger'])
            out[scope][name] = result
            print(f'{venue} {scope} {name} complete',file=sys.stderr,flush=True)
    return out


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    args = parser.parse_args()
    print(json.dumps(dict(costs_per_side=CASES,results=evaluate(args.venue)),indent=2))
