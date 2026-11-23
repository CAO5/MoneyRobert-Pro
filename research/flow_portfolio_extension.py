"""Replay the fixed portfolio rules on nine additional historical month starts."""
import csv
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path
from time import monotonic

from flow_portfolio import SCREEN_DAYS, study
from flow_sector import write
from hf_pressure import CACHE


DAYS = ('2024-01-01', '2024-04-01', '2024-11-01', '2025-01-01', '2025-07-01', '2025-12-01',
        '2026-02-01', '2026-08-01', '2026-10-01')


def main():
    if set(DAYS).intersection(SCREEN_DAYS):
        raise ValueError('The historical extension overlaps the original screen')
    CACHE.mkdir(exist_ok=True)
    started = monotonic()
    with ProcessPoolExecutor(5) as pool:
        pending = {pool.submit(study, day): day for day in DAYS}
        runs = {}
        for future in as_completed(pending):
            day = pending[future]
            runs[day] = future.result()
            print(f'{day} replay complete', flush=True)
    root = Path(__file__).resolve().parent
    for suffix, records in zip(('results', 'trades', 'episodes'), zip(*(runs[day] for day in DAYS))):
        with (root / f'flow_portfolio_{suffix}.csv').open(newline='', encoding='utf-8') as source:
            header = next(csv.reader(source))
        write(f'flow_portfolio_extension_{suffix}.csv', header, records)
    print(f'{len(runs)} additional days completed in {monotonic() - started:.1f}s', flush=True)


if __name__ == '__main__':
    main()
