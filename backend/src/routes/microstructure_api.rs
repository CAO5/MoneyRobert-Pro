//! Microstructure Data REST API
//! 微结构数据 API
//!
//! 依据《系统评估与演进规划》第二阶段任务2：
//!   "增加订单簿、成交、清算、基差和跨交易所数据"
//!
//! 提供端点：
//! - GET  /microstructure/orderbook/{symbol}                查询最新订单簿快照
//! - GET  /microstructure/orderbook/{symbol}/history        查询订单簿历史
//! - POST /microstructure/orderbook                          保存订单簿快照
//! - GET  /microstructure/trades/{symbol}                   查询逐笔成交
//! - POST /microstructure/trades                             批量保存逐笔成交
//! - GET  /microstructure/trades/{symbol}/cvd               计算 CVD
//! - GET  /microstructure/liquidations                      查询清算事件
//! - POST /microstructure/liquidations                       保存清算事件
//! - GET  /microstructure/basis/{symbol}                    查询基差数据
//! - POST /microstructure/basis                              保存基差数据
//! - POST /microstructure/basis/compute                     根据价格计算基差

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::microstructure::{
    self, BasisData, LiquidationEvent, OrderbookSnapshot, TradeTick,
};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        // 订单簿
        .route("/orderbook/{symbol}", get(get_latest_orderbook))
        .route("/orderbook/{symbol}/history", get(list_orderbook_history))
        .route("/orderbook", post(save_orderbook))
        // 逐笔成交
        .route("/trades/{symbol}", get(list_trade_ticks))
        .route("/trades", post(save_trade_ticks_batch))
        .route("/trades/{symbol}/cvd", get(compute_cvd_handler))
        // 清算事件
        .route("/liquidations", get(list_liquidations).post(save_liquidation))
        // 基差数据
        .route("/basis/{symbol}", get(list_basis_data))
        .route("/basis", post(save_basis_data))
        .route("/basis/compute", post(compute_basis_handler))
}

// ============================================================
// 订单簿
// ============================================================

#[derive(Debug, Deserialize)]
struct OrderbookHistoryQuery {
    exchange: Option<String>,
    limit: Option<i64>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
}

async fn get_latest_orderbook(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let snap = microstructure::get_latest_orderbook(&state.db_pool, &symbol)
        .await
        .map_err(|e| AppError::Database(e))?
        .ok_or_else(|| AppError::NotFound("orderbook snapshot not found".into()))?;

    Ok(Json(serde_json::to_value(&snap).unwrap_or_default()))
}

async fn list_orderbook_history(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(q): Query<OrderbookHistoryQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 1000);
    let exchange = q.exchange.as_deref().unwrap_or("okx");

    let rows = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM orderbook_snapshots
           WHERE symbol = $1 AND exchange = $2"#,
    )
    .bind(&symbol)
    .bind(exchange)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let snaps = if let (Some(start), Some(end)) = (q.start_time, q.end_time) {
        sqlx::query_as::<_, OrderbookSnapshot>(
            r#"SELECT * FROM orderbook_snapshots
               WHERE symbol = $1 AND exchange = $2 AND timestamp >= $3 AND timestamp <= $4
               ORDER BY timestamp DESC LIMIT $5"#,
        )
        .bind(&symbol)
        .bind(exchange)
        .bind(start)
        .bind(end)
        .bind(limit)
        .fetch_all(&state.db_pool)
        .await
    } else {
        sqlx::query_as::<_, OrderbookSnapshot>(
            r#"SELECT * FROM orderbook_snapshots
               WHERE symbol = $1 AND exchange = $2
               ORDER BY timestamp DESC LIMIT $3"#,
        )
        .bind(&symbol)
        .bind(exchange)
        .bind(limit)
        .fetch_all(&state.db_pool)
        .await
    }
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "exchange": exchange,
        "total": rows,
        "returned": snaps.len(),
        "snapshots": snaps,
    })))
}

#[derive(Debug, Deserialize)]
struct SaveOrderbookRequest {
    symbol: String,
    exchange: Option<String>,
    /// 买盘 [(price, size), ...]
    bids: Vec<(f64, f64)>,
    /// 卖盘 [(price, size), ...]
    asks: Vec<(f64, f64)>,
    timestamp: Option<DateTime<Utc>>,
}

async fn save_orderbook(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<SaveOrderbookRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.bids.is_empty() || req.asks.is_empty() {
        return Err(AppError::Validation("bids and asks must not be empty".into()));
    }

    let exchange = req.exchange.as_deref().unwrap_or("okx");
    let timestamp = req.timestamp.unwrap_or_else(Utc::now);

    let snap = microstructure::build_orderbook_snapshot(
        &req.symbol,
        exchange,
        &req.bids,
        &req.asks,
        timestamp,
    );

    let snapshot_id = microstructure::save_orderbook_snapshot(&state.db_pool, &snap)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "snapshot_id": snapshot_id,
        "symbol": snap.symbol,
        "exchange": snap.exchange,
        "best_bid": snap.best_bid,
        "best_ask": snap.best_ask,
        "spread": snap.spread,
        "spread_bps": snap.spread_bps,
        "mid_price": snap.mid_price,
        "depth_imbalance_5": snap.depth_imbalance_5,
        "timestamp": snap.timestamp.to_rfc3339(),
    })))
}

// ============================================================
// 逐笔成交
// ============================================================

#[derive(Debug, Deserialize)]
struct TradeTicksQuery {
    exchange: Option<String>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

async fn list_trade_ticks(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(q): Query<TradeTicksQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 5000);
    let now = Utc::now();
    let start = q.start_time.unwrap_or(now - chrono::Duration::hours(1));
    let end = q.end_time.unwrap_or(now);

    let ticks = microstructure::list_trade_ticks(&state.db_pool, &symbol, start, end, limit)
        .await
        .map_err(|e| AppError::Database(e))?;

    let cvd = microstructure::compute_cvd(&ticks);

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "start_time": start.to_rfc3339(),
        "end_time": end.to_rfc3339(),
        "count": ticks.len(),
        "cvd": cvd,
        "ticks": ticks,
    })))
}

#[derive(Debug, Deserialize)]
struct SaveTradeTicksRequest {
    ticks: Vec<SaveTradeTickInput>,
}

#[derive(Debug, Deserialize)]
struct SaveTradeTickInput {
    symbol: String,
    exchange: Option<String>,
    trade_id: Option<String>,
    timestamp: DateTime<Utc>,
    price: f64,
    size: f64,
    side: String,
    is_buyer_maker: Option<bool>,
}

async fn save_trade_ticks_batch(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<SaveTradeTicksRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.ticks.is_empty() {
        return Err(AppError::Validation("ticks must not be empty".into()));
    }
    if req.ticks.len() > 1000 {
        return Err(AppError::Validation("batch size must not exceed 1000".into()));
    }

    let mut saved = 0u64;
    let mut errors: Vec<String> = Vec::new();

    for (idx, t) in req.ticks.iter().enumerate() {
        if t.size <= 0.0 || t.price <= 0.0 {
            errors.push(format!("tick[{}]: price and size must be positive", idx));
            continue;
        }
        let side = t.side.to_lowercase();
        if side != "buy" && side != "sell" {
            errors.push(format!("tick[{}]: side must be 'buy' or 'sell'", idx));
            continue;
        }

        let tick = TradeTick {
            tick_id: 0,
            symbol: t.symbol.clone(),
            exchange: t.exchange.clone().unwrap_or_else(|| "okx".into()),
            trade_id: t.trade_id.clone(),
            timestamp: t.timestamp,
            price: t.price,
            size: t.size,
            notional: t.price * t.size,
            side,
            is_buyer_maker: t.is_buyer_maker.unwrap_or(false),
            created_at: Utc::now(),
        };

        match microstructure::save_trade_tick(&state.db_pool, &tick).await {
            Ok(_) => saved += 1,
            Err(e) => errors.push(format!("tick[{}]: {}", idx, e)),
        }
    }

    Ok(Json(serde_json::json!({
        "saved": saved,
        "failed": errors.len(),
        "errors": errors,
    })))
}

#[derive(Debug, Deserialize)]
struct CvdQuery {
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CvdResponse {
    symbol: String,
    start_time: String,
    end_time: String,
    tick_count: usize,
    cvd: f64,
    buy_volume: f64,
    sell_volume: f64,
    buy_notional: f64,
    sell_notional: f64,
}

async fn compute_cvd_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(q): Query<CvdQuery>,
) -> Result<Json<CvdResponse>> {
    let limit = q.limit.unwrap_or(1000).clamp(1, 10000);
    let now = Utc::now();
    let start = q.start_time.unwrap_or(now - chrono::Duration::hours(1));
    let end = q.end_time.unwrap_or(now);

    let ticks = microstructure::list_trade_ticks(&state.db_pool, &symbol, start, end, limit)
        .await
        .map_err(|e| AppError::Database(e))?;

    let cvd = microstructure::compute_cvd(&ticks);

    let mut buy_volume = 0.0;
    let mut sell_volume = 0.0;
    let mut buy_notional = 0.0;
    let mut sell_notional = 0.0;
    for t in &ticks {
        if t.side == "buy" {
            buy_volume += t.size;
            buy_notional += t.notional;
        } else {
            sell_volume += t.size;
            sell_notional += t.notional;
        }
    }

    Ok(Json(CvdResponse {
        symbol,
        start_time: start.to_rfc3339(),
        end_time: end.to_rfc3339(),
        tick_count: ticks.len(),
        cvd,
        buy_volume,
        sell_volume,
        buy_notional,
        sell_notional,
    }))
}

// ============================================================
// 清算事件
// ============================================================

#[derive(Debug, Deserialize)]
struct LiquidationsQuery {
    symbol: Option<String>,
    exchange: Option<String>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

async fn list_liquidations(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(q): Query<LiquidationsQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let now = Utc::now();
    let start = q.start_time.unwrap_or(now - chrono::Duration::hours(24));
    let end = q.end_time.unwrap_or(now);

    let events = microstructure::list_liquidation_events(
        &state.db_pool,
        q.symbol.as_deref(),
        start,
        end,
        limit,
    )
    .await
    .map_err(|e| AppError::Database(e))?;

    let total_notional: f64 = events.iter().map(|e| e.notional).sum();
    let long_liq: f64 = events
        .iter()
        .filter(|e| e.side == "long")
        .map(|e| e.notional)
        .sum();
    let short_liq: f64 = events
        .iter()
        .filter(|e| e.side == "short")
        .map(|e| e.notional)
        .sum();

    Ok(Json(serde_json::json!({
        "start_time": start.to_rfc3339(),
        "end_time": end.to_rfc3339(),
        "count": events.len(),
        "total_notional": total_notional,
        "long_liquidation_notional": long_liq,
        "short_liquidation_notional": short_liq,
        "events": events,
    })))
}

#[derive(Debug, Deserialize)]
struct SaveLiquidationRequest {
    symbol: String,
    exchange: Option<String>,
    timestamp: DateTime<Utc>,
    side: String,
    price: f64,
    size: f64,
    liquidation_type: Option<String>,
}

async fn save_liquidation(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<SaveLiquidationRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.price <= 0.0 || req.size <= 0.0 {
        return Err(AppError::Validation("price and size must be positive".into()));
    }
    let side = req.side.to_lowercase();
    if side != "long" && side != "short" {
        return Err(AppError::Validation("side must be 'long' or 'short'".into()));
    }

    let event = LiquidationEvent {
        event_id: 0,
        symbol: req.symbol.clone(),
        exchange: req.exchange.unwrap_or_else(|| "okx".into()),
        timestamp: req.timestamp,
        side,
        price: req.price,
        size: req.size,
        notional: req.price * req.size,
        liquidation_type: req.liquidation_type,
        created_at: Utc::now(),
    };

    let event_id = microstructure::save_liquidation_event(&state.db_pool, &event)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "event_id": event_id,
        "symbol": event.symbol,
        "notional": event.notional,
        "side": event.side,
    })))
}

// ============================================================
// 基差数据
// ============================================================

#[derive(Debug, Deserialize)]
struct BasisQuery {
    exchange: Option<String>,
    limit: Option<i64>,
}

async fn list_basis_data(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(q): Query<BasisQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let data = microstructure::list_basis_data(&state.db_pool, &symbol, limit)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "count": data.len(),
        "basis_data": data,
    })))
}

#[derive(Debug, Deserialize)]
struct SaveBasisRequest {
    symbol: String,
    exchange: Option<String>,
    timestamp: DateTime<Utc>,
    spot_price: Option<f64>,
    perp_price: Option<f64>,
    futures_price: Option<f64>,
    futures_expiry: Option<DateTime<Utc>>,
    funding_rate: Option<f64>,
}

async fn save_basis_data(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<SaveBasisRequest>,
) -> Result<Json<serde_json::Value>> {
    let spot = req.spot_price.unwrap_or(0.0);
    let perp = req.perp_price.unwrap_or(0.0);
    let (perp_basis, perp_basis_pct, futures_basis, futures_basis_pct) =
        microstructure::compute_basis(spot, perp, req.futures_price, req.funding_rate);

    let funding_annualized = req.funding_rate.map(|r| r * 3.0 * 365.0); // 8h × 3 次/天 × 365 天

    let data = BasisData {
        basis_id: 0,
        symbol: req.symbol.clone(),
        exchange: req.exchange.unwrap_or_else(|| "okx".into()),
        timestamp: req.timestamp,
        spot_price: req.spot_price,
        perp_price: req.perp_price,
        futures_price: req.futures_price,
        futures_expiry: req.futures_expiry,
        perp_basis: Some(perp_basis),
        perp_basis_pct: Some(perp_basis_pct),
        futures_basis,
        futures_basis_pct,
        funding_rate: req.funding_rate,
        funding_rate_annualized: funding_annualized,
        created_at: Utc::now(),
    };

    let basis_id = microstructure::save_basis_data(&state.db_pool, &data)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "basis_id": basis_id,
        "symbol": data.symbol,
        "perp_basis": data.perp_basis,
        "perp_basis_pct": data.perp_basis_pct,
        "futures_basis": data.futures_basis,
        "futures_basis_pct": data.futures_basis_pct,
        "funding_rate_annualized": data.funding_rate_annualized,
    })))
}

#[derive(Debug, Deserialize)]
struct ComputeBasisRequest {
    spot_price: f64,
    perp_price: f64,
    futures_price: Option<f64>,
    funding_rate: Option<f64>,
}

async fn compute_basis_handler(
    _user: CurrentUser,
    Json(req): Json<ComputeBasisRequest>,
) -> Result<Json<serde_json::Value>> {
    let (perp_basis, perp_basis_pct, futures_basis, futures_basis_pct) =
        microstructure::compute_basis(
            req.spot_price,
            req.perp_price,
            req.futures_price,
            req.funding_rate,
        );

    let funding_annualized = req.funding_rate.map(|r| r * 3.0 * 365.0);

    Ok(Json(serde_json::json!({
        "perp_basis": perp_basis,
        "perp_basis_pct": perp_basis_pct,
        "futures_basis": futures_basis,
        "futures_basis_pct": futures_basis_pct,
        "funding_rate_annualized": funding_annualized,
    })))
}
