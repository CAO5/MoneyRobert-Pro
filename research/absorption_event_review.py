"""Measure anonymous quote replenishment against matched high-pressure spot events."""
import argparse
import json
import math
from collections import Counter, defaultdict, deque
from concurrent.futures import ProcessPoolExecutor
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean, median

from flow_history_download import read_downloads
from flow_history_replay import HOUR, NS, Market, verify_downloads
from flow_portfolio import STEPS


SYMBOLS = ('BTCUSDT', 'ETHUSDT', 'DOGEUSDT')
WINDOW = 30
HORIZONS = (30, 60, 300, 900)
PRIMARY = 300
DELAY = 1
SIZE = 25.
COSTS = {'base': (.001, .0005), 'stress': (.002, .001)}
MIN_DOMINANCE = .70
MATCH_HOURS = 2
MATCH_TOLERANCE = 2.
CONFIRM_SECONDS = 30
EXHAUSTION_RATIO = .5


@dataclass(frozen=True)
class Event:
    time: int
    side: str
    pressure: float
    impact_bps: float
    spread_bps: float
    range_bps: float
    depth: float
    refill: float
    ofi: float

    def factors(self):
        return (math.log(self.pressure), self.impact_bps, self.spread_bps, self.range_bps, math.log(self.depth))


@dataclass(frozen=True)
class Confirmation(Event):
    shock_time: int
    pressure_ratio: float
    recovery_bps: float

    def factors(self):
        return (*super().factors(),self.pressure_ratio,self.recovery_bps)


def quantile(values, p):
    ordered = sorted(values)
    return ordered[math.floor((len(ordered)-1)*p)]


def queue_imbalance(old, new):
    bid, ask, qb, qa = new
    ob, oa, oqb, oqa = old
    return ((qb if bid >= ob else 0) - (oqb if bid <= ob else 0)
            - (qa if ask <= oa else 0) + (oqa if ask >= oa else 0))


def matched_refills(adds, trades):
    """Credit visible same-price increases only to earlier observed aggressive prints."""
    output = defaultdict(lambda: [0., 0.])
    pending = defaultdict(deque)
    cursor = 0
    for stamp, side, price, size in adds:
        while cursor < len(trades) and trades[cursor][0] < stamp:
            when, trade_side, trade_price, quantity = trades[cursor]
            cursor += 1
            if stamp-when <= NS:
                pending[trade_side,trade_price].append((when,quantity))
        credits = pending[side,price]
        while credits and stamp-credits[0][0] > NS:
            credits.popleft()
        used = 0.
        while credits and size > 0:
            when, quantity = credits.popleft()
            matched = min(size,quantity)
            size -= matched
            used += matched
            if quantity > matched:
                credits.appendleft((when,quantity-matched))
        output[stamp//NS][side] += used*price
    return output


class QuoteFlow:
    def __init__(self):
        self.first, self.last = {}, {}
        self.adds, self.trades = [], []
        self.ofi = defaultdict(float)

    def book_update(self, stamp, old, new):
        second = stamp//NS
        self.first.setdefault(second,new)
        self.last[second] = new
        if old is None or not 0 < old[0] < old[1]:
            return
        bid, ask, qb, qa = new
        self.ofi[second] += queue_imbalance(old,new)*(bid+ask)/2
        if bid == old[0] and qb > old[2]:
            self.adds.append((stamp,0,bid,qb-old[2]))
        if ask == old[1] and qa > old[3]:
            self.adds.append((stamp,1,ask,qa-old[3]))

    def refills(self):
        return matched_refills(self.adds,self.trades)


def observe_hour(market, folder, symbol, hour):
    flow = QuoteFlow()
    _, gaps = market.books(folder/f'{symbol}_orderbook.parquet',hour,observe=flow.book_update)
    totals = market.trades(folder/f'{symbol}_trades.parquet',hour,gaps,
                           observe=lambda t,p,q,m:flow.trades.append((t,0 if m else 1,p,q)))
    return flow.first,flow.last,totals,flow.ofi,flow.refills()


def confirm_pressure(event, hour, points, totals, gaps, end):
    stop = event.time+CONFIRM_SECONDS
    if stop >= end or stop > hour+HOUR:
        return None
    offset = event.time-hour
    q = points[offset-1:stop-hour]
    if not all(q) or any(t in gaps for t in range(event.time,stop)):
        return None
    sums = [math.fsum(totals[t][i] for t in range(event.time,stop) if t in totals) for i in (0,1)]
    adverse = 0 if event.side == 'bid' else 1
    ratio = sums[adverse]/event.pressure
    recovery = (1 if event.side == 'bid' else -1)*math.log(sum(q[-1][:2])/sum(q[0][:2]))*10000
    if ratio > EXHAUSTION_RATIO or sums[1-adverse] <= sums[adverse] or recovery <= 0:
        return None
    return Confirmation(**dict(asdict(event),time=stop),shock_time=event.time,pressure_ratio=ratio,recovery_bps=recovery)


def scan_hour(hour, last, totals, ofi, refills, end, *, confirmations=None):
    state = None
    gaps = {t for t,row in totals.items() if row[2]}
    points = []
    for stamp in range(hour, hour+HOUR):
        if stamp in gaps:
            state = None
        elif stamp in last:
            state = stamp,last[stamp]
        quote = state[1] if state and stamp-state[0] < 2 else None
        points.append(quote)
    events = []
    # Avoid matching refill credits across files and deriving features across an opening checkpoint.
    for offset in range(2*WINDOW,HOUR+1,WINDOW):
        decision = hour+offset
        if decision >= end or any(t in gaps for t in range(decision-WINDOW-1,decision)):
            continue
        q = points[offset-WINDOW-1:offset]
        if not all(q):
            continue
        flows = [totals[t] for t in range(decision-WINDOW,decision) if t in totals]
        sold, bought = (math.fsum(r[i] for r in flows) for i in (0,1))
        total = sold+bought
        action = 0 if sold > bought else 1
        pressure = max(sold,bought)
        if total <= 0 or pressure/total < MIN_DOMINANCE:
            continue
        mids = [(p[0]+p[1])/2 for p in q]
        depth = mean(p[0]*p[2]+p[1]*p[3] for p in q[1:])
        seconds = range(decision-WINDOW,decision)
        add = math.fsum(refills[t][action] for t in seconds if t in refills)
        events.append(Event(decision,'bid' if action == 0 else 'ask',pressure,
                            (1 if action else -1)*math.log(mids[-1]/mids[0])*10000,
                            mean((p[1]-p[0])*10000/m for p,m in zip(q[1:],mids[1:])),
                            math.log(max(mids)/min(mids))*10000,depth,add/pressure,
                            math.fsum(ofi[t] for t in seconds if t in ofi)/depth))
        if confirmations is not None:
            confirmation = confirm_pressure(events[-1],hour,points,totals,gaps,end)
            if confirmation:
                confirmations.append(confirmation)
    return events,gaps


def net_bps(entry, exit, buy_first, step, fee, slippage):
    buy, sell = (entry,exit) if buy_first else (exit,entry)
    paid, received = buy[1]*(1+slippage),sell[0]*(1-slippage)
    units = math.floor(SIZE/(paid if buy_first else received)/step)*step
    if not units or buy[3] < units or sell[2] < units:
        return None
    reference = paid if buy_first else received
    return (received*(1-fee)-paid*(1+fee))/reference*10000


def event_metrics(symbol, e, quotes, quality, end):
    outcome = {}
    entry = e.time+DELAY
    step = STEPS[symbol]
    for horizon in HORIZONS:
        exit = entry+horizon
        initial, final = quotes.get(entry), quotes.get(exit)
        if exit >= end or not (initial and final) or quality[exit-quality.start+1] != quality[e.time-quality.start]:
            outcome[str(horizon)] = None
            continue
        outcome[str(horizon)] = {name:net_bps(initial,final,e.side=='bid',step,*cost)
                                 for name,cost in COSTS.items()}
    return outcome


class Quality(list):
    def __init__(self, start, end, gaps):
        super().__init__([0])
        self.start = start
        for t in range(start,end):
            self.append(self[-1]+int(t in gaps))


def fit_thresholds(training, weak_refill):
    if len(training) < 30:
        return None
    pressure_cut = quantile([e.pressure for e in training],.90)
    strong = [e for e in training if e.pressure >= pressure_cut]
    impact_cut = median(e.impact_bps for e in strong)
    train_muted = [e for e in strong if e.impact_bps <= impact_cut]
    refill_cut = quantile([e.refill for e in train_muted],.75)
    control_cut = quantile([e.refill for e in train_muted],.25) if weak_refill else 0.
    features = list(zip(*(e.factors() for e in train_muted)))
    floors = (.10, .10, .01, .10, .10)
    scales = [max(quantile(v,.75)-quantile(v,.25),floor) for v,floor in zip(features,floors)]
    return dict(training_events=len(training),pressure_cut_usdt=pressure_cut,impact_cut_bps=impact_cut,
                refill_cut=refill_cut,control_refill_ceiling=control_cut),scales


def match_exposed(exposed, controls, scales):
    controls = dict(controls)
    pairs = []
    limit = MATCH_HOURS*HOUR
    selected_until = 0
    for event in exposed:
        if event.time < selected_until:
            continue
        ev = event.factors()
        valid = []
        for t, control in controls.items():
            if t//86400 != event.time//86400 or abs(t-event.time) > limit or abs(t-event.time) < PRIMARY:
                continue
            differences = [abs(a-b)/d for a,b,d in zip(ev,control.factors(),scales)]
            if max(differences) <= MATCH_TOLERANCE:
                valid.append((sum(differences),abs(t-event.time),t,control))
        if valid:
            _,_,t,control = min(valid,key=lambda row:row[:3])
            controls = {k:v for k,v in controls.items() if abs(k-t) >= PRIMARY}
            pairs.append((event,control))
            selected_until = event.time+PRIMARY
    return pairs


def pair_events(training, testing, weak_refill=False, *, training_confirmations=None):
    model = fit_thresholds(training,weak_refill)
    if model is None:
        return dict(training_events=len(training), candidates=0, controls=0, pairs=[]),[],[]
    cut,scales = model
    eligible = [e for e in testing if e.pressure >= cut['pressure_cut_usdt'] and e.impact_bps <= cut['impact_cut_bps']]
    refill_cut = cut['refill_cut']
    exposed = [e for e in eligible if e.refill > 0 and e.refill >= refill_cut]
    controls = {e.time:e for e in eligible if e.refill <= cut['control_refill_ceiling'] and (e.refill < refill_cut or e.refill == 0)}
    if training_confirmations is not None:
        features = list(zip(*(e.factors()[5:] for e in training_confirmations)))
        for index,floor in enumerate((.05,.10)):
            values = features[index] if features else ()
            scales.append(max(quantile(values,.75)-quantile(values,.25),floor) if values else floor)
    return dict(**cut,candidates=len(exposed),controls=len(controls),pairs=match_exposed(exposed,controls,scales)),eligible,exposed


def nonoverlapping_primary_events(events):
    until = 0
    for event in events:
        if event.time < until:
            continue
        until = event.time+PRIMARY
        yield event


def summarize_confirmed(events, evaluate, costs):
    outcomes = {lane:[] for lane in costs}
    skip = 0
    for event in nonoverlapping_primary_events(events):
        marks = evaluate(event)[str(PRIMARY)]
        if not marks or any(value is None for value in marks.values()):
            skip += 1
            continue
        for lane,value in marks.items():
            outcomes[lane].append(value)
    return dict(missing=skip,counts={k:len(v) for k,v in outcomes.items()},
                mean_bps={k:mean(v) if v else None for k,v in outcomes.items()})


def source_paths(symbol, hour):
    label = datetime.fromtimestamp(hour,timezone.utc).strftime('binance_spot/%Y-%m-%d/%H')
    return label,[f'{label}/{symbol}_{kind}.parquet' for kind in ('orderbook','trades')]


def check_coverage(root, first, end):
    manifest = read_downloads(root)
    missing = []
    for hour in range(first,end,HOUR):
        for symbol in SYMBOLS:
            _,paths = source_paths(symbol,hour)
            for path in paths:
                if manifest.get(path,{}).get('status') != 'ok' or not (root/'raw'/path).is_file():
                    missing.append(path)
    if missing:
        raise ValueError(f'{len(missing)} required source files missing or unverified; first: {missing[0]}')


def inspect_market(root, symbol, start, train_end, end, weak_refill, *, evaluate_from=None, confirm=False,
                   compare_refill_controls=False):
    market = Market()
    events, quotes, gaps = [], {}, set()
    confirmations = [] if confirm or compare_refill_controls else None
    evaluate_from = train_end if evaluate_from is None else evaluate_from
    manifest = read_downloads(root)
    verified = []
    for hour in range(start,end,HOUR):
        label,paths = source_paths(symbol,hour)
        verify_downloads(root,paths)
        verified.extend((p,manifest[p]['sha256']) for p in paths)
        first,last,totals,ofi,refill = observe_hour(market,root/'raw'/label,symbol,hour)
        if market.book_id is None and not quotes:
            raise ValueError(f'No opening book to seed {symbol} {label}')
        current,bad = scan_hour(hour,last,totals,ofi,refill,end,confirmations=confirmations)
        events.extend(current)
        quotes.update(first)
        gaps.update(bad)
    quality = Quality(start,end,gaps)
    def evaluate(event):
        return event_metrics(symbol,event,quotes,quality,end)

    if compare_refill_controls:
        scores = {}
        for variant, confirmed in (('absorption',None),('exhaustion',confirmations)):
            scores[variant] = {
                control:score_sides(events,confirmed,train_end,evaluate_from,use_weak,evaluate,standalone=True)
                for control,use_weak in (('no_observed_refill',False),('calibration_bottom_quartile',True))
            }
        variants = dict(variants=scores)
    else:
        variants = dict(sides=score_sides(events,confirmations,train_end,evaluate_from,weak_refill,evaluate))
    print(symbol,'finished',len(events),'eligible-pressure windows',flush=True)
    return symbol,dict(book_audit=dict(market.audit),invalid_seconds=len(gaps),valid_update_seconds=len(quotes),
                       source_digests=verified, **variants)


def score_sides(events, confirmations, train_end, evaluate_from, weak_refill, evaluate, *, standalone=False,
                costs=COSTS):
    confirm = confirmations is not None
    results = {}
    for side in ('bid','ask'):
        scorable = confirmations if confirm else events
        outcome,eligible,exposed = pair_events(
            [e for e in events if e.side == side and e.time < train_end],
            [e for e in scorable if e.side == side and (e.shock_time if confirm else e.time) >= evaluate_from],
            weak_refill,training_confirmations=[e for e in confirmations if e.side == side and e.time < train_end] if confirm else None)
        if confirm:
            outcome['exhaustion_without_refill_filter'] = summarize_confirmed(eligible,evaluate,costs)
            outcome['exhaustion_with_strong_refill'] = summarize_confirmed(exposed,evaluate,costs)
        elif standalone:
            outcome['pressure_without_refill_filter'] = summarize_confirmed(eligible,evaluate,costs)
            outcome['pressure_with_strong_refill'] = summarize_confirmed(exposed,evaluate,costs)
        ledger = []
        for candidate,control in outcome.pop('pairs'):
            ledger.append(dict(candidate=asdict(candidate),control=asdict(control),
                               treated_bps=evaluate(candidate),control_bps=evaluate(control)))
        scores = {}
        for horizon in map(str,HORIZONS):
            scores[horizon] = {}
            for lane in costs:
                pairs = [(row['treated_bps'][horizon][lane],row['control_bps'][horizon][lane]) for row in ledger
                         if row['treated_bps'][horizon] is not None and row['control_bps'][horizon] is not None and
                         row['treated_bps'][horizon][lane] is not None and row['control_bps'][horizon][lane] is not None]
                scores[horizon][lane] = dict(pairs=len(pairs), missing=len(ledger)-len(pairs),
                                             treated_mean_bps=mean(a for a,_ in pairs) if pairs else None,
                                             control_mean_bps=mean(b for _,b in pairs) if pairs else None,
                                             paired_difference_bps=mean(a-b for a,b in pairs) if pairs else None)
        results[side] = dict(**outcome,scores=scores,ledger=ledger)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root',type=Path)
    parser.add_argument('--start',default='2026-10-20T00:00:00+00:00')
    parser.add_argument('--hours',type=int,default=24)
    parser.add_argument('--calibration-hours',type=int,default=12)
    parser.add_argument('--evaluation-offset-hours',type=int,help='Skip previously scored hours before testing')
    parser.add_argument('--weak-refill-control',action='store_true',help='Use the calibration bottom quartile as control')
    parser.add_argument('--confirm-exhaustion',action='store_true',help='Wait for weaker pressure, signed flow reversal and a recovered mid')
    parser.add_argument('--compare-refill-controls',action='store_true',
                        help='Report both entry variants and both controls from one pass over the archives')
    parser.add_argument('--output',type=Path,required=True)
    args = parser.parse_args()
    start = datetime.fromisoformat(args.start)
    if start.utcoffset() != timezone.utc.utcoffset(start) or start.year < 2024 or start.minute or start.second or start.microsecond:
        parser.error('Supply a whole UTC hour in 2024 or later')
    if not 0 < args.calibration_hours < args.hours:
        parser.error('The evaluation needs separate calibration and scoring hours')
    if args.compare_refill_controls and (args.weak_refill_control or args.confirm_exhaustion):
        parser.error('The comparison already includes both flags; do not combine them')
    first = int(start.timestamp())
    split,end = first+args.calibration_hours*HOUR,first+args.hours*HOUR
    offset = args.calibration_hours if args.evaluation_offset_hours is None else args.evaluation_offset_hours
    if not args.calibration_hours <= offset < args.hours:
        parser.error('The scoring period must start at or after the calibration cutoff, before the end')
    scoring = first+offset*HOUR
    try:
        check_coverage(args.root,first,end)
    except ValueError as error:
        parser.error(str(error))
    with ProcessPoolExecutor(len(SYMBOLS)) as pool:
        jobs = [pool.submit(inspect_market,args.root,s,first,split,end,args.weak_refill_control,
                            evaluate_from=scoring,confirm=args.confirm_exhaustion,
                            compare_refill_controls=args.compare_refill_controls) for s in SYMBOLS]
        reports = dict(job.result() for job in jobs)
    payload = dict(start_utc_second=first,calibration_end_utc_second=split,end_utc_second=end,
                   primary_seconds=PRIMARY,all_horizons_seconds=HORIZONS,delay_seconds=DELAY,costs=COSTS,
                   control_style='calibration_bottom_quartile' if args.weak_refill_control else 'no_observed_refill',
                   spot_ask_absorption='Downward price response and hypothetical symmetric cost benchmark. Not a tradable spot short.',
                   symbols=reports)
    if args.confirm_exhaustion or scoring != split:
        payload.update(evaluation_start_utc_second=scoring,
                       confirmation=dict(seconds=CONFIRM_SECONDS,maximum_remaining_pressure=EXHAUSTION_RATIO,
                                         opposite_turnover_dominates=True,mid_recovers=True) if args.confirm_exhaustion else None)
    if args.compare_refill_controls:
        payload.pop('control_style')
        payload.pop('confirmation',None)
        payload.update(evaluation_start_utc_second=scoring,
                       exhaustion_confirmation=dict(seconds=CONFIRM_SECONDS,maximum_remaining_pressure=EXHAUSTION_RATIO,
                                                    opposite_turnover_dominates=True,mid_recovers=True))
    args.output.write_text(json.dumps(payload,indent=2,allow_nan=False)+'\n',encoding='utf-8')


if __name__ == '__main__':
    main()
