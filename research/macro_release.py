"""Historical DOGE depth around confirmed U.S. CPI and FOMC releases."""
import argparse
import csv
import math
from collections import defaultdict
from concurrent.futures import ProcessPoolExecutor, as_completed
from datetime import date
from pathlib import Path

from depth_ladder import replay_hour
from external_clocks import first_sunday
from participant_traces import replay


ROOT = Path(__file__).parent
CPI = {
    2025: '01-15 02-12 03-12 04-10 05-13 06-11 07-15 08-12 09-11 10-24 12-18',
    2026: '01-13 02-13 03-11 04-10 05-12 06-10 07-14 08-12 09-11 10-14 11-10',
}
FOMC = {
    2025: '01-29 03-19 05-07 06-18 07-30 09-17 10-29 12-10',
    2026: '01-28 03-18 04-29 06-17 07-29 09-16 10-28',
}
HOLD = 600
TRIGGERS = ('all', 'upswing', 'drawdown', 'up_then_holds', 'down_then_recovers')
ARMS = ('release', 'hour_before')


def official_clock(family, day):
    session = date.fromisoformat(day)
    summer = first_sunday(session.year, 3).day + 7
    march_transition = date(session.year, 3, summer)
    november_transition = first_sunday(session.year, 11)
    shift = 4 if march_transition <= session < november_transition else 5
    hour, minute = {'cpi': (8, 30), 'fomc_statement': (14, 0), 'fomc_conference': (14, 30)}[family]
    return hour + shift, minute


def as_quotes(states):
    return [(None, *state[:4]) if state else None for state in states]


def signals(quotes, anchor):
    initial = quotes[anchor - 1]
    first = quotes[anchor + 59]
    confirmed = quotes[anchor + 119]
    observations = {}
    if not initial or not first:
        return observations
    shock = math.log(sum(first[1:3]) / sum(initial[1:3]))
    observations['all'] = (anchor + 59, HOLD, shock, 0., anchor)
    if shock >= .003:
        observations['upswing'] = observations['all']
    if shock <= -.003:
        observations['drawdown'] = observations['all']
    if not confirmed:
        return observations
    afterwards = math.log(sum(confirmed[1:3]) / sum(first[1:3]))
    delayed = (anchor + 119, HOLD, shock, afterwards, anchor)
    if shock >= .003 and afterwards >= -.001:
        observations['up_then_holds'] = delayed
    if shock <= -.003 and afterwards >= .001:
        observations['down_then_recovers'] = delayed
    return observations


def session(job):
    day, families = job
    current_hour, _ = official_clock(families[0], day)
    base = day.replace('-', '')
    first_hour = current_hour - (1 if families == ('cpi',) else 2)
    quotes = as_quotes([state for hour in range(first_hour, current_hour + 1)
                        for state in replay_hour(base + f'{hour:02}')])
    offset = first_hour * 3600
    summary, fills = [], []
    for family in families:
        _, minute = official_clock(family, day)
        for arm in ARMS:
            hour = current_hour if arm == 'release' else current_hour - 1
            anchor = (hour - first_hour) * 3600 + minute * 60
            events = signals(quotes, anchor)
            for rule in TRIGGERS:
                event = [events[rule]] if rule in events else []
                for fee in (.001, .002):
                    outcome, ledger = replay(quotes, event, fee)
                    key = (day, family, arm, rule, fee)
                    summary.append((*key, anchor + offset, int('all' in events), *outcome))
                    for candidate_sec, hold, shock, after, _, entry, exit_sec, quantity, debit, credit, net in ledger:
                        fills.append((*key, candidate_sec + offset, hold, shock, after, entry + offset, exit_sec + offset, quantity, debit, credit, net))
    return summary, fills


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workers', type=int, default=4, choices=range(1, 7))
    parser.add_argument('--year', type=int, choices=sorted(CPI), default=2025)
    args = parser.parse_args()
    tasks = [(f'{args.year}-{suffix}', ('cpi',)) for suffix in CPI[args.year].split()]
    tasks.extend((f'{args.year}-{suffix}', ('fomc_statement', 'fomc_conference')) for suffix in FOMC[args.year].split())
    summaries, fills, errors = [], [], []
    with ProcessPoolExecutor(args.workers) as pool:
        jobs = {pool.submit(session, item): item[0] for item in tasks}
        for job in as_completed(jobs):
            day = jobs[job]
            try:
                summary, trades = job.result()
            except (OSError, ValueError) as error:
                errors.append((day, str(error)))
                print(day, 'unavailable', str(error), flush=True)
                continue
            summaries.extend(summary)
            fills.extend(trades)
            print(day, len(trades), flush=True)
    stem = 'macro_release' if args.year == 2025 else f'macro_release_{args.year}'
    with (ROOT / f'{stem}_results.csv').open('w', newline='') as file, (ROOT / f'{stem}_trades.csv').open('w', newline='') as trades, (ROOT / f'{stem}_missing.csv').open('w', newline='') as missing:
        writer, journal, absent = csv.writer(file), csv.writer(trades), csv.writer(missing)
        writer.writerow(('day', 'family', 'arm', 'rule', 'fee', 'anchor_utc_sec', 'anchor_input', 'raw_signals', 'signal_seconds',
                         'attempts', 'misses', 'trades', 'wins', 'net_usdt', 'dd_pct', 'unpriced_seconds', 'delayed_exit_seconds'))
        journal.writerow(('day', 'family', 'arm', 'rule', 'fee', 'signal_utc_sec', 'hold', 'first_minute_log_move', 'next_minute_log_move',
                          'entry_utc_sec', 'exit_utc_sec', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt'))
        absent.writerow(('day', 'error'))
        writer.writerows(sorted(summaries))
        journal.writerows(sorted(fills))
        absent.writerows(sorted(errors))


if __name__ == '__main__':
    main()
