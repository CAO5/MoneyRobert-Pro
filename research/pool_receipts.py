"""Public execution receipts and isolated AMM math; not a live order simulator."""
import csv
import hashlib
import json
import time
import urllib.request
from datetime import datetime, timezone
from decimal import Decimal

from wrapped_flow import CACHE, DOGE, POOLS, ROOT


PAIR, USDT = POOLS['usdt']
RPC = 'https://bsc-dataseed-public.bnbchain.org'
SYMBOLS = (USDT, DOGE)
SYNC = '0x1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1'
SWAP = '0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822'
START = int(datetime(2026, 11, 19, 3, tzinfo=timezone.utc).timestamp())
END = START + 3600


def json_file(name, request):
    folder = CACHE / 'receipts'
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / name
    if not path.exists():
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
        json.loads(raw)
        path.write_bytes(raw)
    return json.loads(path.read_bytes())


def rpc_batches(method, params):
    result = []
    for start in range(0, len(params), 10):
        batch = [dict(jsonrpc='2.0', id=index, method=method, params=args)
                 for index, args in enumerate(params[start:start + 10])]
        raw = json.dumps(batch).encode()
        request = urllib.request.Request(RPC, data=raw, headers={'Content-Type': 'application/json'})
        messages = json_file(f'{method}-{hashlib.sha256(raw).hexdigest()[:20]}.json', request)
        ordered = sorted(messages, key=lambda entry: entry['id'])
        if [item['id'] for item in ordered] != list(range(len(batch))) or any(item.get('result') is None for item in ordered):
            raise ValueError('Missing RPC record')
        result.extend(item['result'] for item in ordered)
    return result


def words(data, count):
    if not data.startswith('0x') or len(data) != 2 + 64 * count:
        raise ValueError('Malformed log')
    return [int(data[2 + 64 * index:66 + 64 * index], 16) for index in range(count)]


def swap_output(amount, pool_in, pool_out):
    return amount * 9975 * pool_out // (pool_in * 10000 + amount * 9975)


def decode(receipt, block, dex):
    if int(receipt['status'], 16) != 1 or receipt['blockHash'] != block['hash'] or receipt['transactionHash'].lower() != dex['tx_hash'].lower():
        raise ValueError('Unconfirmed or inconsistent receipt')
    number, stamp = int(block['number'], 16), int(block['timestamp'], 16)
    if number != dex['block_number'] or stamp != int(datetime.fromisoformat(dex['block_timestamp']).timestamp()):
        raise ValueError('Indexer and node disagree on block or time')
    direction = 'buy' if dex['from_token_address'].lower() == USDT else 'sell'
    reverse = direction == 'sell'
    if ((dex['from_token_address'].lower(), dex['to_token_address'].lower()) !=
            (SYMBOLS[int(reverse)], SYMBOLS[int(not reverse)])):
        raise ValueError('Wrong trade tokens')
    expected_in = int(Decimal(dex['from_token_amount']) * 10 ** (8 if reverse else 18))
    expected_out = int(Decimal(dex['to_token_amount']) * 10 ** (18 if reverse else 8))
    previous = None
    for log in receipt['logs']:
        if log['address'].lower() != PAIR:
            continue
        topic = log['topics'][0]
        if topic == SWAP and previous and previous['topics'][0] == SYNC:
            reserve_cash, reserve_coin = words(previous['data'], 2)
            cash_in, coin_in, cash_out, coin_out = words(log['data'], 4)
            actual_in, actual_out = (coin_in, cash_out) if reverse else (cash_in, coin_out)
            if (abs(actual_in - expected_in) <= max(2, expected_in // 10 ** 10) and
                    abs(actual_out - expected_out) <= max(2, expected_out // 10 ** 10)):
                before_cash = reserve_cash - cash_in + cash_out
                before_coin = reserve_coin - coin_in + coin_out
                if min(before_cash, before_coin, reserve_cash, reserve_coin) <= 0:
                    raise ValueError('Invalid reserves')
                max_out = swap_output(actual_in, before_coin if reverse else before_cash, before_cash if reverse else before_coin)
                if actual_out > max_out + 1 or (cash_in and coin_in) or (cash_out and coin_out):
                    raise ValueError('Swap is not a simple constant-product execution')
                money = 25 * 10 ** 18
                bought = swap_output(money, before_cash, before_coin)
                returned = swap_output(bought, before_coin - bought, before_cash + money)
                pool_cost = (money - returned) / 10 ** 18
                buy_impact = (money * before_coin / (bought * before_cash) - 1) * 10000
                gas = int(receipt['gasUsed'], 16) * int(receipt['effectiveGasPrice'], 16) / 10 ** 18
                return (number, stamp, receipt['transactionHash'], int(log['logIndex'], 16), direction,
                        before_cash / 10 ** 18, before_coin / 10 ** 8, actual_in, actual_out, max_out - actual_out,
                        gas, pool_cost, buy_impact)
        previous = log
    raise ValueError('No amount-matched swap and sync in receipt')


def quote():
    url = 'https://data-api.binance.vision/api/v3/ticker/bookTicker?symbol=BNBUSDT'
    payload = json_file('bnb-fuel-snapshot.json', url)
    bid, ask = float(payload['bidPrice']), float(payload['askPrice'])
    if payload['symbol'] != 'BNBUSDT' or not 0 < bid <= ask:
        raise ValueError('Invalid conversion quote')
    return bid, ask


def main():
    url = f'https://api.geckoterminal.com/api/v2/networks/bsc/pools/{PAIR}/trades'
    items = [entry['attributes'] for entry in json_file('20261119-trades.json', url)['data']]
    stamps = [datetime.fromisoformat(item['block_timestamp']).timestamp() for item in items]
    if min(stamps) > START or max(stamps) < END:
        raise ValueError('Feed does not bracket the requested hour. Do not replace with another window.')
    selected = [item for item, stamp in zip(items, stamps) if START <= stamp < END]
    if not selected:
        raise ValueError('No confirmed swaps in requested window')
    hashes = sorted({item['tx_hash'] for item in selected})
    blocks = sorted({item['block_number'] for item in selected})
    receipts = dict(zip(hashes, rpc_batches('eth_getTransactionReceipt', [[value] for value in hashes])))
    headers = dict(zip(blocks, rpc_batches('eth_getBlockByNumber', [[hex(value), False] for value in blocks])))
    records = [decode(receipts[item['tx_hash']], headers[item['block_number']], item) for item in selected]
    if len({(record[2], record[3]) for record in records}) != len(records):
        raise ValueError('Duplicate event')
    _, ask = quote()
    with (ROOT / 'pool_receipt_costs.csv').open('w', newline='') as stream:
        writer = csv.writer(stream)
        writer.writerow(('block', 'utc_sec', 'tx_hash', 'log_index', 'doge_direction', 'before_usdt', 'before_doge', 'raw_input_units', 'raw_output_units', 'output_below_formula_raw_units', 'whole_tx_gas_bnb', 'no_move_roundtrip_cost_on_25_usdt', 'buy_impact_including_fee_bps', 'fuel_reference_ask_not_historical', 'whole_tx_gas_at_reference_usdt'))
        for record in sorted(records):
            writer.writerow((*record, ask, ask * record[10]))
    print(len(records), 'verified swaps;', len(hashes), 'distinct receipts', flush=True)


if __name__ == '__main__':
    main()
