"""DOGE quote paths, visible depth and transaction-level hypotheses."""
from itertools import accumulate, pairwise
from statistics import median, quantiles

from depth_shape import snapshots
from doge_replay_inputs import DAY, LAST_SIGNAL, Signal, bps, mid, start_microseconds
from hf_pressure import rows


def quote_paths(day, home):
    rules = {number: Signal() for number in range(65, 71)}
    for sec in range(74, LAST_SIGNAL, 15):
        window = home[sec-60:sec+1:5]
        if not all(window):
            continue
        prices = tuple(bps(mid(quote), mid(window[0])) for quote in window)
        change = prices[-1]
        low, high = min(prices), max(prices)
        returns = [later - earlier for earlier, later in pairwise(prices)]
        signs = [value > 0 for value in returns if value]
        reversals = sum(left != right for left, right in pairwise(signs))
        quartile, _, upper = quantiles(returns, method='inclusive')
        dwell = sum(price <= low + .25 * (high - low) for price in prices)
        length = sum(map(abs, returns))
        tail_range = max(prices[-5:]) - min(prices[-5:])
        early, middle, late = prices[4], prices[8] - prices[4], change - prices[8]

        rules[65].observe(sec, change <= -15 and reversals >= 7, change, reversals)
        rules[66].observe(sec, high-low >= 30 and dwell >= 8 and change-low >= 5,
                          high-low, dwell, change-low)
        rules[67].observe(sec, change <= -10 and median(returns) > 0 and quartile < 0
                          and -quartile >= 2 * upper, quartile, upper, change)
        rules[68].observe(sec, change >= 22 and change >= .75 * length, change, length)
        rules[69].observe(sec, change >= 20 and tail_range <= 4 and high-change <= 3,
                          change, tail_range, high-change)
        rules[70].observe(sec, early <= -20 and abs(middle) <= -.5 * early and late >= 10,
                          early, middle, late)
    return rules


def sweep_ask(prices, sizes, budget):
    remaining = budget
    units = 0.
    for price, size in zip(prices, sizes, strict=True):
        cost = min(remaining, price * size)
        units += cost / price
        remaining -= cost
        if remaining <= 1e-9:
            return budget / units
    return None


def sweep_bid(prices, sizes, quantity):
    remaining = quantity
    cash = 0.
    for price, size in zip(prices, sizes, strict=True):
        units = min(remaining, size)
        cash += units * price
        remaining -= units
        if remaining <= 1e-9:
            return cash / quantity
    return None


def sell_penalty(snapshot):
    bids, bid_sizes, asks, _ = snapshot
    reference = (bids[0] + asks[0]) / 2
    small = sweep_bid(bids, bid_sizes, 25 / reference)
    large = sweep_bid(bids, bid_sizes, 250 / reference)
    return bps(small, large) if small and large else None


def depth_signals(day, home):
    rules = {number: Signal() for number in range(71, 77)}
    history = snapshots(day)

    def aligned(sec):
        sample = history[(sec + 1) // 5]
        if sample and sample[1] and home[sec]:
            bids, _, asks, _ = sample[1]
            if (bids[0], asks[0]) == home[sec][1:3]:
                return sample[1]
        return None

    for sec in range(74, LAST_SIGNAL, 15):
        current = aligned(sec)
        if current is None:
            continue
        bids, bid_qty, asks, ask_qty = current
        large_buy = sweep_ask(asks, ask_qty, 250)
        if home[sec-60] and large_buy:
            premium = bps(large_buy, sweep_ask(asks, ask_qty, 25))
            change = bps(mid(home[sec]), mid(home[sec-60]))
            rules[71].observe(sec, premium >= 3 and abs(change) <= 10, premium, change)

        old = aligned(sec - 30)
        if old is None:
            continue
        old_bids, old_bid_qty, old_asks, old_ask_qty = old
        change = bps(mid(home[sec]), mid(home[sec-30]))
        before, after = sell_penalty(old), sell_penalty(current)
        if before is not None and after is not None:
            rules[72].observe(sec, before >= 2 and after <= .5 * before and change <= -15,
                              before, after, change)
        gaps = [right - left for left, right in pairwise(asks)]
        previous_gaps = [right - left for left, right in pairwise(old_asks)]
        # Ties are ordinary one-tick spacing, not a migrating gap.
        new_gap = gaps[-1] - max(gaps[:-1]) > 1e-8
        old_gap = previous_gaps[0] - max(previous_gaps[1:]) > 1e-8
        near_gap = bps(old_asks[1], old_asks[0])
        far_gap = bps(asks[-1], asks[-2])
        rules[73].observe(sec, new_gap and old_gap and far_gap >= 1 and change <= -10,
                          near_gap, far_gap, change)
        bid_retained = sum(bid_qty[:3]) / sum(old_bid_qty[:3])
        ask_retained = sum(ask_qty[:3]) / sum(old_ask_qty[:3])
        rules[74].observe(sec, max(bid_retained, ask_retained) <= .6 and change <= -15,
                          bid_retained, ask_retained, change)
        total, old_total = sum(bid_qty), sum(old_bid_qty)
        near_change = sum(bid_qty[:2]) / total - sum(old_bid_qty[:2]) / old_total
        rules[75].observe(sec, bids[0] == old_bids[0] and abs(total / old_total - 1) <= .2
                          and near_change >= .25, near_change, total / old_total)
        middle = aligned(sec - 15)
        if middle:
            index = max(range(len(old_bids)), key=old_bid_qty.__getitem__)
            wall, original = old_bids[index], old_bid_qty[index]
            middle_quantity = dict(zip(middle[0], middle[1], strict=True)).get(wall, 0.)
            end_quantity = dict(zip(bids, bid_qty, strict=True)).get(wall, 0.)
            preserved = min(middle_quantity, end_quantity) / original
            rules[76].observe(sec, preserved >= .8 and change <= -15, preserved, change, wall)
    return rules


def trade_tape(day):
    origin = start_microseconds(day)
    columns = ('count', 'flips', 'first_flip', 'gaps', 'buy', 'sell',
               'buy_with_quote', 'sell_with_quote', 'below_ask', 'below_bid')
    data = {field: [0.] * DAY for field in columns}
    quote_rows = iter(rows(day, 'DOGEUSDT', 'quotes'))
    next_quote = next(quote_rows, None)
    previous_quote = None
    previous_id = previous_side = None
    last_received = -1
    for raw in rows(day, 'DOGEUSDT', 'trades'):
        received = int(raw[3])
        sec = (received - origin) // 1_000_000
        if not 0 <= sec < DAY:
            continue
        if received < last_received:
            raise ValueError('Trade receipt timestamps are out of order')
        last_received = received
        trade_id = int(raw[4])
        if previous_id is not None and trade_id != previous_id + 1:
            data['gaps'][sec] += 1
        side = raw[5]
        if side not in ('buy', 'sell'):
            raise ValueError('Unexpected trade side')
        price, quantity = float(raw[6]), float(raw[7])
        amount = price * quantity
        flipped = previous_side is not None and previous_side != side
        if data['count'][sec] == 0 and flipped:
            data['first_flip'][sec] = 1
        data['count'][sec] += 1
        data['flips'][sec] += flipped
        data[side][sec] += amount
        previous_id, previous_side = trade_id, side

        while next_quote is not None and int(next_quote[3]) < received:
            previous_quote = next_quote
            next_quote = next(quote_rows, None)
        if previous_quote is None or received - int(previous_quote[3]) > 2_000_000:
            continue
        ask, bid = float(previous_quote[5]), float(previous_quote[6])
        if not 0 < bid < ask:
            continue
        data[side + '_with_quote'][sec] += amount
        if side == 'buy' and price < ask:
            data['below_ask'][sec] += amount
        if side == 'sell' and price < bid:
            data['below_bid'][sec] += amount
    data['value_squares'] = [(buy + sell) ** 2 for buy, sell in zip(data['buy'], data['sell'], strict=True)]
    return data


def trade_signals(day, home):
    rules = {number: Signal() for number in range(77, 83)}
    tape = trade_tape(day)
    cumulative = {name: list(accumulate(values, initial=0.)) for name, values in tape.items()}

    def total(name, start, end):
        return cumulative[name][end+1] - cumulative[name][start+1]

    for sec in range(119, LAST_SIGNAL, 10):
        start = sec - 60
        if not home[sec] or start < 0 or not home[start] or total('gaps', start, sec):
            continue
        count = total('count', start, sec)
        if count < 30:
            continue
        buy, sell = total('buy', start, sec), total('sell', start, sec)
        activity = buy + sell
        if activity <= 0:
            continue
        buy_share = buy / activity
        change = bps(mid(home[sec]), mid(home[start]))
        late_share = total('count', sec-10, sec) / count
        rules[77].observe(sec, late_share >= .5 and buy_share >= .55 and abs(change) <= 10,
                          late_share, buy_share, change)
        first_trade_sec = next(i for i in range(start+1, sec+1) if tape['count'][i])
        flips = total('flips', start, sec) - tape['first_flip'][first_trade_sec]
        rules[78].observe(sec, flips >= .6 * count and .4 <= buy_share <= .6 and change <= -15,
                          flips / count, buy_share, change)

        old = sec - 120
        if old >= 0 and home[old] and not total('gaps', old, start) and total('count', old, start) >= 30:
            old_sell_net = total('sell', old, start) - total('buy', old, start)
            sell_net = sell - buy
            past_change = bps(mid(home[start]), mid(home[old]))
            if min(old_sell_net, sell_net) >= 1000 and past_change < 0 and change < 0:
                old_impact, new_impact = -past_change / old_sell_net, -change / sell_net
                rules[79].observe(sec, old_impact * .5 >= new_impact and past_change+change <= -15,
                                  old_impact, new_impact, past_change+change)
        supported = total('sell_with_quote', start, sec)
        if supported >= 1000 and home[sec-5]:
            through = total('below_bid', start, sec) / supported
            bounce = bps(mid(home[sec]), mid(home[sec-5]))
            rules[80].observe(sec, through >= .25 and change <= -15 and bounce >= 8, through, change, bounce)
        concentration = total('value_squares', start, sec) / activity ** 2
        rules[81].observe(sec, concentration <= .045 and buy_share <= .4 and change <= -15,
                          concentration, buy_share, change)
        supported = total('buy_with_quote', start, sec)
        if supported >= 1000:
            inside = total('below_ask', start, sec) / supported
            rules[82].observe(sec, buy_share >= .6 and inside >= .3 and change >= 8,
                              inside, buy_share, change)
    return rules
