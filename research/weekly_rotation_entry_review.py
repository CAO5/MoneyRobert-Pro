"""Test changes to weekly rotation entries with the same exits and account budget."""
import json
import math
from pathlib import Path
from statistics import mean

from slow_spot_replay import BAR, STEPS, SYMBOLS, decisions, epoch, load, simulate
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


WEEK = 7*24*3600//BAR
THREE_DAYS = 3*24*3600//BAR


def entry_rules(bars, weekly):
    persistent, pullback, fixed_delay = {}, {}, {}
    for i, (picks, exits) in weekly.items():
        symbol = picks[0] if picks else None
        last_picks, _ = weekly.get(i-WEEK, ([], []))
        persistent[i] = (picks if picks == last_picks else [], exits)
        pullback[i] = ([], exits)
        fixed_delay[i] = ([], exits)
        if not symbol:
            continue
        series = bars[symbol]
        if i+THREE_DAYS < len(series):
            fixed_delay[i+THREE_DAYS] = (picks, [])
        cutoff = .97*series[i-1].close
        below = False
        for j in range(i+1, min(i+WEEK, len(series))):
            last_close = series[j-1].close
            if last_close is None:
                continue
            if below and last_close > cutoff:
                pullback[j] = (picks, [])
                break
            below = below or last_close <= cutoff
    return {
        'original': weekly,
        'two_weeks': persistent,
        'pullback_reclaim': pullback,
        'fixed_three_day_delay': fixed_delay,
    }


def signal_counts(signals, first, stop):
    return sum(bool(picks) for i, (picks, _) in signals.items() if first <= i < stop)


def week_matched(bars, alternatives, first, stop):
    fee, slip = BASE_COST
    weekly = alternatives['original']
    pullbacks = alternatives['pullback_reclaim']

    def net(symbol, entry, exit_index):
        purchase = bars[symbol][entry].open * (1+slip)
        units = math.floor(24.5/purchase/STEPS[symbol])*STEPS[symbol]
        credit = units*bars[symbol][exit_index].open*(1-slip)*(1-fee)
        return credit-units*purchase*(1+fee)

    paired, missed, persistent, first_weeks = [], [], [], []
    for i, (picks, _) in weekly.items():
        if not picks or i < first or i+WEEK >= stop:
            continue
        symbol = picks[0]
        immediate = net(symbol, i, i+WEEK)
        if alternatives['two_weeks'][i][0]:
            persistent.append(immediate)
        else:
            first_weeks.append(immediate)
        trigger = next((j for j in range(i+1, i+WEEK) if pullbacks.get(j, ((), ()))[0]), None)
        if trigger is None:
            missed.append(immediate)
            continue
        paired.append(dict(symbol=symbol, week=bars[symbol][i].time, delayed=bars[symbol][trigger].time,
                           immediate_net=immediate, reclaim_net=net(symbol, trigger, i+WEEK)))

    def summarize(values):
        return dict(windows=len(values), wins=sum(v>0 for v in values),
                    mean_net_usdt=mean(values) if values else None)

    return dict(immediate_matched=summarize([r['immediate_net'] for r in paired]),
                reclaim_matched=summarize([r['reclaim_net'] for r in paired]),
                immediate_weeks_without_reclaim=summarize(missed),
                immediate_persistent=summarize(persistent), immediate_first_week=summarize(first_weeks),
                pairs=paired)


def review(bars, previous):
    weekly = decisions(bars)['rotation']
    alternatives = entry_rules(bars, weekly)
    origin = bars[SYMBOLS[0]][0].time
    result = {}
    for period, old in previous.items():
        start, end = period.split('/')
        first, stop = [(epoch(bound)-origin)//BAR for bound in (start, end)]
        accounts = {}
        for label, cost in [('base', BASE_COST), ('stress', STRESS_COST)]:
            cases = {
                name: annotate(simulate(bars, {'rotation': signals}, 'rotation', start, end, cost,
                                       profit_protection=True))
                for name, signals in alternatives.items()
            }
            assert cases['original'] == old['profit_protection'][label]
            accounts[label] = cases
        result[period] = dict(signal_counts={name: signal_counts(rules, first, stop)
                                            for name, rules in alternatives.items()},
                              accounts=accounts, week_matched=week_matched(bars, alternatives, first, stop))
    return result


if __name__ == '__main__':
    control = Path(__file__).with_name('weekly_rotation_results.json')
    previous = json.loads(control.read_text(encoding='utf-8'))
    bars = load('2020-03-01')
    print(json.dumps(review(bars, {**previous['earlier'], **previous['recent']}), indent=2))
