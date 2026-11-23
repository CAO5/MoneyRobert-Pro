"""Finite-horizon, spot-only inventory grids. No live order functions."""
import csv
import math
from collections import deque
from pathlib import Path
from statistics import median

from independent_round import book


DAYS = tuple(f'2025-{number:02}-01' for number in range(1,13))
BLOCK = 7200


def replay(quotes, fee, anchor_rule):
    cash = peak = 100.
    drawdown = 0.
    history = deque(maxlen=120)
    inventory = {}
    ledger = []
    levels = []
    previous = None
    for sec, now in enumerate(quotes[:-1]):
        following = quotes[sec+1]
        if sec%BLOCK==0:
            levels = []
        if not levels and not inventory and now and sec%BLOCK<60 and len(history)>=108:
            prior = [price for moment,price in history if moment>=sec-BLOCK]
            if len(prior)>=108:
                anchor = median(prior) if anchor_rule=='median' else (now[1]+now[2])/2
                levels = [anchor*(1-.0075*rung) for rung in (1,2,3)]
        if now and sec%60==0:
            history.append((sec,(now[1]+now[2])/2))
        if now:
            equity = cash+sum(lot[0]*now[1]*(1-fee) for lot in inventory.values())
            peak = max(peak,equity)
            drawdown = max(drawdown,1-equity/peak)
        force = drawdown>=.08 or sec%BLOCK>=BLOCK-30 or sec>=len(quotes)-32 or not levels
        if now and following:
            available = following[3]
            sold = False
            for rung,(size,cost,target,entered) in list(inventory.items()):
                if (force or now[1]>=target) and available>=size:
                    proceeds = size*following[1]*(1-fee)
                    cash += proceeds
                    available -= size
                    del inventory[rung]
                    ledger.append((entered,sec+1,rung,cost,proceeds-cost,force))
                    sold = True
            if not sold and not force and previous is not None and following[4]>0:
                for rung,price in enumerate(levels):
                    if rung in inventory or not now[2]<=price<previous:
                        continue
                    size = math.floor(20/now[2])
                    cost = size*following[2]*(1+fee)
                    if size>0 and following[4]>=size and cash>=cost:
                        cash -= cost
                        inventory[rung] = (size,cost,price/(1-.0075),sec+1)
                    break
            equity = cash+sum(lot[0]*following[1]*(1-fee) for lot in inventory.values())
            peak = max(peak,equity)
            drawdown = max(drawdown,1-equity/peak)
        previous = now[2] if now else None
    if not inventory and not math.isclose(cash-100,sum(row[4] for row in ledger),abs_tol=1e-8):
        raise ValueError('Inventory ledger does not reconcile')
    return ledger,drawdown,inventory


def main():
    folder = Path(__file__).parent
    with (folder/'inventory_grid_results.csv').open('w',newline='') as output, (folder/'inventory_grid_trades.csv').open('w',newline='') as detail:
        summary,orders = csv.writer(output),csv.writer(detail)
        summary.writerow(('day','anchor','fee','trades','wins','net_usdt','dd_pct','forced_sales','residual_lots'))
        orders.writerow(('day','anchor','fee','entry_second','exit_second','rung','cost_usdt','net_usdt','forced'))
        for day in DAYS:
            quotes = book(day,'DOGEUSDT')
            for anchor in ('opening','median'):
                for fee in (.001,.002):
                    ledger,dd,inventory = replay(quotes,fee,anchor)
                    orders.writerows((day,anchor,fee,*row) for row in ledger)
                    summary.writerow((day,anchor,fee,len(ledger),sum(row[4]>0 for row in ledger),
                                      round(sum(row[4] for row in ledger),6) if not inventory else None,
                                      round(100*dd,4),sum(row[5] for row in ledger),len(inventory)))


if __name__=='__main__':
    main()
