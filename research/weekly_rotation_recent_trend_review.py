"""Check whether recent absolute price direction improves a relative leader entry."""
import argparse
import json
import math
import sys
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, load
from weekly_rotation_delayed_account import WINDOWS, bitstamp_fill, first_minute, run
from weekly_rotation_delay_audit import minute_open
from weekly_rotation_reentry_delay import cached_reader
from weekly_rotation_review import BASE_COST, STRESS_COST
from weekly_rotation_venue_review import load_bitstamp


SEVEN_DAYS = 7*86400//BAR
PERIODS = [('2018-01-01','2020-03-01'),*WINDOWS]


def return_7d(series,index):
    before, after = (series[j].close for j in (index-1-SEVEN_DAYS,index-1))
    if before is not None and after is not None:
        return math.log(after/before)


def leader_up(bars, symbol, i):
    r = return_7d(bars[symbol],i)
    return r is not None and r>0


def market_up(bars, symbol, i):
    returns = [return_7d(bars[s],i) for s in SYMBOLS]
    return None not in returns and math.fsum(returns)>0


def evaluate(venue):
    bars = load_bitstamp()[0] if venue=='bitstamp' else load('2017-12-01',allow_aligned_gaps=True)
    reader = cached_reader(venue,bitstamp_fill if venue=='bitstamp' else minute_open)
    before = first_minute(venue)
    original = json.loads(Path(__file__).with_name(
        'weekly_rotation_delayed_account_results.json').read_text(encoding='utf-8'))['results'][venue]
    outcomes = {}
    for start,end in PERIODS:
        span = start+'/'+end
        outcomes[span] = {}
        for mode,cost in [('base',BASE_COST),('stress',STRESS_COST)]:
            cases = {}
            for label,entry_check in [('control',None),('leader_up',leader_up),('market_up',market_up)]:
                a = run(bars,start,end,cost,reader,before,entry_check)
                if a['pending'] or a['open_position']:
                    raise ValueError(f'Unsettled account for {venue} {span} {mode} {label}')
                cases[label] = a
                print(f'{venue} {span} {mode} {label} done',file=sys.stderr,flush=True)
            if span in original and cases['control'] != original[span][mode]:
                raise ValueError('Delayed control does not match the previous ledger')
            outcomes[span][mode] = cases
    return outcomes


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('venue',choices=('binance','bitstamp'))
    print(json.dumps(evaluate(parser.parse_args().venue),indent=2))
