"""Two independently funded spot accounts. Sequential legs, inventory and unwind ledger."""
import csv
import math
from pathlib import Path

from independent_round import book


VENUES = [('binance','DOGEUSDT'),('okex','DOGE-USDT'),('bybit-spot','DOGEUSDT')]


def screen(books,fee):
    cash = [50.,50.]
    inventory = [0,0]
    qty = 0
    peak = 100.
    dd = 0.
    swap = pending = None
    cycle_cash = seed_cost = 0.
    repaired = attempts = 0
    increments = []
    last_exit = [None,None]
    limit = len(books[0])
    for sec,(left,right) in enumerate(zip(*books)):
        quotes = (left,right)
        ending = sec>=limit-90 or dd>=.08
        if qty:
            if all(q is not None or inventory[venue]==0 for venue,q in enumerate(quotes)):
                equity = sum(cash)+sum(inventory[venue]*q[1]*(1-fee) for venue,q in enumerate(quotes) if q)
                peak = max(peak,equity)
                dd = max(dd,1-equity/peak)
                ending |= dd>=.08
        elif sec<60 and left and right:
            qty = math.floor(20/max(left[2],right[2]))
            if not qty or any(q[4]<qty for q in quotes):
                qty = 0
                continue
            for venue,q in enumerate(quotes):
                cost = qty*q[2]*(1+fee)
                cash[venue] -= cost
                inventory[venue] += qty
                seed_cost += cost
            continue
        if not qty:
            continue
        if pending:
            kind,buyer,at = pending
            seller = 1-buyer
            bid_q,ask_q = quotes[seller],quotes[buyer]
            if kind=='buy':
                if ask_q and ask_q[4]>=qty and cash[buyer]>=qty*ask_q[2]*(1+fee):
                    cash[buyer] -= qty*ask_q[2]*(1+fee)
                    inventory[buyer] += qty
                    pending = ('sell',buyer,at)
                else:
                    pending = None
                continue
            if kind=='sell':
                if bid_q and bid_q[3]>=qty and inventory[seller]>=qty:
                    cash[seller] += qty*bid_q[1]*(1-fee)
                    inventory[seller] -= qty
                    if swap is None:
                        swap = (buyer,sec)
                    else:
                        increments.append(sum(cash)-cycle_cash)
                        swap = None
                    pending = None
                else:
                    pending = ('repair',buyer,at)
                continue
            if ask_q and ask_q[3]>=qty:
                cash[buyer] += qty*ask_q[1]*(1-fee)
                inventory[buyer] -= qty
                repaired += 1
                if swap is None:
                    increments.append(sum(cash)-cycle_cash)
                pending = None
            continue
        if ending and swap is None:
            for venue,q in enumerate(quotes):
                if inventory[venue] and q and q[3]>=inventory[venue]:
                    last_exit[venue] = q[1]
                    cash[venue] += inventory[venue]*q[1]*(1-fee)
                    inventory[venue] = 0
            if not any(inventory):
                peak = max(peak,sum(cash))
                dd = max(dd,1-sum(cash)/peak)
                break
            continue
        if not left or not right:
            continue
        if swap is None:
            if sec>=limit-300:
                continue
            buyer = 0 if left[2]/right[1]<right[2]/left[1] else 1
            edge = quotes[1-buyer][1]*(1-fee)/(quotes[buyer][2]*(1+fee))-1
            if edge<.0002:
                continue
            cycle_cash = sum(cash)
        else:
            previous,opened = swap
            buyer = 1-previous
            estimated = sum(cash)-cycle_cash+qty*(quotes[previous][1]*(1-fee)-quotes[buyer][2]*(1+fee))
            if estimated<qty*quotes[buyer][2]*.0002 and sec-opened<180 and not ending:
                continue
        if quotes[buyer][4]>=qty and quotes[1-buyer][3]>=qty and inventory[1-buyer]>=qty:
            attempts += 1
            pending = ('buy',buyer,sec)
    resolved = qty and not any(inventory) and pending is None and swap is None
    net = sum(cash)-100 if resolved else None
    hold = qty*sum(last_exit)*(1-fee)-seed_cost if resolved else None
    if resolved and not math.isclose(net-hold,sum(increments),abs_tol=1e-8):
        raise ValueError('Inventory difference failed reconciliation')
    return (attempts,len(increments),sum(pnl>0 for pnl in increments),repaired,
            round(sum(increments),6),round(net,6) if resolved else None,
            round(hold,6) if resolved else None,round(100*dd,4),int(not resolved))


def main():
    with Path(__file__).with_name('prefunded_cycle_results.csv').open('w',newline='') as output:
        writer = csv.writer(output)
        writer.writerow(('day','first','second','fee','attempts','cycle_or_repair','wins','repairs','incremental_usdt','net_usdt','hold_usdt','dd_pct','unresolved'))
        for number in range(1,10):
            day = f'2025-{number:02}-01'
            books = [book(day,symbol,venue) for venue,symbol in VENUES]
            for first,second in ((0,1),(0,2),(1,2)):
                for fee in (.001,.002):
                    result = screen((books[first],books[second]),fee)
                    writer.writerow((day,VENUES[first][0],VENUES[second][0],fee,*result))
            print(day,flush=True)


if __name__=='__main__':
    main()
