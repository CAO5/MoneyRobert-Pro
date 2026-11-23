"""Compare volatility-scaled spot signals over a shared simulated 100 USDT ledger."""
import math
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass

from flow_path import HOLD, flow_events, log_mid
from flow_sector import FEES, write
from independent_round import book
from sell_pressure import volume


SYMBOLS = ('DOGEUSDT', 'BTCUSDT', 'ETHUSDT')
DOGE = SYMBOLS[0]
STEPS = {'DOGEUSDT': 1., 'BTCUSDT': .00001, 'ETHUSDT': .0001}
SCREEN_DAYS = ('2024-08-01', '2025-06-01', '2025-08-01', '2026-01-01', '2026-05-01', '2026-11-01')
SHOCK = 60
VOL_WINDOW = 30
PRESSURE_FEE = .002
COST_FLOOR = math.log((1 + PRESSURE_FEE) / (1 - PRESSURE_FEE) * 1.0002)


def volatility_floor(quotes, sec):
    end = sec - SHOCK
    start = end - VOL_WINDOW * SHOCK
    if start < 0:
        return None
    mids = [log_mid(quotes[point]) for point in range(start, end + 1, SHOCK)]
    returns = [right - left for left, right in zip(mids, mids[1:]) if left is not None and right is not None]
    if len(returns) < 24:
        return None
    rms = math.sqrt(math.fsum(change * change for change in returns) / len(returns))
    _, bid, ask, _, _ = quotes[sec]
    return max(3 * rms, COST_FLOOR + math.log(ask / bid))


@dataclass(frozen=True)
class Signal:
    sec: int
    symbol: str
    drop: float
    floor: float

    @property
    def strength(self):
        return self.drop / self.floor


def discover(symbol, quotes, trades, *, tail=HOLD + 70):
    original = []
    if symbol == DOGE:
        events, _ = flow_events(quotes, *trades, tail=tail)
        original = [Signal(sec, symbol, -move, .004) for sec, _, move, *_ in events['balanced_drop']]
    events, _ = flow_events(quotes, *trades, move_floor=volatility_floor, tail=tail)
    scaled = [Signal(sec, symbol, -move, volatility_floor(quotes, sec))
              for sec, _, move, *_ in events['balanced_drop']]
    return original, scaled


def group_signals(signals):
    """Group signals by a fixed sixty-second window from the first signal."""
    groups = []
    for signal in sorted(signals, key=lambda item: (item.sec, item.symbol)):
        if not groups or signal.sec - groups[-1][0].sec > SHOCK:
            groups.append([])
        groups[-1].append(signal)
    return groups


@dataclass
class Position:
    signal: Signal
    entry: int
    units: float
    debit: float
    due: int


class Account:
    """Single shared spot position and cash ledger, with next-second orders."""
    def __init__(self, fee):
        self.fee = fee
        self.cash = self.peak = 100.
        self.drawdown = 0.
        self.stopped = False
        self.pending = None
        self.position = None
        self.counts = Counter()
        self.trades = []

    def step(self, sec, quotes, signals):
        if self.pending:
            signal, units = self.pending
            self.pending = None
            quote = quotes[signal.symbol]
            nominal = units * quote[2] if quote else 0.
            debit = nominal * (1 + self.fee)
            if not quote or quote[4] < units or not 20 <= nominal <= 25 or debit > self.cash:
                self.counts['missed_entries'] += 1
            else:
                self.cash -= debit
                self.position = Position(signal, sec, units, debit, sec + HOLD)

        if self.position:
            position = self.position
            quote = quotes[position.signal.symbol]
            proceeds = None
            if quote and quote[3] >= position.units:
                proceeds = quote[1] * position.units * (1 - self.fee)
                equity = self.cash + proceeds
                self.peak = max(self.peak, equity)
                self.drawdown = max(self.drawdown, 1 - equity / self.peak)
                if self.drawdown >= .08 and not self.stopped:
                    self.stopped = True
                    position.due = min(position.due, sec + 1)
            else:
                self.counts['unpriced_seconds'] += 1
            if sec >= position.due:
                if proceeds is None:
                    self.counts['delayed_exit_seconds'] += 1
                else:
                    self.cash += proceeds
                    self.trades.append((position.signal.symbol, position.signal.sec, position.entry, sec,
                                        position.units, position.debit, proceeds, proceeds - position.debit))
                    self.position = None

        if signals:
            if self.position or self.stopped:
                self.counts['skipped_signals'] += len(signals)
            else:
                signal = min(signals, key=lambda item: (-item.strength, SYMBOLS.index(item.symbol)))
                self.counts['skipped_signals'] += len(signals) - 1
                quote = quotes[signal.symbol]
                if quote:
                    step = STEPS[signal.symbol]
                    units = math.floor(24.5 / quote[2] / step) * step
                    self.pending = signal, units
                else:
                    self.counts['missed_entries'] += 1


def replay(tapes, events, fee):
    account = Account(fee)
    by_second = defaultdict(list)
    for event in events:
        by_second[event.sec].append(event)
    size = len(tapes[DOGE])
    for sec in range(size):
        quotes = {symbol: tape[sec] for symbol, tape in tapes.items()}
        account.step(sec, quotes, by_second.get(sec))
    if account.position or account.pending:
        raise ValueError('Day ended with an unresolved position or order')
    if not math.isclose(account.cash - 100, math.fsum(trade[-1] for trade in account.trades), abs_tol=1e-8):
        raise ValueError('Account cash does not reconcile with the trade ledger')
    return account


def study(day):
    tapes = {}
    original, scaled = [], []
    for symbol in SYMBOLS:
        tape = book(day, symbol)
        tapes[symbol] = tape
        fixed_signals, scaled_signals = discover(symbol, tape, volume(day, 'binance', symbol))
        original.extend(fixed_signals)
        scaled.extend(scaled_signals)
    groups = group_signals(scaled)
    episodes = []
    cluster = {}
    for group in groups:
        symbols = tuple(symbol for symbol in SYMBOLS if any(event.symbol == symbol for event in group))
        for event in group:
            cluster[event.symbol, event.sec] = group[0].sec, len(symbols)
        episodes.append((day, group[0].sec, group[-1].sec, len(group), len(symbols), '+'.join(symbols)))
    summary, trades = [], []
    variants = (('doge_original', original), ('doge_scaled', [s for s in scaled if s.symbol == DOGE]),
                ('portfolio', scaled))
    for variant, signals in variants:
        for fee in FEES:
            account = replay(tapes, signals, fee)
            key = (day, variant, fee)
            counts = account.counts
            summary.append((*key, len(signals), len(account.trades), sum(t[-1] > 0 for t in account.trades),
                            account.cash - 100, account.drawdown * 100, counts['missed_entries'],
                            counts['skipped_signals'], counts['unpriced_seconds'], counts['delayed_exit_seconds']))
            for trade in account.trades:
                episode = cluster.get(trade[:2], ('', '')) if variant != 'doge_original' else ('', '')
                trades.append((*key, *trade, *episode))
    return summary, trades, episodes


def main():
    with ProcessPoolExecutor(len(SCREEN_DAYS)) as pool:
        runs = list(pool.map(study, SCREEN_DAYS))
    summary, trades, episodes = zip(*runs)
    key = ('day', 'variant', 'fee')
    write('flow_portfolio_results.csv',
          (*key, 'signals', 'trades', 'wins', 'net_usdt', 'dd_pct', 'missed_entries', 'skipped_signals',
           'unpriced_seconds', 'delayed_exit_seconds'), summary)
    write('flow_portfolio_trades.csv',
          (*key, 'symbol', 'signal_sec', 'entry_sec', 'exit_sec', 'qty', 'debit_usdt', 'credit_usdt', 'net_usdt',
           'episode_start', 'episode_symbols'), trades)
    write('flow_portfolio_episodes.csv',
          ('day', 'first_signal_sec', 'last_signal_sec', 'signals', 'symbols', 'members'), episodes)
    print(f'{len(runs)} days replayed, {sum(map(len, episodes))} volatility-scaled episodes', flush=True)


if __name__ == '__main__':
    main()
