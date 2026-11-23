"""Check official claim deals against the separately received public feed, by trade ID."""
import csv
import gzip
import math
from collections import defaultdict

from hf_pressure import rows
from leveraged_claims import DAY, DAYS, ROOT, archive_buy, input_file, midnight


def reconcile(day, side):
    origin = midnight(day)
    recorded, official = {}, {}
    for row in rows(day, f'DOGE3{side}_USDT', 'trades', 'gate-io'):
        stamp, received = int(row[2]) / 1e6, int(row[3]) / 1e6
        if origin <= stamp < origin + DAY:
            ident = int(row[4])
            value = (float(row[6]), float(row[7]), row[5], stamp, received)
            if ident in recorded and recorded[ident][:3] != value[:3]:
                raise ValueError('Conflicting duplicate receipt')
            recorded.setdefault(ident, value)
    with gzip.open(input_file(day[:7].replace('-', ''), side), 'rt') as stream:
        for row in csv.reader(stream):
            stamp = float(row[0])
            if origin <= stamp < origin + DAY:
                ident = int(row[1])
                value = (float(row[2]), float(row[3]), int(row[4]), stamp)
                if ident in official and official[ident] != value:
                    raise ValueError('Conflicting official prints')
                official[ident] = value
    both = recorded.keys() & official.keys()
    unmatched = defaultdict(list)
    for ident in official.keys() - both:
        unmatched[int(official[ident][3] * 100000)].append(ident)
    links = {}
    for ident in recorded.keys() - both:
        price, amount, _, stamp, _ = recorded[ident]
        slot = int(stamp * 100000)
        matches = [candidate for near in range(slot - 1, slot + 2) for candidate in unmatched.get(near, ())
                   if abs(official[candidate][3] - stamp) <= .000010 and price == official[candidate][0]
                   and abs(amount - official[candidate][1]) <= .000001]
        if len(matches) == 1:
            links[ident] = matches[0]
    if len(links) != len(set(links.values())):
        raise ValueError('Ambiguous fallback match')
    offsets = sorted({remote - local for local, remote in links.items()})
    max_amount_difference = 0.
    raw_opposite = side_errors = size_errors = stamp_errors = price_errors = late = negative = 0
    for local, remote in ({ident: ident for ident in both} | links).items():
        tick, original = recorded[local], official[remote]
        raw_opposite += (tick[2] == 'buy') != (original[2] == 2)
        side_errors += (tick[2] == 'buy') != archive_buy(original[2])
        size_errors += not math.isclose(tick[1], original[1], rel_tol=1e-8, abs_tol=1e-8)
        max_amount_difference = max(max_amount_difference, abs(tick[1] - original[1]))
        price_errors += not math.isclose(tick[0], original[0], rel_tol=1e-8, abs_tol=1e-10)
        stamp_errors += abs(tick[3] - original[3]) > .01
        late += tick[4] - original[3] > 60
        negative += tick[4] < original[3]
    return (day, side, len(official), len(recorded), len(both), len(links), ';'.join(map(str, offsets)),
            raw_opposite, side_errors, price_errors, size_errors, max_amount_difference, stamp_errors, late, negative)


if __name__ == '__main__':
    with (ROOT / 'claim_feed_check_results.csv').open('w', newline='') as stream:
        writer = csv.writer(stream)
        writer.writerow(('day', 'side', 'archive', 'receiver', 'matched_ids', 'matched_time_price_size', 'fallback_id_offsets', 'raw_label_opposite', 'normalized_errors', 'price_errors', 'amount_errors', 'max_amount_difference', 'timestamp_diff_gt_10ms', 'arrived_after_60s', 'negative_clock_gap'))
        for day in tuple(f'2022-{month:02}-01' for month in (1, 4, 7, 10)) + DAYS:
            for side in ('L', 'S'):
                record = reconcile(day, side)
                writer.writerow(record)
                print(*record, flush=True)
