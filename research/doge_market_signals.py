"""DOGE spot signals built from other assets, futures and spot venues."""
import math
from itertools import accumulate

from doge_replay_inputs import DAY, LAST_SIGNAL, Signal, bids_value, bps, mid, spread, start_microseconds
from hf_pressure import rows
from independent_round import book


def big_assets(day, home):
    bitcoin = book(day, 'BTCUSDT')
    ether = book(day, 'ETHUSDT')
    rules = {number: Signal() for number in range(83, 87)}
    for sec in range(74, LAST_SIGNAL, 15):
        start, half = sec-60, sec-30
        local, btc, eth = [tuple(market[point] for point in (start, half, sec))
                           for market in (home, bitcoin, ether)]
        if not (all(local) and all(btc) and all(eth)):
            continue
        btc_early, btc_late = (bps(mid(btc[1]), mid(btc[0])), bps(mid(btc[2]), mid(btc[1])))
        eth_early, eth_late = (bps(mid(eth[1]), mid(eth[0])), bps(mid(eth[2]), mid(eth[1])))
        local_total = bps(mid(local[2]), mid(local[0]))
        local_late = bps(mid(local[2]), mid(local[1]))
        btc_total, eth_total = btc_early+btc_late, eth_early+eth_late
        rules[83].observe(sec, eth_early >= 15 and abs(btc_early) <= 5 and btc_late >= 15
                          and local_total <= 8, eth_early, btc_late, local_total)
        rules[84].observe(sec, eth_total-btc_total >= 25 and abs(btc_total) <= 10 and local_total <= -10,
                          eth_total-btc_total, btc_total, local_total)
        rules[85].observe(sec, btc_early*eth_early <= -100 and min(abs(btc_early), abs(eth_early)) >= 10
                          and min(btc_late, eth_late) >= 10 and local_late <= 5,
                          btc_late, eth_late, local_late)
        before = (spread(btc[0]), spread(eth[0]))
        after = (spread(btc[2]), spread(eth[2]))
        widening = tuple(new-old for new, old in zip(after, before, strict=True))
        rules[86].observe(sec, min(widening) >= .2 and all(new >= 2*old for new, old in zip(after, before))
                          and local_total <= -15, *widening, local_total)
    return rules


def perp_tickers(day):
    origin = start_microseconds(day)
    tape = [None] * DAY
    last_received = -1
    for raw in rows(day, 'DOGEUSDT', 'derivative_ticker', 'bybit'):
        received = int(raw[3])
        if received < last_received:
            raise ValueError('Perpetual ticker receipt timestamps are out of order')
        last_received = received
        sec = (received-origin) // 1_000_000
        if not 0 <= sec < DAY:
            continue
        values = tuple(float(value) for value in raw[7:11] if value)
        tape[sec] = (received, values if len(values) == 4 and
                     all(math.isfinite(value) and value > 0 for value in values) else None)
    recent = None
    for sec, update in enumerate(tape):
        if update is not None:
            recent = update
        tape[sec] = recent[1] if recent and origin+(sec+1)*1_000_000-recent[0] <= 2_000_000 else None
    return tape


def traded_notional(day):
    origin = start_microseconds(day)
    values, gaps = [0.] * DAY, [0] * DAY
    prior_id = None
    last_received = -1
    for raw in rows(day, 'DOGEUSDT', 'trades'):
        received = int(raw[3])
        if received < last_received:
            raise ValueError('Trade receipt timestamps are out of order')
        last_received = received
        sec = (received-origin) // 1_000_000
        if not 0 <= sec < DAY:
            continue
        trade_id = int(raw[4])
        if prior_id is not None and trade_id != prior_id+1:
            gaps[sec] += 1
        prior_id = trade_id
        values[sec] += float(raw[6])*float(raw[7])
    return list(accumulate(values, initial=0.)), list(accumulate(gaps, initial=0))


def derivative_signals(day, home):
    tickers = perp_tickers(day)
    perp = book(day, 'DOGEUSDT', 'binance-futures')
    notional, gaps = traded_notional(day)
    rules = {number: Signal() for number in range(87, 95)}
    for sec in range(74, LAST_SIGNAL, 15):
        start = sec-60
        if not home[start] or not home[sec]:
            continue
        local = bps(mid(home[sec]), mid(home[start]))
        now, before, half = tickers[sec], tickers[start], tickers[sec-30]
        if now:
            interest, last, index, mark = now
            premium = bps(last, mark)
            rules[87].observe(sec, premium <= -12 and abs(local) <= 10, premium, local)
        if now and before:
            old_interest, old_last, old_index, old_mark = before
            mark_change, index_change = bps(mark, old_mark), bps(index, old_index)
            if half:
                first, later = bps(half[1], old_last), bps(last, half[1])
                rules[88].observe(sec, abs(mark_change) <= 5 and first <= -20 and later >= 15,
                                  first, later, mark_change)
            rules[89].observe(sec, index_change >= 20 and max(mark_change, local) <= 8,
                              index_change, mark_change, local)
            expansion = mark_change - index_change
            rules[90].observe(sec, expansion >= 12 and max(index_change, local) <= -10,
                              expansion, index_change, local)
            delta = interest / old_interest - 1
            recovering = premium - bps(old_last, old_mark)
            rules[91].observe(sec, delta <= -.008 and recovering >= 6 and local <= -15,
                              delta, recovering, local)
            turnover = notional[sec+1] - notional[start+1]
            if turnover >= 500 and gaps[sec+1] == gaps[start+1]:
                ratio = (interest-old_interest) * last / turnover
                rules[92].observe(sec, ratio >= 4 and index_change >= 8 and local <= 5,
                                  ratio, index_change, local)
        if perp[sec]:
            bid_ratio = perp[sec][3] / perp[sec][4]
            local_ratio = home[sec][3] / home[sec][4]
            rules[94].observe(sec, bid_ratio >= 2 and local_ratio <= .5 and local <= -10,
                              bid_ratio, local_ratio, local)
            if perp[start]:
                then, now = spread(perp[start]), spread(perp[sec])
                move = bps(mid(perp[sec]), mid(perp[start]))
                rules[93].observe(sec, then >= 2 and now <= .5*then and move >= 12 and local <= 5,
                                  then, now, move)
    return rules


def dispersion(quotes):
    mids = tuple(map(mid, quotes))
    return bps(max(mids), min(mids))


def weighted_gap(quotes):
    mids = tuple(map(mid, quotes))
    bid_values = tuple(map(bids_value, quotes))
    weighted = sum(price*value for price, value in zip(mids, bid_values)) / sum(bid_values)
    return bps(weighted, sum(mids)/3), bps(weighted, quotes[0][2])


def venue_signals(day, home):
    bybit = book(day, 'DOGEUSDT', 'bybit-spot')
    okx = book(day, 'DOGE-USDT', 'okex')
    markets = (home, bybit, okx)
    rules = {number: Signal() for number in range(95, 101)}
    for sec in range(74, LAST_SIGNAL, 15):
        now, past = (tuple(market[point] for market in markets) for point in (sec, sec-60))
        if not (all(now) and all(past)):
            continue
        local, bybit_move, okx_move = (bps(mid(new), mid(old)) for new, old in zip(now, past))
        spread_before, spread_now = dispersion(past), dispersion(now)
        away_move = (bybit_move + okx_move)/2
        rules[95].observe(sec, spread_before >= 20 and spread_now <= .5*spread_before and
                          away_move >= 10 and local <= 5, spread_before, spread_now, away_move)
        rules[96].observe(sec, okx_move >= 25 and max(abs(local), abs(bybit_move)) <= 8,
                          okx_move, bybit_move, local)
        away_ratio = sum(map(bids_value, now[1:])) / sum(map(bids_value, past[1:]))
        local_ratio = bids_value(now[0]) / bids_value(past[0])
        rules[97].observe(sec, away_ratio >= 1.5 and local_ratio <= 2/3 and spread_now <= 10,
                          away_ratio, local_ratio, spread_now)
        half = tuple(market[sec-30] for market in markets)
        if all(half):
            early = tuple(bps(mid(middle), mid(old)) for middle, old in zip(half, past))
            late = tuple(bps(mid(new), mid(middle)) for new, middle in zip(now, half))
            early_gap = min(early[1:])-early[0]
            late_gap = late[0]-max(late[1:])
            rules[98].observe(sec, min(early_gap, late_gap) >= 8 and local < 0, early_gap, late_gap, local)
        path = [tuple(market[point] for market in markets) for point in range(sec-60, sec+1, 10)]
        if all(all(quotes) for quotes in path):
            switches = 0
            leader = min(range(3), key=lambda idx: path[0][idx][2])
            for quotes in path[1:]:
                best = min(range(3), key=lambda idx: quotes[idx][2])
                if quotes[leader][2] > quotes[best][2]:
                    leader = best
                    switches += 1
            rules[99].observe(sec, switches >= 4 and local <= -15, switches, local)
        recent = tuple(market[sec-15] for market in markets)
        if all(recent):
            weighted, to_entry = weighted_gap(now)
            old_weighted, old_to_entry = weighted_gap(recent)
            rules[100].observe(sec, min(weighted, old_weighted) >= 4 and min(to_entry, old_to_entry) >= 10,
                               weighted, to_entry, old_to_entry)
    return rules
