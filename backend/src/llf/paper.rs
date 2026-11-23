//! LLF-Paper-001 consumes archived OKX observations and shares the core simulated-fill ledger.
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sqlx::{types::Json, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    backtest::{
        matching_engine::MatchingConfig,
        models::{AccountState, SimulatedFill, SimulatedPosition},
    },
    orchestration::ledger_service::LedgerService,
};

const ACCOUNT: &str = "LLF001-PAPER";
const ENTRY_TTL: i64 = 5;
type Tx<'a> = Transaction<'a, Postgres>;

struct Settings {
    enabled: bool,
    size: Option<f64>,
    fee: Option<f64>,
}

impl Settings {
    fn load() -> Self {
        let number = |name| {
            std::env::var(name)
                .ok()?
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
        };
        Self {
            enabled: super::FORMULA_STATUS == "confirmed"
                && std::env::var("LLF_PAPER_ENABLED").is_ok_and(|v| v.eq_ignore_ascii_case("true")),
            size: number("LLF_PAPER_ORDER_USDT").filter(|x| *x > 0.0),
            fee: number("LLF_PAPER_TAKER_BPS").filter(|x| (0.0..10_000.0).contains(x)),
        }
    }
}

fn number(v: &Value) -> Option<f64> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_f64())
        .filter(|x| x.is_finite())
}

struct Market {
    book: Option<(i64, DateTime<Utc>, DateTime<Utc>, Value)>,
    mark: Option<f64>,
    // OKX size is contracts. Convert both L2 quantities and the trading lot to coins.
    contract: Option<(f64, f64, f64)>,
}

async fn market(tx: &mut Tx<'_>, asset: &str) -> Result<Market> {
    let mut data = Market {
        book: None,
        mark: None,
        contract: None,
    };
    let rows = sqlx::query(
        "SELECT m.channel,m.raw_id,m.received_at,m.exchange_ts,m.payload FROM llf_market_latest m
         LEFT JOIN llf_market_session s ON s.id=1
         WHERE m.symbol=$1 AND m.quality='ok' AND ((m.channel='instruments' AND m.received_at BETWEEN now()-interval '30 minutes' AND now())
           OR (m.channel IN ('mark-price','books5') AND s.connected AND m.session_id=s.session_id
               AND m.received_at BETWEEN now()-interval '5 seconds' AND now()
               AND m.exchange_ts BETWEEN now()-interval '5 seconds' AND now()))")
        .bind(format!("{asset}-USDT-SWAP")).fetch_all(&mut **tx).await?;
    for row in rows {
        let v: Value = row.try_get("payload")?;
        match row.try_get::<&str, _>("channel")? {
            "books5" => {
                data.book = Some((
                    row.try_get("raw_id")?,
                    row.try_get("exchange_ts")?,
                    row.try_get("received_at")?,
                    v,
                ))
            }
            "mark-price" => data.mark = number(&v["markPx"]).filter(|x| *x > 0.0),
            "instruments"
                if v["state"] == "live" && v["ctValCcy"] == asset && v["ctType"] == "linear" =>
            {
                // OKX publishes an empty multiplier for some products; ctVal is then the face value.
                let mult = match v["ctMult"].as_str() {
                    Some("") => Some(1.0),
                    _ => number(&v["ctMult"]),
                };
                if let (Some(cv), Some(m), Some(lot), Some(min)) = (
                    number(&v["ctVal"]),
                    mult,
                    number(&v["lotSz"]),
                    number(&v["minSz"]),
                ) {
                    let face = cv * m;
                    if [face, lot, min].iter().all(|x| x.is_finite() && *x > 0.0) {
                        data.contract = Some((face, lot, min));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(data)
}

fn levels(value: &Value, bids: bool) -> Option<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    for row in value.as_array()? {
        let price = number(row.get(0)?)?;
        let qty = number(row.get(1)?)?;
        if price <= 0.0
            || qty <= 0.0
            || out
                .last()
                .is_some_and(|&(p, _)| if bids { price >= p } else { price <= p })
        {
            return None;
        }
        out.push((price, qty));
    }
    (!out.is_empty()).then_some(out)
}

struct Execution {
    requested: f64,
    fill: SimulatedFill,
    consumed: Vec<(f64, f64)>,
}

fn match_book(
    book: &Value,
    contract: (f64, f64, f64),
    position: Option<&SimulatedPosition>,
    size: Option<f64>,
    asset: &str,
    signal: Uuid,
    now: DateTime<Utc>,
    ledger: &LedgerService,
) -> std::result::Result<Execution, &'static str> {
    let bids = levels(&book["bids"], true).ok_or("invalid_book")?;
    let asks = levels(&book["asks"], false).ok_or("invalid_book")?;
    if bids[0].0 >= asks[0].0 {
        return Err("invalid_book");
    }
    let (face, lot, min) = contract;
    let step = face * lot;
    let closing = position.is_some();
    let side = if closing { "buy" } else { "sell" };
    let depth = if closing { &asks } else { &bids };
    let touch = depth[0].0;
    let requested = match position {
        Some(p) => p.quantity,
        None => ((size.ok_or("configuration_missing")? / touch) / step).floor() * step,
    };
    if !requested.is_finite() || requested <= 0.0 || (!closing && requested < face * min) {
        return Err("below_minimum");
    }
    // Treat each order as IOC. Opening remainders are cancelled; closing remainders
    // stay on the position and can only use a later, independent book snapshot.
    let mut consumed = Vec::new();
    let mut qty = 0.0;
    let mut notional = 0.0;
    for &(price, contracts) in depth {
        let available = ((contracts / lot + 1e-10).floor() * step).min(requested - qty);
        if available > 0.0 {
            qty += available;
            notional += available * price;
            consumed.push((price, available));
        }
        if qty >= requested {
            break;
        }
    }
    if qty <= 0.0 || !notional.is_finite() {
        return Err("no_depth");
    }
    let price = notional / qty;
    // Measure depth cost against the executable touch; the spread is already reflected in P&L.
    let slip = if closing {
        price - touch
    } else {
        touch - price
    }
    .max(0.0);
    let fill = SimulatedFill {
        fill_id: Uuid::new_v4(),
        order_id: Uuid::new_v4(),
        asset: asset.into(),
        exchange: Some("OKX".into()),
        side: side.into(),
        filled_quantity: qty,
        filled_price: price,
        notional: Some(notional),
        fee: ledger.matching().compute_fee(notional, false),
        slippage_bps: Some(slip / touch * 10_000.0),
        slippage_cost: Some(slip * qty),
        maker_taker: "taker".into(),
        signal_id: Some(signal),
        strategy_id: Some("LLF-Paper-001".into()),
        intent_type: Some(
            if closing {
                "close_position"
            } else {
                "open_position"
            }
            .into(),
        ),
        fill_time: now,
        ..Default::default()
    };
    Ok(Execution {
        requested,
        fill,
        consumed,
    })
}

async fn new_signal(
    tx: &mut Tx<'_>,
    id: i64,
    asset: &str,
    kind: &str,
    observed: DateTime<Utc>,
    pct: Option<f64>,
    mark: Option<f64>,
    status: &str,
    reason: &str,
    settings: &Settings,
    quality: &str,
) -> Result<Uuid> {
    let signal = Uuid::new_v4();
    sqlx::query("INSERT INTO llf_paper_signals(signal_id,snapshot_id,asset,kind,observed_at,decided_at,percentile,
        entry_mark,status,reason,risk_check,order_usdt,fee_bps) VALUES ($1,$2,$3,$4,$5,clock_timestamp(),$6,$7,$8,$9,$10,$11,$12)")
        .bind(signal).bind(id).bind(asset).bind(kind).bind(observed).bind(pct).bind(mark)
        .bind(status).bind(reason).bind(json!({"quality":quality,"enabled":settings.enabled,"leverage":1,"reason":reason}))
        .bind(settings.size).bind(settings.fee).execute(&mut **tx).await?;
    Ok(signal)
}

async fn finish_signal(tx: &mut Tx<'_>, signal: Uuid, status: &str, reason: &str) -> Result<()> {
    sqlx::query(
        "UPDATE llf_paper_signals SET status=$2, reason=$3,
        risk_check=risk_check||jsonb_build_object('execution_reason',$3::text) WHERE signal_id=$1",
    )
    .bind(signal)
    .bind(status)
    .bind(reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A history row is a confirmed funding rate. Reconstruct the short size at its settlement
/// boundary from actual fills, including positions that closed before the rate arrived.
/// Missing contemporaneous marks remain unbooked and visible in the research status.
async fn settle_funding(tx: &mut Tx<'_>, asset: &str) -> Result<f64> {
    let cash: f64 = sqlx::query_scalar(
        "WITH due AS (
           SELECT t.position_id,f.id AS fid,f.exchange_ts AS settled,
              (f.payload->>'realizedRate')::float8 AS rate,m.id AS mid,(m.payload->>'markPx')::float8 AS mark,
              (SELECT sum(CASE WHEN side='sell' THEN quantity ELSE -quantity END)
               FROM llf_paper_fills x WHERE x.position_id=t.position_id AND x.fill_time < f.exchange_ts) AS qty
           FROM llf_paper_trades t JOIN llf_market_data f ON f.symbol=t.asset||'-USDT-SWAP'
             AND f.channel='funding-rate-history' AND f.quality='ok' AND f.exchange_ts > t.opened_at
             AND f.exchange_ts <= coalesce(t.closed_at,now()) AND nullif(f.payload->>'realizedRate','') IS NOT NULL
           CROSS JOIN LATERAL (SELECT id,payload FROM llf_market_data m WHERE m.symbol=f.symbol AND m.channel='mark-price' AND m.quality='ok'
               AND m.exchange_ts BETWEEN f.exchange_ts-interval '1 second' AND f.exchange_ts
               ORDER BY m.exchange_ts DESC LIMIT 1) m
           WHERE t.asset=$1 AND NOT EXISTS (SELECT 1 FROM llf_paper_ledger l
             WHERE l.position_id=t.position_id AND l.kind='funding' AND l.effective_at=f.exchange_ts)
         ), posted AS (
           INSERT INTO llf_paper_ledger(entry_id,account_id,position_id,kind,effective_at,cash_delta,funding_raw_id,mark_raw_id)
           SELECT gen_random_uuid(),$2,position_id,'funding',settled,qty*mark*rate,fid,mid FROM due
           WHERE qty>0 AND mark>0 ON CONFLICT (position_id,kind,effective_at) DO NOTHING
           RETURNING position_id,cash_delta
         ), by_trade AS (
           SELECT position_id,sum(cash_delta) AS delta FROM posted GROUP BY position_id
         ), updated AS (
           UPDATE llf_paper_trades t SET funding=funding+b.delta FROM by_trade b
           WHERE t.position_id=b.position_id RETURNING b.delta
         ) SELECT coalesce(sum(delta),0)::float8 FROM updated")
        .bind(asset).bind(ACCOUNT).fetch_one(&mut **tx).await?;
    Ok(cash)
}

async fn post_execution(
    tx: &mut Tx<'_>,
    account: &mut AccountState,
    positions: &mut Vec<SimulatedPosition>,
    execution: Execution,
    ledger: &mut LedgerService,
    book_id: i64,
    fee_bps: f64,
) -> Result<bool> {
    let f = &execution.fill;
    let opening = f.side == "sell";
    let complete = (execution.requested - f.filled_quantity).abs() < 1e-9;
    let old_margin = account.margin_used;
    let old_cash = account.cash;
    let id = positions
        .iter()
        .find(|p| p.asset == f.asset)
        .map(|p| p.position_id);
    let (created, _) = ledger.apply_fill(f, account, positions);
    let position_id = id
        .or_else(|| created.map(|p| p.position_id))
        .context("LLF fill has no position")?;
    let p = positions
        .iter()
        .find(|p| p.position_id == position_id)
        .context("LLF position missing after fill")?;
    let delta = account.cash - old_cash;
    // The ledger uses VWAP, so the depth cost is reported for attribution and never charged twice.
    sqlx::query("INSERT INTO llf_paper_trades(position_id,asset,entry_signal,opened_at,closed_at,position,entry_quantity,
           fee_bps,fees,slippage_cost,net_fills) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         ON CONFLICT(position_id) DO UPDATE SET position=EXCLUDED.position,closed_at=EXCLUDED.closed_at,
           fees=llf_paper_trades.fees+EXCLUDED.fees,slippage_cost=llf_paper_trades.slippage_cost+EXCLUDED.slippage_cost,
           net_fills=llf_paper_trades.net_fills+EXCLUDED.net_fills")
        .bind(position_id).bind(&f.asset).bind(p.open_signal_id).bind(p.opened_at).bind(p.closed_at)
        .bind(Json(p)).bind(f.filled_quantity).bind(fee_bps).bind(f.fee).bind(f.slippage_cost).bind(delta)
        .execute(&mut **tx).await?;
    sqlx::query("INSERT INTO llf_paper_orders(order_id,signal_id,book_id,created_at,side,requested_qty,filled_qty,status,risk_check)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(f.order_id).bind(f.signal_id).bind(book_id).bind(f.fill_time).bind(&f.side).bind(execution.requested)
        .bind(f.filled_quantity).bind(if complete {"filled"} else {"partial"})
        .bind(json!({"approved":true,"leverage":1,"fee_bps":fee_bps,"margin_before":old_margin,"cash_before":old_cash}))
        .execute(&mut **tx).await?;
    sqlx::query("INSERT INTO llf_paper_fills(fill_id,order_id,position_id,side,quantity,price,fee,slippage_bps,slippage_cost,levels,fill_time)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(f.fill_id).bind(f.order_id).bind(position_id).bind(&f.side).bind(f.filled_quantity).bind(f.filled_price)
        .bind(f.fee).bind(f.slippage_bps).bind(f.slippage_cost).bind(Json(&execution.consumed)).bind(f.fill_time)
        .execute(&mut **tx).await?;
    let entry = ledger
        .entries()
        .last()
        .context("LLF ledger entry missing")?;
    sqlx::query("INSERT INTO llf_paper_ledger(entry_id,account_id,position_id,fill_id,kind,effective_at,cash_delta,margin_delta)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(entry.entry_id).bind(ACCOUNT).bind(position_id).bind(f.fill_id).bind(if opening {"open"} else {"close"})
        .bind(f.fill_time).bind(delta).bind(account.margin_used-old_margin).execute(&mut **tx).await?;
    Ok(opening || p.closed_at.is_some())
}

pub async fn on_snapshot(db: &PgPool, id: i64) -> Result<()> {
    let snap = sqlx::query(
        "SELECT s.asset,s.observed_at,s.percentile,s.quality,
               a.pending_signal IS NOT NULL OR EXISTS (
                    SELECT 1 FROM llf_paper_trades t WHERE t.asset=s.asset
               ) AS has_activity
           FROM llf_snapshots s JOIN llf_paper_assets a ON a.asset=s.asset WHERE s.id=$1",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    let asset: String = snap.try_get("asset")?;
    let tick: DateTime<Utc> = snap.try_get("observed_at")?;
    let quality: String = snap.try_get("quality")?;
    let pct = snap
        .try_get::<Option<f64>, _>("percentile")?
        .filter(|p| (0.0..=100.0).contains(p));
    // For the initial unconfirmed/baseline phase, one indexed read per asset is sufficient.
    // A dangling entry or any past trade still needs exit handling or delayed actual funding.
    if pct.is_none() && !snap.try_get::<bool, _>("has_activity")? {
        return Ok(());
    }
    let settings = Settings::load();
    let mut tx = db.begin().await?;
    // One row serializes all three assets: two concurrent entries cannot spend the same free collateral.
    let state: Json<AccountState> =
        sqlx::query_scalar("SELECT state FROM llf_paper_accounts WHERE account_id=$1 FOR UPDATE")
            .bind(ACCOUNT)
            .fetch_one(&mut *tx)
            .await?;
    let mut account = state.0;
    let row = sqlx::query("SELECT armed,last_tick,pending_signal,last_book_id FROM llf_paper_assets WHERE asset=$1 FOR UPDATE")
        .bind(&asset).fetch_one(&mut *tx).await?;
    let new_factor_minute = row
        .try_get::<Option<DateTime<Utc>>, _>("last_tick")?
        .is_none_or(|t| t < tick);
    let mut armed = row.try_get::<bool, _>("armed")?;
    let mut pending: Option<Uuid> = row.try_get("pending_signal")?;
    let mut last_book: Option<i64> = row.try_get("last_book_id")?;
    let now = Utc::now();
    let current = tick <= now && tick >= now - Duration::seconds(ENTRY_TTL);
    let ready = quality == "ready" && current;

    let trades =
        sqlx::query("SELECT position,fee_bps FROM llf_paper_trades WHERE closed_at IS NULL")
            .fetch_all(&mut *tx)
            .await?;
    let mut positions = Vec::new();
    let mut fee_on_open = None;
    for t in trades {
        let p = t.try_get::<Json<SimulatedPosition>, _>("position")?.0;
        if p.asset == asset {
            fee_on_open = Some(t.try_get::<f64, _>("fee_bps")?);
        }
        positions.push(p);
    }
    let data = market(&mut tx, &asset).await?;
    let paid = settle_funding(&mut tx, &asset).await?;
    account.cash += paid;
    account.realized_pnl += paid;
    let position = positions.iter().find(|p| p.asset == asset).cloned();
    if let (Some(p), Some(mark)) = (positions.iter_mut().find(|p| p.asset == asset), data.mark) {
        p.update_mark_to_market(mark);
        sqlx::query("UPDATE llf_paper_trades SET position=$2,mae_usdt=greatest(mae_usdt,$3) WHERE position_id=$1")
            .bind(p.position_id).bind(Json(&*p)).bind((-p.unrealized_pnl).max(0.0)).execute(&mut *tx).await?;
    }
    account.unrealized_pnl = positions.iter().map(|p| p.unrealized_pnl).sum();
    account.recompute_total_equity();

    let low = ready && pct.is_some_and(|p| p < 70.0);
    // A factor minute is evaluated once. Matching, funding and the time limit use live ticks.
    if new_factor_minute && low {
        armed = true;
    }
    if new_factor_minute && armed && pct.is_some_and(|p| p >= 90.0) {
        armed = false;
        let rejected = if !ready {
            Some("data_unready")
        } else if position.is_some() {
            Some("already_open")
        } else if !settings.enabled {
            Some("observe_only")
        } else if settings.size.is_none() || settings.fee.is_none() {
            Some("configuration_missing")
        } else if pending.is_some() {
            Some("signal_pending")
        } else {
            None
        };
        let sid = new_signal(
            &mut tx,
            id,
            &asset,
            "entry",
            tick,
            pct,
            data.mark,
            if rejected.is_some() {
                "skipped"
            } else {
                "pending"
            },
            rejected.unwrap_or("p90"),
            &settings,
            &quality,
        )
        .await?;
        if rejected.is_none() {
            pending = Some(sid);
        }
    }
    if let Some(p) = &position {
        let timed_out = now >= p.opened_at + Duration::minutes(15);
        if pending.is_none() && (timed_out || (new_factor_minute && low)) {
            pending = Some(
                new_signal(
                    &mut tx,
                    id,
                    &asset,
                    "exit",
                    tick,
                    pct,
                    data.mark,
                    "pending",
                    if timed_out { "time_limit" } else { "below_p70" },
                    &settings,
                    &quality,
                )
                .await?,
            );
        }
    }
    if let Some(signal) = pending {
        let s = sqlx::query("SELECT kind,decided_at,fee_bps,order_usdt,reason FROM llf_paper_signals WHERE signal_id=$1")
            .bind(signal).fetch_one(&mut *tx).await?;
        let opening = s.try_get::<&str, _>("kind")? == "entry";
        let decision: DateTime<Utc> = s.try_get("decided_at")?;
        let fee = if opening {
            s.try_get("fee_bps")?
        } else {
            fee_on_open
        };
        let reason = if opening && !settings.enabled {
            Some("observe_only")
        } else if opening && (!ready || pct.is_none() || low || data.mark.is_none()) {
            Some("data_unready")
        } else if opening && now > decision + Duration::seconds(ENTRY_TTL) {
            Some("book_timeout")
        } else {
            None
        };
        if let Some(reason) = reason {
            finish_signal(&mut tx, signal, "skipped", reason).await?;
            pending = None;
        } else if let (Some((bid, exchange_ts, received, book)), Some(contract), Some(fee_bps)) =
            (&data.book, data.contract, fee)
        {
            // Both timestamps must postdate signal creation, and each raw book may be consumed once per asset.
            if *exchange_ts > decision && *received > decision && last_book != Some(*bid) {
                let mut ledger = LedgerService::new(MatchingConfig {
                    fee_taker_bps: fee_bps,
                    ..Default::default()
                });
                let exec = match_book(
                    book,
                    contract,
                    position.as_ref(),
                    s.try_get("order_usdt")?,
                    &asset,
                    signal,
                    now,
                    &ledger,
                );
                last_book = Some(*bid);
                match exec {
                    Ok(e)
                        if opening
                            && e.fill.notional.unwrap_or_default() + e.fill.fee
                                > account.cash.min(account.total_equity) - account.margin_used =>
                    {
                        finish_signal(&mut tx, signal, "skipped", "insufficient_margin").await?;
                        pending = None;
                    }
                    Ok(e) => {
                        let reason: String = s.try_get("reason")?;
                        let fully_filled = (e.requested - e.fill.filled_quantity).abs() < 1e-9;
                        if post_execution(
                            &mut tx,
                            &mut account,
                            &mut positions,
                            e,
                            &mut ledger,
                            *bid,
                            fee_bps,
                        )
                        .await?
                        {
                            finish_signal(
                                &mut tx,
                                signal,
                                if fully_filled { "filled" } else { "partial" },
                                &reason,
                            )
                            .await?;
                            pending = None;
                        }
                    }
                    Err(reason) if opening => {
                        finish_signal(&mut tx, signal, "skipped", reason).await?;
                        pending = None;
                    }
                    Err(_) => {} // A pending exit keeps retrying with fresh executable depth after an outage.
                }
            }
        }
    }
    // Preserve the last known marks of other assets; an unavailable quote cannot erase an open loss.
    if let (Some(mark), Some(p)) = (
        data.mark,
        positions
            .iter_mut()
            .find(|p| p.asset == asset && p.closed_at.is_none()),
    ) {
        p.update_mark_to_market(mark);
        sqlx::query("UPDATE llf_paper_trades SET position=$2,mae_usdt=greatest(mae_usdt,$3) WHERE position_id=$1")
            .bind(p.position_id).bind(Json(&*p)).bind((-p.unrealized_pnl).max(0.0)).execute(&mut *tx).await?;
    }
    account.unrealized_pnl = positions
        .iter()
        .filter(|p| p.closed_at.is_none())
        .map(|p| p.unrealized_pnl)
        .sum();
    account.total_notional = positions
        .iter()
        .filter(|p| p.closed_at.is_none())
        .filter_map(|p| p.notional)
        .sum();
    account.timestamp = now;
    account.recompute_total_equity();
    sqlx::query("UPDATE llf_paper_accounts SET state=$2,max_drawdown_pct=greatest(max_drawdown_pct,$3) WHERE account_id=$1")
        .bind(ACCOUNT).bind(Json(&account)).bind(account.drawdown_pct*100.0).execute(&mut *tx).await?;
    sqlx::query("UPDATE llf_paper_assets SET last_tick=greatest(last_tick,$2),armed=$3,pending_signal=$4,last_book_id=$5 WHERE asset=$1")
        .bind(&asset).bind(tick).bind(armed).bind(pending).bind(last_book).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn dashboard(db: &PgPool) -> Result<Value> {
    let settings = Settings::load();
    let a =
        sqlx::query("SELECT state,max_drawdown_pct FROM llf_paper_accounts WHERE account_id=$1")
            .bind(ACCOUNT)
            .fetch_one(db)
            .await?;
    let account = a.try_get::<Json<AccountState>, _>("state")?.0;
    let mut metrics: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM (SELECT count(*) AS entries,
        count(closed_at) AS closed_trades,100.0*count(*) FILTER (WHERE closed_at IS NOT NULL AND net_fills+funding>0)
           /nullif(count(closed_at),0) AS win_rate,avg(extract(epoch from closed_at-opened_at)) AS avg_holding_seconds,
        max(mae_usdt) AS max_adverse_usdt,coalesce(sum(fees),0) AS total_fees,
        coalesce(sum(slippage_cost),0) AS slippage_cost,coalesce(sum(funding),0) AS funding,
        count(*) FILTER (WHERE EXISTS(SELECT 1 FROM llf_market_data m WHERE m.symbol=t.asset||'-USDT-SWAP'
            AND m.channel='liquidation-orders' AND m.quality='ok' AND m.exchange_ts BETWEEN t.opened_at AND coalesce(t.closed_at,now()))) AS liquidation_hits
        FROM llf_paper_trades t) s").fetch_one(db).await?;
    metrics["max_drawdown_pct"] = json!(a.try_get::<f64, _>("max_drawdown_pct")?);
    metrics["net_pnl"] = json!(account.total_equity - account.initial_equity);
    let trades: Vec<Value> = sqlx::query_scalar("WITH recent AS (
          SELECT * FROM llf_paper_trades ORDER BY opened_at DESC LIMIT 50
        ) SELECT to_jsonb(s) FROM (SELECT t.position_id,t.asset,t.opened_at,t.closed_at,t.entry_quantity AS quantity,
           (t.position->>'avg_entry_price')::float8 AS entry_price,t.fees,t.slippage_cost,t.funding,t.mae_usdt,
           CASE WHEN t.closed_at IS NULL THEN 'open' ELSE 'closed' END AS status,
           t.net_fills+t.funding+coalesce((t.position->>'unrealized_pnl')::float8,0) AS net_pnl,
           liq.exchange_ts AS liquidation_at,CASE WHEN liq.exchange_ts IS NOT NULL THEN true END AS liquidation_hit,
           CASE WHEN anchor.px>0 THEN (terminal.px/anchor.px-1)*100 END AS post_liquidation_return_pct,
           CASE WHEN terminal.px IS NOT NULL THEN loss.pct END AS post_liquidation_drawdown_pct,
           evidence.fills, gaps.cnt AS pending_funding_settlements
          FROM recent t
          LEFT JOIN LATERAL (SELECT exchange_ts FROM llf_market_data m WHERE m.symbol=t.asset||'-USDT-SWAP'
             AND m.channel='liquidation-orders' AND m.quality='ok' AND m.exchange_ts BETWEEN t.opened_at AND coalesce(t.closed_at,now())
             ORDER BY exchange_ts LIMIT 1) liq ON true
          LEFT JOIN LATERAL (SELECT (payload->>'markPx')::float8 AS px FROM llf_market_data m
             WHERE m.symbol=t.asset||'-USDT-SWAP' AND m.channel='mark-price' AND m.quality='ok' AND m.exchange_ts
             BETWEEN liq.exchange_ts AND liq.exchange_ts+interval '2 seconds' ORDER BY exchange_ts LIMIT 1) anchor ON true
          LEFT JOIN LATERAL (SELECT (payload->>'markPx')::float8 AS px FROM llf_market_data m
             WHERE m.symbol=t.asset||'-USDT-SWAP' AND m.channel='mark-price' AND m.quality='ok' AND m.exchange_ts
             BETWEEN liq.exchange_ts+interval '15 minutes' AND liq.exchange_ts+interval '15 minutes 2 seconds'
             ORDER BY exchange_ts LIMIT 1) terminal ON true
          LEFT JOIN LATERAL (SELECT max((1-x.px/nullif(x.peak,0))*100) AS pct FROM (
             SELECT (payload->>'markPx')::float8 AS px,max((payload->>'markPx')::float8) OVER(ORDER BY exchange_ts,id) AS peak
             FROM llf_market_data m WHERE m.symbol=t.asset||'-USDT-SWAP' AND m.channel='mark-price' AND m.quality='ok'
             AND m.exchange_ts BETWEEN liq.exchange_ts AND liq.exchange_ts+interval '15 minutes') x) loss ON true
          LEFT JOIN LATERAL (SELECT jsonb_agg(jsonb_build_object('fill_id',f.fill_id,'order_id',f.order_id,
              'signal_id',s.signal_id,'snapshot_id',s.snapshot_id,'ledger_entry',l.entry_id,'risk_check',o.risk_check,
              'book_id',o.book_id,'side',f.side,'requested_qty',o.requested_qty,'quantity',f.quantity,
              'price',f.price,'fee',f.fee,'slippage_bps',f.slippage_bps,'levels',f.levels)
              ORDER BY f.fill_time) AS fills FROM llf_paper_fills f JOIN llf_paper_orders o USING(order_id)
              JOIN llf_paper_signals s USING(signal_id) JOIN llf_paper_ledger l USING(fill_id)
              WHERE f.position_id=t.position_id) evidence ON true
          LEFT JOIN LATERAL (SELECT count(DISTINCT f.exchange_ts) AS cnt FROM llf_market_data f
              WHERE f.symbol=t.asset||'-USDT-SWAP' AND f.channel='funding-rate-history'
                AND f.exchange_ts > t.opened_at AND f.exchange_ts <= coalesce(t.closed_at,now())
                AND NOT EXISTS(SELECT 1 FROM llf_paper_ledger l WHERE l.position_id=t.position_id AND l.kind='funding'
                     AND l.effective_at=f.exchange_ts)) gaps ON true ORDER BY t.opened_at DESC) s").fetch_all(db).await?;
    // Keep high events with rejected risk checks or no order. Score replication must include
    // these observations; the outcome is null until an actual 15-minute mark is archived.
    let events: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(s) FROM (SELECT e.signal_id,e.asset,e.observed_at,
           e.percentile,e.reason,e.status,e.entry_mark,e.snapshot_id,e.risk_check,
           CASE WHEN e.entry_mark>0 THEN (outcome.px/e.entry_mark-1)*100 END AS return_15m_pct
         FROM (SELECT * FROM llf_paper_signals WHERE kind='entry' ORDER BY observed_at DESC LIMIT 100) e
         LEFT JOIN LATERAL (SELECT (m.payload->>'markPx')::float8 AS px FROM llf_market_data m
            WHERE m.symbol=e.asset||'-USDT-SWAP' AND m.channel='mark-price' AND m.quality='ok'
              AND m.exchange_ts BETWEEN e.observed_at+interval '15 minutes' AND e.observed_at+interval '15 minutes 2 seconds'
            ORDER BY m.exchange_ts LIMIT 1) outcome ON true ORDER BY e.observed_at DESC) s").fetch_all(db).await?;
    let positions: Vec<Value> = sqlx::query_scalar(
        "SELECT position FROM llf_paper_trades WHERE closed_at IS NULL ORDER BY asset",
    )
    .fetch_all(db)
    .await?;
    Ok(
        json!({"account_id":ACCOUNT,"enabled":settings.enabled,"order_usdt":settings.size,"fee_bps":settings.fee,
        "initial_equity":account.initial_equity,"equity":account.total_equity,"cash":account.cash,
        "margin_used":account.margin_used,"metrics":metrics,"trades":trades,"events":events,"positions":positions}),
    )
}
