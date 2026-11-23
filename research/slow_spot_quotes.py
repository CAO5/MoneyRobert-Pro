"""Replay the low-turnover signals on the archived second-by-second spot quotes."""
import argparse
import json
import math
import tempfile
import urllib.request
from pathlib import Path

from flow_forward import DAY, read_days
from slow_spot_replay import BAR, RULES, STEPS, SYMBOLS, Bar, decisions, epoch, exit_reason, load_month


START, END = epoch('2026-10-20'), epoch('2026-11-19')


def recent_bars(symbol):
    first = epoch('2026-11-01')
    cache = Path(tempfile.gettempdir()) / f'slow-spot-{symbol}-2026-11-01_18.json'
    if cache.exists():
        rows = json.loads(cache.read_bytes())
    else:
        url = (f'https://data-api.binance.vision/api/v3/klines?interval=4h&limit=110&symbol={symbol}'
               f'&startTime={first*1000}&endTime={END*1000-1}')
        with urllib.request.urlopen(url, timeout=30) as response:
            rows = json.load(response)
    history = load_month((symbol, '2026-09')) + load_month((symbol, '2026-10'))
    history.extend(Bar(int(row[0])//1000, *map(float, row[1:5])) for row in rows)
    if [b.time for b in history] != list(range(epoch('2026-09-01'), END, BAR)):
        raise ValueError('Incomplete signal history')
    if any(not 0 < b.low <= min(b.open,b.close) <= max(b.open,b.close) <= b.high for b in history):
        raise ValueError('Invalid signal bar')
    return history


class BookAccount:
    def __init__(self, rule, fee, slip):
        self.rule, self.fee, self.slip = rule, fee, slip
        self.cash = self.peak = 100.
        self.dd = 0.
        self.symbol = self.order = self.exit_due = None
        self.halted = False
        self.ledger = []
        self.misses = self.unpriced = 0

    def step(self, t, quotes, bars, signals, first_bar):
        if self.exit_due is not None and t >= self.exit_due:
            quote = quotes[self.symbol]
            if quote and quote[3] >= self.units:
                credit = self.units*quote[1]*(1-self.slip)*(1-self.fee)
                self.cash += credit
                self.peak = max(self.peak, self.cash)
                self.dd = max(self.dd, 1-self.cash/self.peak)
                self.halted |= self.dd >= .08
                self.ledger.append(dict(symbol=self.symbol, entry=self.entered, exit=t,
                                        debit=self.debit, credit=credit, pnl=credit-self.debit))
                self.symbol = self.exit_due = None
        if self.order:
            s, units = self.order
            self.order = None
            quote = quotes[s]
            price = quote[2]*(1+self.slip) if quote else 0.
            nominal = units*price
            debit = nominal*(1+self.fee)
            if (not self.symbol and not self.halted and quote and quote[4] >= units
                    and 20 <= nominal <= 25 and debit <= self.cash):
                self.symbol, self.units, self.debit = s, units, debit
                self.entered, self.entry_price = t, price
                self.cash -= debit
            else:
                self.misses += 1
        if self.symbol:
            quote = quotes[self.symbol]
            if quote and quote[3] >= self.units:
                equity = self.cash+self.units*quote[1]*(1-self.slip)*(1-self.fee)
                self.peak = max(self.peak, equity)
                self.dd = max(self.dd, 1-equity/self.peak)
                if self.dd >= .08:
                    self.halted = True
                    if self.exit_due is None:
                        self.exit_due = t+1
            else:
                self.unpriced += 1
        if t == END-61 and self.symbol and self.exit_due is None:
            self.exit_due = t+1
        if t % BAR or self.halted or self.exit_due is not None:
            return
        i = (t-first_bar)//BAR
        picks, exits = signals[self.rule].get(i, ([], []))
        if self.symbol:
            reason = exit_reason(self.rule, self.symbol, exits, bars[self.symbol][i-1].close,
                                 self.entry_price, (t+1-self.entered)//BAR)
            if not reason:
                return
            self.exit_due = t+1
            if self.rule != 'rotation' or reason != 'rule':
                return
        if picks:
            s = picks[0]
            if quotes[s]:
                step = STEPS[s]
                self.order = s, math.floor(24.5/(quotes[s][2]*(1+self.slip))/step)*step
            else:
                self.misses += 1


def replay(tape):
    bars = {s: recent_bars(s) for s in SYMBOLS}
    signals = decisions(bars)
    first_bar = bars[SYMBOLS[0]][0].time
    accounts = {f'{r}/{name}': BookAccount(r, fee, slip) for r in RULES
                for name, fee, slip in [('base', .001, .0005), ('stress', .002, .001)]}
    gaps = []
    previous = None
    for day in read_days(tape, START, END, {'clock_jumps': 0, 'filters': {}}):
        gap = day.longest_gap(previous)
        gaps.append(max(gap[s] for s in SYMBOLS))
        for j in range(DAY):
            quotes = {s: day.quotes[s][j] for s in SYMBOLS}
            for account in accounts.values():
                account.step(day.start+j, quotes, bars, signals, first_bar)
        previous = day
    result = {}
    for name, a in accounts.items():
        net = math.fsum(t['pnl'] for t in a.ledger)
        if a.symbol or a.order or not math.isclose(a.cash-100, net, abs_tol=1e-8):
            raise ValueError('Open position or ledger mismatch')
        result[name] = dict(net=net, trades=len(a.ledger), wins=sum(t['pnl']>0 for t in a.ledger),
                            dd_pct=100*a.dd, misses=a.misses, unpriced_seconds=a.unpriced, ledger=a.ledger)
    return dict(days_with_gap_ge_60s=sum(g>=60 for g in gaps), max_gap_seconds=max(gaps), results=result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tape', type=Path)
    print(json.dumps(replay(parser.parse_args().tape), indent=2))
