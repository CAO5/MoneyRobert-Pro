#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Session 001 模拟盘 —— 三臂对照（纯标准库，无第三方依赖）

用法:
    python paper_trade.py --selftest                 # 离线自检，生成合成数据跑通
    python paper_trade.py --fetch                    # 联网抓取 BTC/ETH 日线与资金费率
    python paper_trade.py                            # 用 data/ 下的本地 CSV 回放
    python paper_trade.py --start 2020-01-01         # 指定回放起点

规则来源: research/session001/SPEC.md （冻结，不许改参数）
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import os
import random
import sys
import urllib.request
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

# ---------------------------------------------------------------- 冻结参数
M_WIN = 20          # 动量窗口（日）
F_MEAN = 14         # 资金费率均值窗口（日）
F_Z = 90            # z-score 窗口（日）
Z_LONG = 1.0        # LONG 要求 C < 1.0
Z_SHORT = 0.0       # SHORT 要求 C > 0.0
COST_SIDE = 0.0010  # 单边 10bp
GROSS_CAP = 0.1225  # 总名义 / 权益
MIN_HISTORY = F_Z + F_MEAN + M_WIN

ASSETS = ["BTC", "ETH"]
SYMBOLS = {"BTC": "BTCUSDT", "ETH": "ETHUSDT"}
OKX_INST = {"BTC": "BTC-USDT-SWAP", "ETH": "ETH-USDT-SWAP"}

BASE = Path(__file__).resolve().parent
DATA = BASE / "data"


# ---------------------------------------------------------------- 工具
def log(msg: str) -> None:
    print(msg, flush=True)


def http_json(url: str, timeout: int = 25):
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode("utf-8"))


def to_ms(d: date) -> int:
    return int(datetime(d.year, d.month, d.day, tzinfo=timezone.utc).timestamp() * 1000)


# ---------------------------------------------------------------- 数据读取
def read_klines_csv(path: Path) -> List[Tuple[date, float]]:
    """CSV 需含表头 date,close（大小写不敏感，允许 OHLCV 全列）"""
    rows: List[Tuple[date, float]] = []
    with path.open("r", encoding="utf-8-sig", newline="") as f:
        rd = csv.DictReader(f)
        if not rd.fieldnames:
            raise ValueError(f"{path} 无表头")
        lower = {c.lower().strip(): c for c in rd.fieldnames}
        if "date" not in lower or "close" not in lower:
            raise ValueError(f"{path} 需要 date / close 两列，实际: {rd.fieldnames}")
        for r in rd:
            ds = (r[lower["date"]] or "").strip()[:10]
            cs = (r[lower["close"]] or "").strip()
            if not ds or not cs:
                continue
            try:
                rows.append((date.fromisoformat(ds), float(cs)))
            except ValueError:
                continue
    rows.sort(key=lambda x: x[0])
    return rows


def read_funding_csv(path: Path) -> List[Tuple[date, float]]:
    """CSV 需含表头 date,rate（rate 为单次结算费率，可为负）"""
    rows: List[Tuple[date, float]] = []
    with path.open("r", encoding="utf-8-sig", newline="") as f:
        rd = csv.DictReader(f)
        lower = {c.lower().strip(): c for c in (rd.fieldnames or [])}
        dcol = lower.get("date") or lower.get("fundingtime")
        rcol = lower.get("rate") or lower.get("fundingrate")
        if not dcol or not rcol:
            raise ValueError(f"{path} 需要 date/rate 或 fundingTime/fundingRate")
        for r in rd:
            ds = (r[dcol] or "").strip()[:10]
            rs = (r[rcol] or "").strip()
            if not ds or not rs:
                continue
            try:
                rows.append((date.fromisoformat(ds), float(rs)))
            except ValueError:
                continue
    rows.sort(key=lambda x: x[0])
    return rows


def fetch_binance(asset: str) -> Tuple[List[Tuple[date, float]], List[Tuple[date, float]]]:
    sym = SYMBOLS[asset]
    start = to_ms(date(2019, 1, 1))
    now = to_ms(date.today())

    kl: List[Tuple[date, float]] = []
    cur = start
    while cur < now:
        url = (f"https://fapi.binance.com/fapi/v1/klines?symbol={sym}"
               f"&interval=1d&startTime={cur}&limit=1500")
        data = http_json(url)
        if not data:
            break
        for k in data:
            kl.append((datetime.fromtimestamp(k[0] / 1000, tz=timezone.utc).date(),
                       float(k[4])))
        if len(data) < 1500:
            break
        cur = int(data[-1][0]) + 86400_000

    fr: List[Tuple[date, float]] = []
    cur = start
    while cur < now:
        url = (f"https://fapi.binance.com/fapi/v1/fundingRate?symbol={sym}"
               f"&startTime={cur}&limit=1000")
        data = http_json(url)
        if not data:
            break
        for f in data:
            fr.append((datetime.fromtimestamp(f["fundingTime"] / 1000, tz=timezone.utc).date(),
                       float(f["fundingRate"])))
        if len(data) < 1000:
            break
        cur = int(data[-1]["fundingTime"]) + 1

    return kl, fr


def fetch_okx(asset: str) -> Tuple[List[Tuple[date, float]], List[Tuple[date, float]]]:
    inst = OKX_INST[asset]
    kl: List[Tuple[date, float]] = []
    after = ""
    while True:
        url = (f"https://www.okx.com/api/v5/market/history-candles"
               f"?instId={inst}&bar=1D&limit=100" + (f"&after={after}" if after else ""))
        js = http_json(url)
        data = js.get("data") or []
        if not data:
            break
        for row in data:
            ts = int(row[0])
            kl.append((datetime.fromtimestamp(ts / 1000, tz=timezone.utc).date(),
                       float(row[4])))
        after = data[-1][0]
        if len(data) < 100:
            break

    fr: List[Tuple[date, float]] = []
    before = ""
    while True:
        url = (f"https://www.okx.com/api/v5/public/funding-rate-history"
               f"?instId={inst}&limit=100" + (f"&before={before}" if before else ""))
        js = http_json(url)
        data = js.get("data") or []
        if not data:
            break
        for row in data:
            ts = int(row["fundingTime"])
            fr.append((datetime.fromtimestamp(ts / 1000, tz=timezone.utc).date(),
                       float(row["fundingRate"])))
        before = data[-1]["fundingTime"]
        if len(data) < 100:
            break

    kl.sort(key=lambda x: x[0])
    fr.sort(key=lambda x: x[0])
    return kl, fr


def save_csv(path: Path, rows: Sequence[Tuple[date, float]], col: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(["date", col])
        for d, v in rows:
            w.writerow([d.isoformat(), repr(v)])


# ---------------------------------------------------------------- 信号
def compute_signals(closes: List[float],
                    funds: List[Optional[float]]) -> Tuple[List[Optional[float]], List[Optional[float]]]:
    n = len(closes)
    mom: List[Optional[float]] = [None] * n
    crowd: List[Optional[float]] = [None] * n

    for i in range(M_WIN, n):
        if closes[i - M_WIN] > 0:
            mom[i] = closes[i] / closes[i - M_WIN] - 1.0

    f_mean: List[Optional[float]] = [None] * n
    run = 0.0
    cnt = 0
    for i in range(n):
        f = funds[i]
        if f is None:
            run, cnt = 0.0, 0
            continue
        run += f
        cnt += 1
        if cnt > F_MEAN:
            old = funds[i - F_MEAN]
            if old is not None:
                run -= old
                cnt -= 1
        if cnt == F_MEAN:
            f_mean[i] = run / F_MEAN

    for i in range(n):
        v = f_mean[i]
        if v is None or i < F_Z:
            continue
        win = [x for x in f_mean[i - F_Z + 1:i + 1] if x is not None]
        if len(win) < int(F_Z * 0.8):
            continue
        mu = sum(win) / len(win)
        var = sum((x - mu) ** 2 for x in win) / len(win)
        sd = math.sqrt(var)
        if sd <= 0:
            continue
        crowd[i] = (v - mu) / sd

    return mom, crowd


def target_state(m: Optional[float], c: Optional[float]) -> str:
    if m is None or c is None:
        return "FLAT"
    if m > 0 and c < Z_LONG:
        return "LONG"
    if m < 0 and c > Z_SHORT:
        return "SHORT"
    return "FLAT"


def step_state(cur: str, tgt: str) -> str:
    """禁止直接翻仓：LONG <-> SHORT 必须经过 FLAT"""
    if cur == tgt:
        return tgt
    if {cur, tgt} == {"LONG", "SHORT"}:
        return "FLAT"
    return tgt


# ---------------------------------------------------------------- 回放
def replay(dates: List[date],
           close: Dict[str, List[float]],
           mom: Dict[str, List[Optional[float]]],
           crowd: Dict[str, List[Optional[float]]],
           arm: str,
           start_idx: int) -> Dict[str, object]:
    assets = list(close.keys())
    zero = {a: 0.0 for a in assets}
    prev_w = dict(zero)
    cur_st = {a: "FLAT" for a in assets}
    equity = 1.0
    curve: List[float] = []
    out_dates: List[date] = []
    trades = 0
    expo_sum = 0.0
    long_days = short_days = 0
    state_log: List[Dict[str, str]] = []

    for i in range(max(1, start_idx), len(dates)):
        rets = {}
        for a in assets:
            p0, p1 = close[a][i - 1], close[a][i]
            rets[a] = (p1 / p0 - 1.0) if p0 > 0 else 0.0
        gross = sum(prev_w[a] * rets[a] for a in assets)
        equity *= (1.0 + gross)

        for a in assets:
            cur_st[a] = step_state(cur_st[a], target_state(mom[a][i], crowd[a][i]))

        active = []
        for a in assets:
            s = cur_st[a]
            if s == "SHORT" and arm == "A":
                s = "FLAT"
            if s != "FLAT":
                active.append(a)
        per = GROSS_CAP / len(active) if active else 0.0
        new_w = {}
        for a in assets:
            s = cur_st[a]
            if s == "LONG":
                new_w[a] = per
            elif s == "SHORT" and arm == "B":
                new_w[a] = -per
            else:
                new_w[a] = 0.0

        turnover = sum(abs(new_w[a] - prev_w[a]) for a in assets)
        if turnover > 1e-9:
            trades += 1
            equity *= (1.0 - turnover * COST_SIDE)
        prev_w = new_w

        g = sum(abs(v) for v in new_w.values())
        expo_sum += g
        if any(v > 0 for v in new_w.values()):
            long_days += 1
        if any(v < 0 for v in new_w.values()):
            short_days += 1

        curve.append(equity)
        out_dates.append(dates[i])
        state_log.append({a: cur_st[a] for a in assets})

    return {"curve": curve, "dates": out_dates, "trades": trades,
            "expo": expo_sum / max(1, len(curve)), "long_days": long_days,
            "short_days": short_days, "states": state_log}


def metrics(curve: List[float], dates: List[date]) -> Dict[str, float]:
    if not curve:
        return {"total": 0.0, "cagr": 0.0, "mdd": 0.0, "calmar": 0.0, "days": 0}
    total = curve[-1] - 1.0
    days = max(1, (dates[-1] - dates[0]).days)
    cagr = curve[-1] ** (365.0 / days) - 1.0 if curve[-1] > 0 else -1.0
    peak, mdd = curve[0], 0.0
    for e in curve:
        peak = max(peak, e)
        mdd = max(mdd, 1.0 - e / peak)
    calmar = cagr / mdd if mdd > 1e-9 else float("inf")
    return {"total": total, "cagr": cagr, "mdd": mdd, "calmar": calmar, "days": days}


# ---------------------------------------------------------------- 自检数据
def gen_synthetic(days: int = 900, seed: int = 7):
    rnd = random.Random(seed)
    d0 = date(2022, 1, 1)
    dates = [d0 + timedelta(days=k) for k in range(days)]
    close: Dict[str, List[float]] = {}
    funds: Dict[str, List[Optional[float]]] = {}
    for a, px0 in (("BTC", 40000.0), ("ETH", 2800.0)):
        px = px0
        cl, fd = [], []
        for k in range(days):
            drift = 0.0010 * math.sin(k / 45.0)
            px *= (1.0 + drift + rnd.gauss(0, 0.022))
            cl.append(px)
            fd.append(rnd.gauss(0.00008, 0.00016))
        close[a] = cl
        funds[a] = fd
    return dates, close, funds


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--selftest", action="store_true", help="用合成数据自检")
    ap.add_argument("--fetch", action="store_true", help="联网抓取并缓存到 data/")
    ap.add_argument("--source", choices=["binance", "okx"], default="binance")
    ap.add_argument("--start", default=None, help="回放起点 YYYY-MM-DD")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    DATA.mkdir(parents=True, exist_ok=True)

    if args.selftest:
        log("[自检] 生成合成数据（仅验证引擎正确性，不代表任何真实结果）")
        dates, close, funds_raw = gen_synthetic()
        funds = {a: funds_raw[a] for a in funds_raw}
        src_note = "synthetic"
    else:
        close, funds = {}, {}
        for a in ASSETS:
            kp = DATA / f"{a}_1d.csv"
            fp = DATA / f"{a}_funding.csv"
            if args.fetch:
                log(f"[抓取] {a} <- {args.source}")
                try:
                    kl, fr = (fetch_binance(a) if args.source == "binance" else fetch_okx(a))
                    save_csv(kp, kl, "close")
                    save_csv(fp, fr, "rate")
                    log(f"         K线 {len(kl)} 条, 资金费 {len(fr)} 条")
                except Exception as e:
                    log(f"         抓取失败: {type(e).__name__}: {e}")
            if not kp.exists():
                log(f"[错误] 缺少 {kp}。请先 --fetch，或把日线 CSV 放到该路径。")
                log("       需要的列: date,close  /  资金费: date,rate")
                return 2
            kl = read_klines_csv(kp)
            fr = read_funding_csv(fp) if fp.exists() else []
            close[a] = {d: v for d, v in kl}
            fsum: Dict[date, float] = {}
            for d, v in fr:
                fsum[d] = fsum.get(d, 0.0) + v
            funds[a] = fsum
            src_note = str(DATA)

        alld = sorted(set().union(*[set(close[a].keys()) for a in ASSETS]))
        dates = alld
        c2, f2 = {}, {}
        for a in ASSETS:
            seq, fseq, last = [], [], None
            for d in dates:
                v = close[a].get(d)
                if v is not None:
                    last = v
                seq.append(last if last is not None else 0.0)
                fseq.append(funds[a].get(d))
            c2[a], f2[a] = seq, fseq
        close, funds = c2, f2

    missing_funding = [a for a in close if not any(v is not None for v in funds[a])]
    if missing_funding and not args.selftest:
        log(f"[错误] 以下资产缺少资金费率数据: {missing_funding}")
        log("       没有资金费，拥挤度 C 全为空，所有信号都会是 FLAT（静默失效）。")
        log("       请提供 data/<ASSET>_funding.csv，或先 --fetch。")
        return 4

    assets = list(close.keys())
    mom, crowd = {}, {}
    for a in assets:
        m, c = compute_signals(close[a], funds[a])
        mom[a], crowd[a] = m, c

    start_idx = MIN_HISTORY
    if args.start:
        sd = date.fromisoformat(args.start)
        start_idx = max(start_idx, next((k for k, d in enumerate(dates) if d >= sd), len(dates) - 1))

    if len(dates) <= start_idx + 5:
        log(f"[错误] 有效样本不足：共 {len(dates)} 天，起点索引 {start_idx}。")
        return 3

    res = {}
    for arm in ("A", "B", "C"):
        if arm == "C":
            n = len(dates) - start_idx
            curve = []
            eq = 1.0
            for i in range(start_idx, len(dates)):
                if i == start_idx:
                    curve.append(1.0)
                    continue
                r = sum(close[a][i] / close[a][i - 1] - 1.0 for a in assets) / len(assets)
                eq *= (1.0 + r)
                curve.append(eq)
            res["C"] = {"curve": curve, "dates": dates[start_idx:], "trades": 1,
                        "expo": 1.0, "long_days": n, "short_days": 0, "states": []}
        else:
            res[arm] = replay(dates, close, mom, crowd, arm, start_idx)

    log("")
    log("=" * 74)
    log(f"{'臂':<22}{'总收益':>10}{'年化':>10}{'最大回撤':>11}{'Calmar':>9}{'调仓':>7}")
    log("-" * 74)
    labels = {"A": "A 纯多 LONG/FLAT", "B": "B 多空 LONG/FLAT/SHORT", "C": "C 等权持有"}
    for arm in ("A", "B", "C"):
        m = metrics(res[arm]["curve"], res[arm]["dates"])
        log(f"{labels[arm]:<20}{m['total']*100:>9.2f}%{m['cagr']*100:>9.2f}%"
            f"{m['mdd']*100:>10.2f}%{m['calmar']:>9.2f}{res[arm]['trades']:>7}")
    log("=" * 74)
    log(f"样本区间: {res['A']['dates'][0]} → {res['A']['dates'][-1]}   数据源: {src_note}")
    log(f"平均总敞口: A {res['A']['expo']*100:.1f}%   B {res['B']['expo']*100:.1f}%")
    log(f"持仓天数:   A 多 {res['A']['long_days']} 天 / 空 {res['A']['short_days']} 天"
        f"   B 多 {res['B']['long_days']} 天 / 空 {res['B']['short_days']} 天")

    b, a = metrics(res["B"]["curve"], res["B"]["dates"]), metrics(res["A"]["curve"], res["A"]["dates"])
    log("")
    if b["total"] > a["total"] and b["mdd"] <= a["mdd"] * 1.2:
        log("主判据: B 优于 A（净值更高且回撤未超 1.2 倍）— 空头模块暂予保留")
    elif b["total"] > a["total"]:
        log("主判据: B 净值更高但回撤超标 — 空头模块不通过，需降规模再看")
    else:
        log("主判据: B 未优于 A — 空头模块否决")

    outp = Path(args.out) if args.out else BASE / "ledger.csv"
    with outp.open("w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(["date", "equity_A", "equity_B", "equity_C"] + [f"state_{x}" for x in assets])
        for k, d in enumerate(res["A"]["dates"]):
            st = res["B"]["states"][k] if k < len(res["B"]["states"]) else {}
            w.writerow([d.isoformat(), f"{res['A']['curve'][k]:.6f}",
                        f"{res['B']['curve'][k]:.6f}", f"{res['C']['curve'][k]:.6f}"]
                       + [st.get(x, "") for x in assets])
    log(f"\n账本已写入: {outp}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
