"""Evaluate a fixed historical-neighbor forecast on completed weekly outcomes."""
import json
import math
from datetime import datetime, timezone
from statistics import mean, pstdev

from slow_spot_replay import BAR, SYMBOLS, epoch, load, rotation_decisions, simulate
from weekly_rotation_entry_review import WEEK
from weekly_rotation_review import BASE_COST, STRESS_COST, annotate


TRAIN_WEEKS = 104
MINIMUM_TRAINING = 78
NEIGHBORS = 24
ACCOUNT_LIMITS = dict(entry_target=12.25, minimum_order_notional=10., profit_protection=True)


def date_at(bars, index):
    return datetime.fromtimestamp(bars[SYMBOLS[0]][index].time, timezone.utc).date().isoformat()


def features(bars, symbol, index):
    series = bars[symbol]
    other = SYMBOLS[1-SYMBOLS.index(symbol)]
    closes = [series[i].close for i in range(index-169, index, 6)]
    peer_then, peer_now = [bars[other][i].close for i in (index-169, index-1)]
    if None in (*closes, peer_then, peer_now):
        return
    change_28d = math.log(closes[-1]/closes[0])
    return (math.log(closes[-1]/closes[-8]), change_28d,
            change_28d-math.log(peer_now/peer_then),
            pstdev(math.log(b/a) for a,b in zip(closes, closes[1:])))


def observations(bars, weekly):
    out = {s:{} for s in SYMBOLS}
    for i in weekly:
        end = i+WEEK
        if end+6 >= len(bars[SYMBOLS[0]]):
            break
        for s in SYMBOLS:
            values = features(bars, s, i)
            if values is None or any(b.open is None for b in bars[s][i:end+1]):
                continue
            test = {i:([s],[]), end:([], list(SYMBOLS))}
            trade = simulate(bars, {'rotation':test}, 'rotation', date_at(bars,i), date_at(bars,end+6),
                             BASE_COST, **ACCOUNT_LIMITS)
            if trade['trades'] != 1:
                raise ValueError('Training window did not complete exactly one position')
            out[s][i] = values, trade['net']
    return out


def summarize(row):
    payouts = [net for _,_,net in row]
    return mean(payouts), sum(net>0 for net in payouts)/len(payouts)


def forecasts(bars, weekly):
    history = observations(bars, weekly)
    signals = {'neighbors':{}, 'unconditional':{}, 'weekly_momentum':{}, 'hold_momentum':weekly}
    audit = []
    origin = bars[SYMBOLS[0]][0].time
    for i, (picks, _) in weekly.items():
        if origin+i*BAR < epoch('2020-01-01'):
            continue
        signals['weekly_momentum'][i] = (picks,list(SYMBOLS))
        reported = {}
        for s in SYMBOLS:
            values = features(bars, s, i)
            recent = [(j, v, net) for j,(v,net) in history[s].items()
                      if i-(TRAIN_WEEKS+1)*WEEK <= j < i-WEEK]
            if values is None or len(recent) < MINIMUM_TRAINING:
                continue
            scales = [pstdev(v) or 1. for v in zip(*(v for _,v,_ in recent))]
            def distance(row):
                return math.fsum(((prior-now)/scale)**2 for prior,now,scale in zip(row[1],values,scales))
            closest = sorted(recent, key=distance)[:NEIGHBORS]
            reported[s] = dict(neighbors=summarize(closest), unconditional=summarize(recent),
                               training_count=len(recent), last_training_week=origin+recent[-1][0]*BAR,
                               neighbor_weeks=[origin+row[0]*BAR for row in closest])
        decision = {}
        for rule in ('neighbors','unconditional'):
            eligible = [s for s,p in reported.items() if p[rule][0]>0 and p[rule][1]>.5]
            chosen = max(eligible, key=lambda s:reported[s][rule][0]) if eligible else None
            signals[rule][i] = ([chosen] if chosen else [],list(SYMBOLS))
            decision[rule] = chosen
        audit.append(dict(week=origin+i*BAR,predictions=reported,selection=decision))
    return signals, audit


def review(bars):
    signals,audit = forecasts(bars, rotation_decisions(bars))
    periods = [(f'{y}-01-01', f'{y+1}-01-01') for y in range(2020,2026)] + [
        ('2026-01-01','2026-11-01'), ('2020-01-01','2026-11-01'), ('2022-01-01','2026-11-01')]
    results = {}
    for start,end in periods:
        results[start+'/'+end] = {}
        for name,rule in signals.items():
            results[start+'/'+end][name] = {
                label: annotate(simulate(bars,{'rotation':rule},'rotation',start,end,cost,**ACCOUNT_LIMITS))
                for label,cost in [('base',BASE_COST), ('stress',STRESS_COST)]}
    return dict(training_weeks=TRAIN_WEEKS, minimum_training=MINIMUM_TRAINING, neighbors=NEIGHBORS,
                audit=audit, results=results)


if __name__=='__main__':
    bars = load('2017-12-01', allow_aligned_gaps=True)
    print(json.dumps(review(bars),indent=2))
