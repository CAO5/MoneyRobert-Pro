"""Score the frozen OKX vendor holdout separately from the official forward review."""
import argparse
import json
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

from absorption_pair_audit import summarize
from okx_absorption_review import (DAY, DELAY, DELAY_SENSITIVITY, HORIZONS, MIN_FRESH_SECONDS, PRIMARY, SPECS,
                                   SWAP_COSTS, describe, utc_day)
from okx_tardis_audit import SLICE_MINUTES, archive_path, replay_native


CALIBRATION_DAY = '2026-10-01'
EVALUATION_DAY = '2026-11-01'


def require_daily_archive(root):
    for minute in range(0,1440,SLICE_MINUTES):
        path = archive_path(root,minute)
        if not path.is_file():
            raise ValueError(f'Missing frozen source slice: {path}')


def read_calibration(root, start):
    panels, _, metadata = replay_native(root,start,24,collect_events=True)
    fields = ('coverage','message_coverage','depth_coverage','gap_coverage','events','confirmations','audit')
    return {name:{field:getattr(panel,field) for field in fields} for name,panel in panels.items()},metadata


def combine_and_score(panel, calibration, start, evaluation_start):
    split, end = start+DAY,evaluation_start+DAY
    if (any(not start <= event.time < split for event in (*calibration['events'],*calibration['confirmations']))
            or any(not evaluation_start <= event.time < end for event in (*panel.events,*panel.confirmations))):
        raise ValueError('Events are outside their fixed date roles')
    for field in ('coverage','message_coverage','depth_coverage','gap_coverage'):
        target = getattr(panel,field)
        if target.keys() & calibration[field].keys():
            raise ValueError('Calibration and evaluation dates overlap')
        target.update(calibration[field])
    evaluation_audit = dict(panel.audit)
    panel.events = calibration['events']+panel.events
    panel.confirmations = calibration['confirmations']+panel.confirmations
    panel.mark_times = sorted(panel.mark)
    panel.funding_times = sorted(panel.funding)
    scored = describe(panel,start,split,end,compare_controls=True,delay_sensitivity=DELAY_SENSITIVITY,
                      evaluate_from=evaluation_start)
    scored.update(calibration_audit=dict(calibration['audit']),evaluation_audit=evaluation_audit)
    return scored


def run(calibration_root, evaluation_root):
    for root in (calibration_root,evaluation_root):
        require_daily_archive(root)
    start, evaluation_start = utc_day(CALIBRATION_DAY),utc_day(EVALUATION_DAY)
    with ProcessPoolExecutor(max_workers=1) as workers:
        pending = workers.submit(read_calibration,calibration_root,start)
        holdout, _, evaluation_metadata = replay_native(evaluation_root,evaluation_start,24,
                                                        collect_events=True,keep_execution=True)
        calibration, calibration_metadata = pending.result()
    return dict(diagnostic_only=True,promotion_allowed=False,
                limitation='One month-open holdout and vendor receipt clock; not local execution or a replacement for official capture.',
                start_utc_second=start,calibration_end_utc_second=start+DAY,
                evaluation_start_utc_second=evaluation_start,end_utc_second=evaluation_start+DAY,
                primary_seconds=PRIMARY,all_horizons_seconds=HORIZONS,delay_seconds=DELAY,
                delay_diagnostics_seconds=DELAY_SENSITIVITY,costs=SWAP_COSTS,contract_specs=SPECS,
                min_fresh_update_coverage=MIN_FRESH_SECONDS/DAY,calibration_source=calibration_metadata,
                evaluation_source=evaluation_metadata,
                symbols={name:combine_and_score(panel,calibration[name],start,evaluation_start)
                         for name,panel in holdout.items()})


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('calibration_root',type=Path)
    cli.add_argument('evaluation_root',type=Path)
    cli.add_argument('--output',type=Path,required=True)
    cli.add_argument('--pairs-output',type=Path,required=True)
    args = cli.parse_args()
    data = run(args.calibration_root,args.evaluation_root)
    pair_audit = summarize(data)
    for path, report in ((args.output,data),(args.pairs_output,pair_audit)):
        path.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
