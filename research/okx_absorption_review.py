"""Replay sealed OKX swap depth and trades by local receipt time, never place orders."""
import argparse
import json
import math
from bisect import bisect_left, bisect_right
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal
from pathlib import Path
from statistics import mean

from absorption_event_review import COSTS, DELAY, HORIZONS, HOUR, NS, PRIMARY, SIZE, Quality, QuoteFlow, scan_hour, score_sides
from flow_forward import sealed_rows
from flow_history_replay import Depth
from okx_behavior_tape import INSTRUMENTS


DAY = 24*HOUR
MS = 1000
CLOCK_WARMUP = 60
FUNDING_GUARD = 60*MS
DELAY_SENSITIVITY = (3,5)
MIN_FRESH_SECONDS = .90*DAY
SWAP_COSTS = {**COSTS,'okx_level1_example':(.0005,COSTS['base'][1])}
SPECS = {
    'BTC-USDT-SWAP': ('0.01', '0.01', '0.01'),
    'ETH-USDT-SWAP': ('0.1', '0.01', '0.01'),
    'DOGE-USDT-SWAP': ('1000', '0.01', '0.01'),
}


class RelativeDelay:
    def __init__(self):
        self.baseline = self.first = None

    def fresh(self, mono, exchange_ms):
        age = mono//1_000_000-exchange_ms
        if self.first is None:
            self.first = mono
            self.baseline = age
        self.baseline = min(self.baseline,age)
        return mono-self.first >= CLOCK_WARMUP*NS and age-self.baseline <= 1000


@dataclass(frozen=True)
class FillQuote:
    prices: tuple
    exchange_ms: int
    funding_time: int | None


class Swap:
    def __init__(self, instrument, *, quality_only=False, keep_execution=False):
        self.value, self.lot, self.minimum = map(Decimal,SPECS[instrument])
        self.quality_only = quality_only
        self.keep_execution = keep_execution
        self.hour_coverage = {}
        self.events, self.confirmations = [], []
        self.quotes, self.gaps, self.coverage = {}, set(), Counter()
        self.message_coverage, self.depth_coverage, self.gap_coverage = Counter(), Counter(), Counter()
        self.audit = Counter()
        self.funding, self.rate, self.mark = set(), {}, {}
        self.delays = defaultdict(RelativeDelay)
        self.reset()
        self.new_hour()

    def new_hour(self):
        self.flow = QuoteFlow()
        self.first_exchange = {}
        self.live_seconds = set()
        self.observed_seconds = set()
        self.turnover = defaultdict(lambda: [0.,0.,False])
        self.contaminated = False

    def reset(self):
        self.depth = {-1:Depth(-1),1:Depth(1)}
        self.sequence = self.trade_id = self.top = self.next_funding = None

    def bad(self, second, reason):
        self.turnover[second][2] = True
        self.audit[reason] += 1

    def book(self, wall, mono, action, row, *, checkpoint=False):
        second = wall//NS
        if not checkpoint:
            self.observed_seconds.add(second)
        before, after = int(row['prevSeqId']), int(row['seqId'])
        if action == 'snapshot' and before == -1:
            if self.sequence is not None:
                self.bad(second,'snapshot_reanchor')
            self.depth = {-1:Depth(-1),1:Depth(1)}
            self.top = None
            self.audit['snapshots'] += 1
        elif action != 'update' or before != self.sequence:
            self.bad(second,'book_sequence')
            self.sequence = self.top = None
            return
        self.sequence = after
        exchange_ms = int(row['ts'])
        for sign, field in ((-1,'bids'),(1,'asks')):
            for price, volume, *_ in row[field]:
                self.depth[sign].set(float(price),float(volume)*float(self.value))
        bid, bid_size = self.depth[-1].best()
        ask, ask_size = self.depth[1].best()
        if checkpoint:
            self.top = (bid,ask,bid_size,ask_size) if 0 < bid < ask else None
            return
        live = self.delays['books'].fresh(mono,exchange_ms)
        if not (0 < bid < ask and live and self.trade_id is not None):
            self.top = None
            self.bad(second,'book_not_ready')
            return
        self.live_seconds.add(second)
        if action == 'update' and (row['asks'] or row['bids']):
            top = (bid,ask,bid_size,ask_size)
            if not self.quality_only:
                self.flow.book_update(wall,self.top,top)
            schedule = self.next_funding
            known = schedule[1] if schedule and 0 <= exchange_ms-schedule[0] <= 180*MS else None
            self.first_exchange.setdefault(second,FillQuote(top,exchange_ms,known))
            self.top = top

    def trades(self, wall, mono, rows):
        second = wall//NS
        for row in rows:
            end, count = int(row['tradeId']), int(row['count'])
            if count <= 0:
                raise ValueError('Invalid aggregate trade count')
            if self.trade_id is not None and end-count != self.trade_id:
                self.bad(second,'trade_sequence')
                self.contaminated = True
                if end-count > self.trade_id:
                    self.audit['missing_trade_ids'] += end-count-self.trade_id
                else:
                    self.audit['overlapping_trade_messages'] += 1
                if end-count < self.trade_id:
                    continue
            self.trade_id = end
            if not self.delays['trades'].fresh(mono,int(row['ts'])):
                self.bad(second,'trade_not_ready')
                continue
            price, volume = float(row['px']), float(row['sz'])*float(self.value)
            side = {'sell':0,'buy':1}[row['side']]
            if not (math.isfinite(price*volume) and price > 0 and volume > 0):
                raise ValueError('Invalid trade price or quantity')
            self.turnover[second][side] += price*volume
            if not self.quality_only:
                self.flow.trades.append((wall,side,price,volume))
            self.audit['trade_messages'] += 1

    def funding_message(self, row):
        upcoming = int(row['fundingTime'])
        self.next_funding = int(row['ts']),upcoming
        self.funding.add(upcoming)
        if row['settState'] == 'settled' and row.get('prevFundingTime') and row.get('settFundingRate'):
            stamp = int(row['prevFundingTime'])
            rate = float(row['settFundingRate'])
            if not math.isfinite(rate) or (stamp in self.rate and self.rate[stamp] != rate):
                raise ValueError('Conflicting settled funding data')
            self.funding.add(stamp)
            self.rate[stamp] = rate

    def mark_message(self, row):
        stamp = int(row['ts'])
        price = float(row['markPx'])
        if not math.isfinite(price) or price <= 0:
            raise ValueError('Invalid mark price')
        self.mark[stamp] = price

    def finish_hour(self, hour, start, end, confirm):
        if hour < start:
            self.new_hour()
            return
        day = hour//DAY
        self.message_coverage[day] += len(self.observed_seconds)
        self.depth_coverage[day] += len(self.first_exchange)
        if self.contaminated:
            gaps = range(hour,hour+HOUR)
            valid = {}
        else:
            for second in range(hour,hour+HOUR):
                if second not in self.live_seconds and second-1 not in self.live_seconds:
                    self.turnover[second][2] = True
            if self.quality_only:
                gaps = {second for second,row in self.turnover.items() if row[2]}
            else:
                events, gaps = scan_hour(hour,self.flow.last,self.turnover,self.flow.ofi,self.flow.refills(),
                                         end,confirmations=self.confirmations if confirm else None)
                self.events.extend(events)
            valid = {second:quote for second,quote in self.first_exchange.items() if second not in gaps}
        self.gap_coverage[day] += len(gaps)
        self.coverage[day] += len(valid)
        if self.quality_only:
            self.hour_coverage[hour] = dict(usable=len(valid),received=len(self.observed_seconds),
                                            fresh=len(self.first_exchange),trade_discontinuity=self.contaminated)
        if not self.quality_only or self.keep_execution:
            self.gaps.update(gaps)
            self.quotes.update(valid)
        self.new_hour()

    def funding_per_unit(self, entry, exit):
        opening, closing = entry.exchange_ms, exit.exchange_ms
        if closing <= opening or entry.funding_time is None or entry.funding_time < opening-FUNDING_GUARD:
            return None
        first = bisect_left(self.funding_times,opening-FUNDING_GUARD)
        last = bisect_right(self.funding_times,closing+FUNDING_GUARD)
        slots = self.funding_times[first:last]
        fee = 0.
        for stamp in slots:
            if stamp-opening <= FUNDING_GUARD or closing-stamp <= FUNDING_GUARD or stamp not in self.rate:
                return None
            marks = self.mark_times
            index = bisect_left(marks,stamp)-1
            if index < 0 or stamp-marks[index] > 2*MS:
                return None
            fee += self.rate[stamp]*self.mark[marks[index]]
        return fee

    def evaluate(self, event, quality, end, *, delay=DELAY, rejection=None, holding_end=None):
        out = {}
        start = event.time+delay
        opening = self.quotes.get(start)
        buy_first = event.side == 'bid'
        for horizon in HORIZONS:
            out[str(horizon)] = None
            stop = start+horizon
            closing = self.quotes.get(stop)
            if stop >= min(end,holding_end if holding_end is not None else end):
                if rejection is not None:
                    rejection[str(horizon)] = 'outside_evaluation'
                continue
            if quality[stop-quality.start+1] != quality[event.time-quality.start]:
                if rejection is not None:
                    rejection[str(horizon)] = 'invalid_market_interval'
                continue
            if not opening or not closing:
                if rejection is not None:
                    rejection[str(horizon)] = 'missing_entry_quote' if not opening else 'missing_exit_quote'
                continue
            funding = self.funding_per_unit(opening,closing)
            if funding is None:
                if rejection is not None:
                    rejection[str(horizon)] = 'unverified_funding_or_timing'
                continue
            buy, sell = (opening.prices,closing.prices) if buy_first else (closing.prices,opening.prices)
            out[str(horizon)] = {}
            for label, (fee, slippage) in SWAP_COSTS.items():
                paid, received = buy[1]*(1+slippage),sell[0]*(1-slippage)
                reference = paid if buy_first else received
                contracts = (Decimal(str(SIZE))//(Decimal(str(reference))*self.value*self.lot))*self.lot
                units = float(contracts*self.value)
                if contracts < self.minimum or min(buy[3],sell[2]) < units:
                    out[str(horizon)][label] = None
                    if rejection is not None:
                        rejection.setdefault(str(horizon),{})[label] = (
                            'below_minimum_contracts' if contracts < self.minimum else 'insufficient_displayed_depth')
                else:
                    carry = -funding if buy_first else funding
                    out[str(horizon)][label] = (received*(1-fee)-paid*(1+fee)+carry)/reference*10000
        return out


def replay(roots, start, end, *, confirm, capture_bounds=None, quality_only=False, capture_metadata=None):
    panels = {key:Swap(key,quality_only=quality_only) for key in INSTRUMENTS}
    hour = last_wall = last_mono = anchor = None
    for root in roots:
        for wall, mono, venue, channel, raw in sealed_rows(root):
            if venue != 'okx' or (last_wall is not None and wall < last_wall):
                raise ValueError('Archives must be chronological OKX recordings with ordered receiver clocks')
            if channel == 'configuration':
                if not isinstance(raw,dict) or not raw.get('url'):
                    raise ValueError('Missing public capture configuration')
                if capture_metadata is not None:
                    configured = {item['instId'] for item in raw.get('channels',()) if 'instId' in item}
                    if raw.get('diagnostic_only') or (configured and configured != set(INSTRUMENTS)):
                        capture_metadata['diagnostic_capture'] = True
                last_mono = anchor = None
            if last_mono is not None and mono < last_mono:
                raise ValueError('Monotonic receipt clock reversed')
            if capture_bounds is not None:
                capture_bounds.setdefault('first_receipt_ns',wall)
                capture_bounds['last_receipt_ns'] = wall
            last_wall, last_mono = wall, mono
            second = wall//NS
            if second >= end:
                continue
            if hour is None:
                hour = min(start,second-second%HOUR)
            while second >= hour+HOUR:
                for panel in panels.values():
                    panel.finish_hour(hour,start,end,confirm)
                hour += HOUR
            if anchor is None or channel == 'configuration' or (channel == 'connect' and 'instruments' not in raw):
                anchor = wall-mono
            elif abs(wall-mono-anchor) > NS//4:
                raise ValueError('Receiver wall/monotonic clocks diverged; split archives at the clock step')
            if channel in ('configuration','connect','disconnect','sequence_gap'):
                for instrument in raw.get('instruments',panels):
                    panel = panels[instrument]
                    if channel == 'configuration':
                        panel.delays.clear()
                    panel.reset()
                    panel.bad(second,channel)
                continue
            if channel not in ('books','trades','funding-rate','mark-price'):
                continue
            packet = json.loads(raw)
            panel = panels[packet['arg']['instId']]
            data = packet.get('data',[])
            if not data:
                continue
            if channel == 'books':
                if len(data) != 1:
                    raise ValueError('Unexpected OKX book payload length')
                panel.book(wall,mono,packet['action'],data[0])
            elif channel == 'trades':
                panel.trades(wall,mono,data)
            elif channel == 'funding-rate' and not quality_only:
                for row in data:
                    panel.funding_message(row)
            elif channel == 'mark-price' and not quality_only:
                for row in data:
                    panel.mark_message(row)
    while hour is not None and hour < end:
        for panel in panels.values():
            panel.finish_hour(hour,start,end,confirm)
        hour += HOUR
    for panel in panels.values():
        panel.mark_times = sorted(panel.mark)
        panel.funding_times = sorted(panel.funding)
    return panels


class AccountedFills:
    def __init__(self, panel, quality, end, delay=DELAY, *, hour_bound=False):
        self.panel, self.quality, self.end, self.delay = panel,quality,end,delay
        self.hour_bound = hour_bound
        self.results = {}
        self.counts = {}

    def __call__(self, event):
        key = event.time,event.side
        if key not in self.results:
            issues = {}
            holding_end = ((event.time-1)//HOUR+1)*HOUR if self.hour_bound else None
            marks = self.panel.evaluate(event,self.quality,self.end,delay=self.delay,rejection=issues,
                                        holding_end=holding_end)
            self.results[key] = marks
            side = self.counts.setdefault(event.side,{str(horizon):Counter() for horizon in HORIZONS})
            for horizon, mark in marks.items():
                decisions = side[horizon]
                decisions['events'] += 1
                reason = issues.get(horizon)
                if isinstance(reason,str):
                    decisions[reason] += 1
                elif mark is not None:
                    blocked = reason or {}
                    for lane,value in mark.items():
                        decisions[f'{lane}:{blocked[lane] if value is None else "scored"}'] += 1
        return self.results[key]


def utc_day(text):
    day = datetime.strptime(text,'%Y-%m-%d').replace(tzinfo=timezone.utc)
    if day.year < 2024:
        raise ValueError('Dates must be 2024 or later')
    return int(day.timestamp())


def coverage_progress(panels, start, end, bounds):
    """Bound coverage from finalized hours; a later trade gap can invalidate an open hour."""
    last_receipt = bounds.get('last_receipt_ns',start*NS)//NS
    completed = min(end,max(start,last_receipt-last_receipt%HOUR))
    reports = {}
    for name,panel in panels.items():
        days, hours = {}, {}
        for day in range(start,end,DAY):
            limit = min(day+DAY,max(day,completed))
            usable = 0
            for hour in range(day,limit,HOUR):
                row = panel.hour_coverage.get(hour,dict(usable=0,received=0,fresh=0,trade_discontinuity=False))
                usable += row['usable']
                hours[datetime.fromtimestamp(hour,timezone.utc).isoformat()] = row
            unseen = day+DAY-limit
            ceiling = usable+unseen
            days[datetime.fromtimestamp(day,timezone.utc).date().isoformat()] = dict(
                completed_hours=(limit-day)//HOUR, usable_seconds=usable, remaining_seconds=unseen,
                max_possible_usable_seconds=ceiling, max_possible_coverage=ceiling/DAY,
                can_still_meet_daily_coverage=ceiling >= MIN_FRESH_SECONDS)
        reports[name] = dict(days=days,hours=hours)
    return dict(diagnostic_only=True, sealed_receipt_bounds=bounds,
                capture_missed_window_start='first_receipt_ns' in bounds and bounds['first_receipt_ns'] > start*NS,
                min_fresh_update_coverage=MIN_FRESH_SECONDS/DAY,symbols=reports)


def describe(panel, start, split, end, *, confirm=False, weak_control=False, compare_controls=False,
             delay_sensitivity=(), complete_period=True, diagnostic_capture=False, evaluate_from=None):
    evaluation_start = split if evaluate_from is None else evaluate_from
    if not start < split <= evaluation_start < end:
        raise ValueError('Calibration must precede evaluation')
    days = {day//DAY:datetime.fromtimestamp(day,timezone.utc).date().isoformat()
            for day in (*range(start,split,DAY),*range(evaluation_start,end,DAY))}
    coverage = {name:panel.coverage[day] for day,name in days.items()}
    ready = complete_period and not diagnostic_capture and all(count >= MIN_FRESH_SECONDS for count in coverage.values())
    report = dict(quote_update_seconds=coverage,
                  received_book_message_seconds={name:panel.message_coverage[day] for day,name in days.items()},
                  fresh_depth_before_gap_filter_seconds={name:panel.depth_coverage[day] for day,name in days.items()},
                  invalid_interval_seconds={name:panel.gap_coverage[day] for day,name in days.items()},
                  audit=dict(panel.audit),ready=ready,
                  reasons=[*(['capture_is_diagnostic_only'] if diagnostic_capture else []),
                           *(['capture_does_not_bracket_requested_window'] if not complete_period else []),
                           *[f'insufficient_fresh_coverage:{day}' for day,count in coverage.items()
                             if count < MIN_FRESH_SECONDS]],
                  pressure_windows=len(panel.events),confirmed_windows=len(panel.confirmations))
    if not ready:
        return report
    quality = Quality(start if evaluate_from is None else evaluation_start,end,panel.gaps)

    evaluate = AccountedFills(panel,quality,end)

    if compare_controls:
        report['variants'] = {
            variant:{control:score_sides(panel.events,confirmed,split,evaluation_start,weak,evaluate,standalone=True,costs=SWAP_COSTS)
                     for control,weak in (('no_observed_refill',False),('calibration_bottom_quartile',True))}
            for variant,confirmed in (('absorption',None),('exhaustion',panel.confirmations))
        }
        if delay_sensitivity:
            report['delay_sensitivity'] = delay_report(panel,split,end,quality,delay_sensitivity,report['variants'],
                                                       evaluate_from=evaluation_start)
    else:
        report['sides'] = score_sides(panel.events,panel.confirmations if confirm else None,
                                      split,evaluation_start,weak_control,evaluate,standalone=True,costs=SWAP_COSTS)
    report['execution_diagnostics'] = evaluate.counts
    return report


def delay_report(panel, split, end, quality, delays, references, *, evaluate_from=None):
    evaluation_start = split if evaluate_from is None else evaluate_from
    reports = {}
    for delay in delays:
        evaluate = AccountedFills(panel,quality,end,delay)
        reports[str(delay)] = {}
        for variant, confirmed in (('absorption',None),('exhaustion',panel.confirmations)):
            sides = score_sides(panel.events,confirmed,split,evaluation_start,True,evaluate,standalone=True,costs=SWAP_COSTS)
            reference = references[variant]['calibration_bottom_quartile']
            prefix = 'exhaustion' if confirmed is not None else 'pressure'
            reports[str(delay)][variant] = {
                side:dict(strong=values[f'{prefix}_with_strong_refill'],
                          comparison=values['scores'][str(PRIMARY)],
                          common_matched_ledger=common_delay(reference[side]['ledger'],values['ledger']))
                for side,values in sides.items()
            }
        reports[str(delay)]['execution_diagnostics'] = evaluate.counts
    return reports


def common_delay(reference, delayed):
    horizon = str(PRIMARY)
    previous = {(row['candidate']['time'],row['control']['time']):row for row in reference}
    common = {lane:[] for lane in SWAP_COSTS}
    for row in delayed:
        prior = previous[row['candidate']['time'],row['control']['time']]
        observations = [item[field][horizon] for item in (prior,row) for field in ('treated_bps','control_bps')]
        if any(item is None for item in observations):
            continue
        for lane in SWAP_COSTS:
            before, before_control, after, after_control = (item[lane] for item in observations)
            if None not in (before,before_control,after,after_control):
                common[lane].append((after-before,(after-after_control)-(before-before_control)))
    return {lane:dict(pairs=len(rows),
                      strong_net_change_bps=mean(strong for strong,_ in rows) if rows else None,
                      paired_delta_change_bps=mean(delta for _,delta in rows) if rows else None)
            for lane,rows in common.items()}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('roots',type=Path,nargs='+',help='Sealed capture roots, oldest first')
    cli.add_argument('--start',type=utc_day,required=True)
    cli.add_argument('--calibration-days',type=int,default=1)
    cli.add_argument('--end',type=utc_day,required=True)
    cli.add_argument('--confirm-exhaustion',action='store_true')
    cli.add_argument('--weak-refill-control',action='store_true',
                     help='Match the calibration bottom quartile instead of no observed refill')
    cli.add_argument('--compare-refill-controls',action='store_true',
                     help='Compare both entry rules and controls, with delayed execution diagnostics')
    cli.add_argument('--coverage-only',action='store_true',
                     help='Audit complete sealed hours and bound remaining daily coverage without scoring')
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    split = args.start+args.calibration_days*DAY
    if args.calibration_days < 1 or split >= args.end:
        cli.error('At least one distinct UTC day each for calibration and evaluation')
    if args.compare_refill_controls and (args.confirm_exhaustion or args.weak_refill_control):
        cli.error('The comparison already includes both flags; do not combine them')
    if args.coverage_only and (args.compare_refill_controls or args.confirm_exhaustion or args.weak_refill_control):
        cli.error('Coverage-only does not estimate strategies')
    bounds, metadata = {}, {}
    panels = replay(args.roots,args.start,args.end,confirm=args.confirm_exhaustion or args.compare_refill_controls,
                    capture_bounds=bounds,quality_only=args.coverage_only,capture_metadata=metadata)
    if args.coverage_only:
        output = coverage_progress(panels,args.start,args.end,bounds)
        output.update(start_utc_second=args.start,end_utc_second=args.end,archives=[str(root.resolve()) for root in args.roots],
                      **metadata)
        args.output.write_text(json.dumps(output,indent=2,allow_nan=False)+'\n',encoding='utf-8')
        return
    complete_period = ('first_receipt_ns' in bounds and bounds['first_receipt_ns'] <= args.start*NS
                       and bounds['last_receipt_ns'] >= args.end*NS)
    reports = {key:describe(panel,args.start,split,args.end,confirm=args.confirm_exhaustion,
                            weak_control=args.weak_refill_control,compare_controls=args.compare_refill_controls,
                            delay_sensitivity=DELAY_SENSITIVITY if args.compare_refill_controls else (),
                            complete_period=complete_period,**metadata)
               for key,panel in panels.items()}
    output = dict(archives=[str(root.resolve()) for root in args.roots],
                  start_utc_second=args.start,calibration_end_utc_second=split,end_utc_second=args.end,
                  evaluation_start_utc_second=split,primary_seconds=PRIMARY,
                  all_horizons_seconds=HORIZONS,delay_seconds=DELAY,
                  costs=SWAP_COSTS, contract_specs=SPECS, min_fresh_update_coverage=MIN_FRESH_SECONDS/DAY,
                  sealed_receipt_bounds=bounds,capture_brackets_period=complete_period,
                  control_style='calibration_bottom_quartile' if args.weak_refill_control else 'no_observed_refill',
                  confirmation=args.confirm_exhaustion, symbols=reports, **metadata)
    if args.compare_refill_controls:
        output.pop('control_style')
        output.pop('confirmation')
        output['delay_diagnostics_seconds'] = DELAY_SENSITIVITY
    args.output.write_text(json.dumps(output,indent=2,allow_nan=False)+'\n',encoding='utf-8')


if __name__ == '__main__':
    main()
