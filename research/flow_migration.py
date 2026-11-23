"""Compare hedge-demand and speculative-flow interpretations on a collateralized perpetual."""
import csv
import math
from datetime import date
from pathlib import Path

from hf_pressure import rows
from independent_round import book


DAYS = ('2023-11-01','2023-12-01','2024-07-01')


def flow(day,venue):
    origin = (date.fromisoformat(day)-date(1970,1,1)).days*86400
    buckets = [[0.,0.,0] for _ in range(1440)]
    for row in rows(day,'DOGEUSDT','trades',venue):
        minute = (int(row[3])//1000000-origin)//60
        if 0<=minute<1440 and row[5] in ('buy','sell'):
            notional = float(row[6])*float(row[7])
            slot = buckets[minute]
            slot[0] += notional
            slot[1] += notional if row[5]=='buy' else -notional
            slot[2] += 1
    return buckets


def signals(spot,perp):
    ledger = {}
    for minute in range(30,1440):
        spot_volume,spot_net,spot_count = spot[minute]
        perp_volume,perp_net,perp_count = perp[minute]
        spot_hist = sum(slot[0] for slot in spot[minute-30:minute])
        perp_hist = sum(slot[0] for slot in perp[minute-30:minute])
        sec = minute*60+59
        if (min(spot_count,perp_count)<10 or min(spot_volume,perp_volume,spot_hist,perp_hist)<=0
                or sec%3600<60 or sec%3600>=3000 or perp_volume/spot_volume<2*perp_hist/spot_hist):
            continue
        cash_imbalance = spot_net/spot_volume
        deriv_imbalance = perp_net/perp_volume
        if abs(deriv_imbalance)>=.5 and cash_imbalance*deriv_imbalance<=0:
            ledger[sec] = 1 if deriv_imbalance>0 else -1
    return ledger


def account(quotes,signals_by_sec,mode,fee):
    cash = peak = 100.
    quantity = dd = end = 0
    pnl_rows = []
    misses = crossed_hour = 0
    for sec,now in enumerate(quotes[:-1]):
        after = quotes[sec+1]
        if quantity:
            if now:
                price = now[1] if quantity>0 else now[2]
                equity = cash+quantity*(price-entry_price)-abs(quantity)*price*fee
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
            if (sec>=end or dd>=.08) and after:
                size = after[3] if quantity>0 else after[4]
                if abs(quantity)>size:
                    continue
                price = after[1] if quantity>0 else after[2]
                proceeds = quantity*(price-entry_price)-abs(quantity)*price*fee
                cash += proceeds
                pnl_rows.append((entered,sec+1,quantity,entry_price,price,proceeds-entry_fee))
                crossed_hour += (entered+1)//3600!=(sec+2)//3600
                quantity = 0
                peak = max(peak,cash)
                dd = max(dd,1-cash/peak)
            continue
        if sec not in signals_by_sec or dd>=.08 or not now:
            continue
        direction = signals_by_sec[sec] if mode=='follow' else -signals_by_sec[sec]
        amount = math.floor(20/now[2])
        if not after or amount<=0 or amount>(after[4] if direction>0 else after[3]):
            misses += 1
            continue
        entry_price = after[2] if direction>0 else after[1]
        if amount*entry_price*(1+fee)>cash:
            misses += 1
            continue
        quantity = amount*direction
        entry_fee = amount*entry_price*fee
        cash -= entry_fee
        entered = sec+1
        end = entered+300
    if not quantity and not math.isclose(cash-100,sum(row[-1] for row in pnl_rows),abs_tol=1e-8):
        raise ValueError('Perpetual ledger mismatch')
    return pnl_rows,dd,misses,quantity,crossed_hour


if __name__=='__main__':
    folder = Path(__file__).parent
    with (folder/'flow_migration_results.csv').open('w',newline='') as output, (folder/'flow_migration_trades.csv').open('w',newline='') as detail:
        writer,orders = csv.writer(output),csv.writer(detail)
        writer.writerow(('day','mode','fee','signals','trades','wins','net_usdt','dd_pct','misses','residual','crossed_hour'))
        orders.writerow(('day','mode','fee','entry_sec','exit_sec','signed_units','entry_price','exit_price','net_usdt'))
        for day in DAYS:
            quotes = book(day,'DOGEUSDT','binance-futures')
            opportunities = signals(flow(day,'binance'),flow(day,'binance-futures'))
            for mode in ('follow','fade'):
                for fee in (.001,.002):
                    trades,dd,misses,residual,crossed = account(quotes,opportunities,mode,fee)
                    orders.writerows((day,mode,fee,*row) for row in trades)
                    record = (day,mode,fee,len(opportunities),len(trades),sum(row[-1]>0 for row in trades),
                              round(sum(row[-1] for row in trades),6) if not (residual or crossed) else None,
                              round(100*dd,4),misses,residual,crossed)
                    writer.writerow(record)
                    print(*record,flush=True)
