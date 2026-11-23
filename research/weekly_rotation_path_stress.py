"""Replay and stress the fixed weekly rotation paths without any new market data."""
import argparse
import hashlib
import json
import math
import random
import tempfile
from dataclasses import dataclass
from pathlib import Path

from slow_spot_replay import BAR, SYMBOLS, epoch
from weekly_account_score import DELAY, EPSILON, drawdown_trade, near
from weekly_profit_2024_review import ARCHIVE_END, DATA_END, DATA_START, load_2024
from weekly_profit_policy import COSTS, WEEK
from weekly_rotation_venue_review import MARKETS


INITIAL = 100.
MAX_DRAWDOWN = .08
BLOCKS = (4,13)
RESAMPLES = 5000
NAMES = ('immediate','confirmed')
ROOT = Path(__file__).parent


@dataclass
class WeekPath:
    net: float = 0.
    peak: float = 0.
    trough: float = 0.
    drawdown: float = 0.
    required_cash: float = 0.

    def mark(self, high, low):
        if high < low:
            raise ValueError('Invalid equity bound ordering')
        self.peak = max(self.peak,high)
        self.trough = min(self.trough,low)
        self.drawdown = max(self.drawdown,self.peak-low)
        self.required_cash = max(self.required_cash,((1-MAX_DRAWDOWN)*self.peak-low)/MAX_DRAWDOWN)


class Ignore:
    def mark(self, high, low):
        pass


def assemble(profiles):
    cash = peak = INITIAL
    drawdown = 0.
    risk_ok = True
    for profile in profiles:
        trough = cash+profile.trough
        drawdown = max(drawdown,profile.drawdown,peak-trough)
        risk_ok &= (cash+EPSILON >= profile.required_cash and trough+EPSILON >= peak*(1-MAX_DRAWDOWN))
        peak = max(peak,cash+profile.peak)
        cash += profile.net
    net = cash-INITIAL
    return dict(net=net,drawdown=drawdown,net_minus_drawdown=net-drawdown,under_eight_percent=risk_ok,
                passes=net>drawdown+EPSILON and risk_ok)


def cached_bars(venue):
    root = Path(tempfile.gettempdir())
    if venue=='binance':
        months = [f'{year}-{month:02}' for year in range(int(DATA_START[:4]),int(ARCHIVE_END[:4])+1)
                  for month in range(1,13) if DATA_START[:7] <= f'{year}-{month:02}' < ARCHIVE_END[:7]]
        paths = [root/'moneyrobert-spot-v1'/f'{name}-4h-{month}.zip' for name in SYMBOLS for month in months]
        paths += [root/f'slow-spot-{name}-2026-11-01_18.json' for name in SYMBOLS]
    elif venue=='bitstamp':
        first,last = epoch(DATA_START),epoch(DATA_END)
        stride = 768*BAR
        paths = [root/'moneyrobert-bitstamp-scoped-2024'/f'{MARKETS[name]}-{start}-{min(start+stride,last)}.json'
                 for name in SYMBOLS for start in range(first,last,stride)]
    else:
        raise ValueError(f'Unknown venue {venue}')
    if missing := next((path for path in paths if not path.is_file()),None):
        raise ValueError(f'Offline audit requires cached bars: {missing}')
    series,metadata = load_2024(venue)
    return {name:{bar.time:bar for bar in rows} for name,rows in series.items()},metadata


def venue_paths(venue, reference, archived, span):
    bars,metadata = cached_bars(venue)
    if metadata['bars_sha256'] != reference['source']['bars_sha256']:
        raise ValueError(f'Bar digest changed: {venue}')
    boundary_cache = Path(tempfile.gettempdir())/f'moneyrobert-cooldown-{venue}-pre-fill'
    bounds_used = {}

    def cached_minute(symbol, stamp):
        key = f'{symbol}-{stamp}.json'
        if key not in bounds_used:
            low,high = json.loads((boundary_cache/key).read_bytes())
            if not 0 < low <= high < math.inf:
                raise ValueError(f'Invalid cached execution minute: {venue}/{key}')
            bounds_used[key] = (low,high)
        return bounds_used[key]

    paths, calendar = {},None
    for name in NAMES:
        for lane,(fee,slip) in COSTS.items():
            case = reference['costs'][lane][name]
            series = case['weeks']
            dates = [row['week'] for row in series]
            if calendar is None:
                calendar = dates
            if dates != calendar or any(epoch(after)-epoch(before)!=WEEK for before,after in zip(dates,dates[1:])):
                raise ValueError('Mismatched or discontinuous calendar')
            ledger = archived['primary'][span][name][lane]['ledger']
            indexed = {trade['entry']-DELAY:trade for trade in ledger}
            if len(indexed)!=len(ledger) or not indexed.keys() <= {epoch(day) for day in dates}:
                raise ValueError('Trade omitted from the weekly path')
            profiles = []
            for row in series:
                profile = WeekPath()
                trade = indexed.get(epoch(row['week']))
                if trade:
                    drawdown_trade(trade,0.,profile,Ignore(),bars,cached_minute,(1-fee)*(1-slip))
                    profile.net = trade['pnl']
                if not near(profile.net,row['net']) or not near(profile.drawdown,row['drawdown_amount']):
                    raise ValueError(f'Path changed at {venue}/{name}/{lane}/{row["week"]}')
                profiles.append(profile)
            actual = assemble(profiles)
            score = case['score']
            if not near(actual['net'],score['net']) or not near(actual['drawdown'],score['drawdown_amount']):
                raise ValueError(f'Full path does not reconcile: {venue}/{name}/{lane}')
            paths[name,lane] = profiles
    used_sha = hashlib.sha256(json.dumps(bounds_used,sort_keys=True).encode('ascii')).hexdigest()
    return paths,calendar,used_sha


def bootstrap(paths, calendar, block):
    rng = random.Random(20261121)
    counts = dict.fromkeys(NAMES,0)
    gaps = {name:[] for name in NAMES}
    length = len(calendar)
    for _ in range(RESAMPLES):
        indices = []
        while len(indices) < length:
            start = rng.randrange(length)
            indices.extend((start+offset)%length for offset in range(min(block,length-len(indices))))
        for name in NAMES:
            cases = [assemble(weeks[index] for index in indices) for key,weeks in paths.items() if key[1]==name]
            counts[name] += int(all(case['passes'] for case in cases))
            gaps[name].append(min(case['net_minus_drawdown'] for case in cases))
    return {name:dict(joint_pass_fraction=counts[name]/RESAMPLES,
                      worst_lane_profit_minus_drawdown_95=[sorted(gaps[name])[int((RESAMPLES-1)*fraction)]
                                                           for fraction in (.025,.975)]) for name in NAMES}


def report(account_file, trades_file):
    raw = account_file.read_bytes()
    book = json.loads(raw)
    archive_bytes = trades_file.read_bytes()
    archive_sha = hashlib.sha256(archive_bytes).hexdigest()
    if archive_sha != book['source_sha256']:
        raise ValueError('Trading ledger is not the source of the weekly account report')
    archive = json.loads(archive_bytes)
    paths, calendar, used = {},None,{}
    for venue,reference in book['venues'].items():
        cases,dates,used[venue] = venue_paths(venue,reference,archive['venues'][venue],
                                            book['first_week']+'/'+book['last_week_end'])
        if calendar is None:
            calendar = dates
        if dates != calendar:
            raise ValueError('Venues must be compared on the same weeks')
        paths.update({(venue,*key):value for key,value in cases.items()})
    canonical = paths['binance','confirmed','stress']
    winners = sorted(range(len(calendar)),key=lambda index:canonical[index].net,reverse=True)[:3]
    if any(canonical[index].net <= 0 for index in winners):
        raise ValueError('Not enough profitable weeks for the tail stress')
    historical = {'/'.join(key):dict(full=assemble(profile),no_top_three=assemble(
        WeekPath() if index in winners else week for index,week in enumerate(profile))) for key,profile in paths.items()}
    return dict(historical_diagnostic_only=True,promotion_allowed=False,weeks=len(calendar),
                tail_weeks_removed=[calendar[index] for index in winners],starting_account_units=INITIAL,
                ledger_sha256=archive_sha,weekly_book_sha256=hashlib.sha256(raw).hexdigest(),
                execution_minutes_sha256=used,historical=historical,resamples=RESAMPLES,
                paired_block_sensitivity={str(block):bootstrap(paths,calendar,block) for block in BLOCKS},
                interpretation='Same fixed weekly paths; not strategy reruns or forward probabilities. Each venue keeps its own quote currency.')


if __name__=='__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    data = report(ROOT/'weekly_account_results.json',ROOT/'weekly_profit_2024_results.json')
    args.output.write_text(json.dumps(data,indent=2,allow_nan=False)+'\n',encoding='utf-8')
