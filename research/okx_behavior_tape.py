"""Record read-only OKX perpetual depth and trades with receiver clocks and gap markers."""
import argparse
import asyncio
import json
import shutil
import time
from pathlib import Path

import aiohttp

from forward_tape import GIB, Tape


URL = 'wss://ws.okx.com/ws/v5/public'
PUBLIC_ENDPOINTS = {'global':URL, 'aws':'wss://wsaws.okx.com/ws/v5/public'}
INSTRUMENTS = ('BTC-USDT-SWAP', 'ETH-USDT-SWAP', 'DOGE-USDT-SWAP')
CHANNELS = ('books', 'trades', 'funding-rate', 'mark-price')
NS = 1_000_000_000
BOOK_WATCH = INSTRUMENTS[:2]
BOOK_STALL_SECONDS = 8
MAX_EXCESS_DELAY_MS = 3000
STALL_GRACE_SECONDS = 3


class BookWatch:
    def __init__(self, instruments=INSTRUMENTS):
        self.instruments = tuple(name for name in BOOK_WATCH if name in instruments)
        self.baseline = {}
        self.reconnect(0)

    def reconnect(self, mono):
        self.last = dict.fromkeys(self.instruments,mono)
        self.delayed = {}

    def book(self, instrument, mono, venue_ms):
        if instrument not in self.last:
            return
        age = mono//1_000_000-venue_ms
        baseline = self.baseline[instrument] = min(self.baseline.get(instrument,age),age)
        if age-baseline > MAX_EXCESS_DELAY_MS:
            self.delayed.setdefault(instrument,mono)
        else:
            self.delayed.pop(instrument,None)
        self.last[instrument] = mono

    def check(self, mono):
        for instrument in self.instruments:
            if mono-self.last[instrument] > BOOK_STALL_SECONDS*NS:
                return dict(instrument=instrument,detail='no-book-message')
            if instrument in self.delayed and mono-self.delayed[instrument] > STALL_GRACE_SECONDS*NS:
                return dict(instrument=instrument,detail='sustained-relative-delay')
        return None


async def collect(root, seconds, cap, *, split_streams=False, endpoint='global',
                  instruments=INSTRUMENTS, ws_compression=False):
    tape = Tape(root)
    url = PUBLIC_ENDPOINTS[endpoint]
    started = time.monotonic()
    deadline = started + seconds
    first_seal = started + 15
    next_check = started
    reason = 'interrupted'
    groups = [(instrument,) for instrument in instruments] if split_streams else [instruments]
    stopped = None
    next_connect = 0.
    connect_lock = asyncio.Lock()
    options = [{'channel':channel,'instId':instrument} for instrument in instruments for channel in CHANNELS]
    tape.record('okx','configuration',dict(url=url,channels=options,seconds=seconds,max_bytes=cap,
                                          book_stall_seconds=BOOK_STALL_SECONDS,
                                          max_excess_delay_ms=MAX_EXCESS_DELAY_MS,
                                          stall_grace_seconds=STALL_GRACE_SECONDS,split_streams=split_streams,
                                          ws_compression=ws_compression))

    def check():
        nonlocal next_check, stopped
        if stopped:
            return stopped
        now = time.monotonic()
        if now >= deadline:
            stopped = 'deadline'
        if now >= next_check:
            next_check = now+1
            if (root/'STOP').exists():
                stopped = 'stop-file'
            elif tape.size() >= cap-2*1024**2 or shutil.disk_usage(root).free < 10*GIB:
                stopped = 'storage-limit'
            if now >= first_seal and tape.shard == 0:
                tape.seal()
        return stopped

    async def capture(client, instruments):
        nonlocal stopped, next_connect
        scope = {'instruments':instruments} if split_streams or instruments!=INSTRUMENTS else {}
        watchdog = BookWatch(instruments)
        subscriptions = [item for item in options if item['instId'] in instruments]
        while not (stop := check()):
            sequence = {}
            try:
                async with connect_lock:
                    await asyncio.sleep(max(0.,next_connect-time.monotonic()))
                    next_connect = time.monotonic()+(.55 if len(groups)>1 else 1.1)
                if check():
                    return
                async with client.ws_connect(url,heartbeat=10,compress=15 if ws_compression else 0,
                                             timeout=aiohttp.ClientWSTimeout(ws_close=1)) as socket:
                    tape.record('okx','connect',dict(compression_window=socket.compress,**scope))
                    watchdog.reconnect(time.monotonic_ns())
                    await socket.send_json({'op':'subscribe','args':subscriptions})
                    while not (stop := check()):
                        try:
                            msg = await socket.receive(timeout=5)
                        except TimeoutError:
                            await socket.send_str('ping')
                            msg = await socket.receive(timeout=5)
                        wall,mono = time.time_ns(),time.monotonic_ns()
                        if msg.type != aiohttp.WSMsgType.TEXT:
                            tape.record('okx','disconnect',dict(type=msg.type.name,**scope),wall,mono)
                            break
                        if msg.data == 'pong':
                            tape.record('okx','pong','pong',wall,mono)
                        else:
                            packet = json.loads(msg.data)
                            arg = packet.get('arg',{})
                            channel,inst = arg.get('channel'),arg.get('instId')
                            tape.record('okx',channel or 'control',msg.data,wall,mono)
                            if packet.get('event') == 'error':
                                stopped = 'subscription-error'
                                return
                            if channel == 'books' and packet.get('data'):
                                if len(packet['data']) != 1 or inst not in instruments:
                                    raise ValueError('Unexpected depth message shape')
                                update = packet['data'][0]
                                action = packet.get('action')
                                previous,current = int(update['prevSeqId']),int(update['seqId'])
                                if (action == 'snapshot' and previous == -1) or (
                                        action == 'update' and sequence.get(inst) == previous):
                                    sequence[inst] = current
                                else:
                                    tape.record('okx','sequence_gap',dict(instrument=inst,expected=sequence.get(inst),
                                                                        previous=previous,current=current,**scope),wall,mono)
                                    break
                                if update['asks'] or update['bids']:
                                    watchdog.book(inst,mono,int(update['ts']))
                        stale = watchdog.check(mono)
                        if stale:
                            tape.record('okx','disconnect',dict(type='stale-book',**stale,**scope),wall,mono)
                            break
            except (TimeoutError,aiohttp.ClientError,OSError) as error:
                tape.record('okx','disconnect',dict(type=type(error).__name__,detail=str(error),**scope))
            if stop:
                break
            await asyncio.sleep(1)
    try:
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=None,sock_connect=10)) as client:
            async with asyncio.TaskGroup() as tasks:
                for group in groups:
                    tasks.create_task(capture(client,group))
        reason = stopped or reason
    finally:
        tape.seal()
        tape.catalog.close()
        (root/'summary.json').write_text(json.dumps(dict(reason=reason, files=tape.shard, bytes=tape.sealed,
                                                  counts=tape.counts),indent=2),encoding='utf-8')


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('directory',type=Path)
    cli.add_argument('--seconds',type=int,default=60)
    cli.add_argument('--max-gib',type=int,default=2)
    cli.add_argument('--split-streams',action='store_true',help='Use a separate public connection for each instrument')
    cli.add_argument('--endpoint',choices=PUBLIC_ENDPOINTS,default='global')
    cli.add_argument('--instrument',choices=INSTRUMENTS,help='Limit a diagnostic run to one instrument')
    cli.add_argument('--ws-compression',action='store_true',help='Request compression on the wire, not on disk')
    args = cli.parse_args()
    if not (1 <= args.seconds <= 7*86400 and 1 <= args.max_gib <= 8):
        cli.error('Set a collection duration up to 7 days and a cap of 1 to 8 GiB')
    asyncio.run(collect(args.directory,args.seconds,args.max_gib*GIB,split_streams=args.split_streams,
                        endpoint=args.endpoint,instruments=(args.instrument,) if args.instrument else INSTRUMENTS,
                        ws_compression=args.ws_compression))
