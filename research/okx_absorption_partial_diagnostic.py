"""Run a non-promotable sensitivity check on already complete OKX archive hours."""
import argparse
import json
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from datetime import datetime, timezone
from pathlib import Path

from absorption_event_review import DELAY, HORIZONS, PRIMARY, Quality, score_sides
from flow_history_download import read_downloads
from flow_history_replay import verify_downloads
from okx_absorption_archive import feed_hour, keys, read_rates
from okx_absorption_review import AccountedFills, DAY, HOUR, SPECS, SWAP_COSTS, Swap, utc_day


MIN_COVERAGE = .90


def diagnostic(root, symbol, start, split, end, manifest, funding):
    rate, digest = read_rates(funding,symbol,start,end)
    events, confirmed, quotes, marks = [],[],{},{}
    gaps = set()
    settlements = set(rate)
    calendar = Counter()
    kept = []
    missing = []
    coverage_rejects = []
    for hour in range(start,end,HOUR):
        paths = keys(symbol,hour)
        if any(manifest.get(name,{}).get('status') != 'ok' or not (root/'raw'/name).is_file() for name in paths):
            missing.append(hour)
            gaps.update(range(hour,hour+HOUR))
            continue
        verify_downloads(root,paths)
        panel = Swap(symbol)
        feed_hour(panel,[root/'raw'/name for name in paths],hour,hour,hour+HOUR)
        fresh = panel.coverage[hour//DAY]
        if fresh < MIN_COVERAGE*HOUR:
            coverage_rejects.append(dict(hour_utc_second=hour, fresh_seconds=fresh, audit=dict(panel.audit)))
            gaps.update(range(hour,hour+HOUR))
            continue
        kept.append(dict(hour_utc_second=hour,fresh_seconds=fresh,
                         sha256={name:manifest[name]['sha256'] for name in paths}))
        calendar['calibration_hours' if hour < split else 'evaluation_hours'] += 1
        events.extend(panel.events)
        confirmed.extend(panel.confirmations)
        quotes.update(panel.quotes)
        gaps.update(panel.gaps)
        marks.update(panel.mark)
        settlements.update(panel.funding)
    result = dict(kept_hours=kept,missing_hours=missing,rejected_hours=coverage_rejects,
                  session_counts=dict(calendar),official_funding_sha256=digest)
    if not calendar['calibration_hours'] or not calendar['evaluation_hours']:
        return symbol,result
    combined = Swap(symbol)
    combined.quotes,combined.mark,combined.rate = quotes,marks,rate
    combined.funding_times,combined.mark_times = sorted(settlements),sorted(marks)
    accounted = AccountedFills(combined,Quality(start,end,gaps),end,hour_bound=True)

    result['variants'] = {
        variant:{control:score_sides(events,selected,split,split,weak,accounted,standalone=True,costs=SWAP_COSTS)
                 for control,weak in (('no_observed_refill',False),('calibration_bottom_quartile',True))}
        for variant,selected in (('absorption',None),('exhaustion',confirmed))
    }
    result['execution_diagnostics'] = accounted.counts
    return symbol,result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('root',type=Path)
    cli.add_argument('--start',type=utc_day,required=True)
    cli.add_argument('--end',type=utc_day,required=True)
    cli.add_argument('--output',type=Path,required=True)
    args = cli.parse_args()
    split = args.start+DAY
    if split >= args.end:
        cli.error('Use separate UTC dates for calibration and evaluation')
    manifest = read_downloads(args.root)
    with ProcessPoolExecutor(len(SPECS)) as workers:
        jobs = [workers.submit(diagnostic,args.root,symbol,args.start,split,args.end,manifest,
                               args.root/f'{symbol}-www.okx.com-settled-funding.json') for symbol in SPECS]
        panels = dict(job.result() for job in jobs)
    report = dict(diagnostic_only=True,promotion_allowed=False,
                  source='CryptoHFTData complete verified hours + OKX actual funding history',
                  limitation='Missing hours can bias selection; reconstructed independently; no account PnL.',
                  start_utc_second=args.start,evaluation_start_utc_second=split,end_utc_second=args.end,
                  calibration_end_utc_second=split,primary_seconds=PRIMARY,horizons_seconds=HORIZONS,
                  delay_seconds=DELAY,costs=SWAP_COSTS,min_hourly_quote_fraction=MIN_COVERAGE,
                  symbols=panels)
    args.output.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')


if __name__ == '__main__':
    main()
