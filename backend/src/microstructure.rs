//! Market Microstructure Data Module
//! 微结构数据模块：订单簿、逐笔成交、清算事件、基差数据
//!
//! 依据《系统评估与演进规划》第二阶段任务2：
//!   "增加订单簿、成交、清算、基差和跨交易所数据"
//!
//! 提供微结构数据的采集、存储和查询能力

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

// ============================================================
// 数据模型
// ============================================================

/// 订单簿快照
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrderbookSnapshot {
    pub snapshot_id: i64,
    pub symbol: String,
    pub exchange: String,
    pub timestamp: DateTime<Utc>,
    pub best_bid: f64,
    pub best_ask: f64,
    pub spread: f64,
    pub spread_bps: f64,
    pub bid_depth_5: Option<f64>,
    pub ask_depth_5: Option<f64>,
    pub bid_depth_10: Option<f64>,
    pub ask_depth_10: Option<f64>,
    pub bid_depth_20: Option<f64>,
    pub ask_depth_20: Option<f64>,
    pub depth_imbalance_5: Option<f64>,
    pub depth_imbalance_10: Option<f64>,
    pub depth_imbalance_20: Option<f64>,
    pub mid_price: f64,
    pub bids: Option<serde_json::Value>,
    pub asks: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// 逐笔成交
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TradeTick {
    pub tick_id: i64,
    pub symbol: String,
    pub exchange: String,
    pub trade_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub side: String,
    pub is_buyer_maker: bool,
    pub created_at: DateTime<Utc>,
}

/// 清算事件
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LiquidationEvent {
    pub event_id: i64,
    pub symbol: String,
    pub exchange: String,
    pub timestamp: DateTime<Utc>,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub liquidation_type: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 基差数据
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BasisData {
    pub basis_id: i64,
    pub symbol: String,
    pub exchange: String,
    pub timestamp: DateTime<Utc>,
    pub spot_price: Option<f64>,
    pub perp_price: Option<f64>,
    pub futures_price: Option<f64>,
    pub futures_expiry: Option<DateTime<Utc>>,
    pub perp_basis: Option<f64>,
    pub perp_basis_pct: Option<f64>,
    pub futures_basis: Option<f64>,
    pub futures_basis_pct: Option<f64>,
    pub funding_rate: Option<f64>,
    pub funding_rate_annualized: Option<f64>,
    pub created_at: DateTime<Utc>,
}

// ============================================================
// 订单簿快照存储
// ============================================================

/// 从原始买卖盘数据构建订单簿快照
pub fn build_orderbook_snapshot(
    symbol: &str,
    exchange: &str,
    bids: &[(f64, f64)], // (price, size)
    asks: &[(f64, f64)],
    timestamp: DateTime<Utc>,
) -> OrderbookSnapshot {
    let best_bid = bids.first().map(|(p, _)| *p).unwrap_or(0.0);
    let best_ask = asks.first().map(|(p, _)| *p).unwrap_or(0.0);
    let spread = best_ask - best_bid;
    let mid_price = (best_bid + best_ask) / 2.0;
    let spread_bps = if mid_price > 0.0 {
        (spread / mid_price) * 10000.0
    } else {
        0.0
    };

    let bid_depth_5 = cumulative_depth(bids, 5);
    let ask_depth_5 = cumulative_depth(asks, 5);
    let bid_depth_10 = cumulative_depth(bids, 10);
    let ask_depth_10 = cumulative_depth(asks, 10);
    let bid_depth_20 = cumulative_depth(bids, 20);
    let ask_depth_20 = cumulative_depth(asks, 20);

    let depth_imbalance_5 = compute_imbalance(bid_depth_5, ask_depth_5);
    let depth_imbalance_10 = compute_imbalance(bid_depth_10, ask_depth_10);
    let depth_imbalance_20 = compute_imbalance(bid_depth_20, ask_depth_20);

    let bids_json = serde_json::to_value(
        bids.iter().map(|(p, s)| vec![*p, *s]).collect::<Vec<_>>(),
    )
    .ok();
    let asks_json = serde_json::to_value(
        asks.iter().map(|(p, s)| vec![*p, *s]).collect::<Vec<_>>(),
    )
    .ok();

    OrderbookSnapshot {
        snapshot_id: 0,
        symbol: symbol.into(),
        exchange: exchange.into(),
        timestamp,
        best_bid,
        best_ask,
        spread,
        spread_bps,
        bid_depth_5,
        ask_depth_5,
        bid_depth_10,
        ask_depth_10,
        bid_depth_20,
        ask_depth_20,
        depth_imbalance_5,
        depth_imbalance_10,
        depth_imbalance_20,
        mid_price,
        bids: bids_json,
        asks: asks_json,
        created_at: Utc::now(),
    }
}

fn cumulative_depth(levels: &[(f64, f64)], count: usize) -> Option<f64> {
    if levels.is_empty() {
        return None;
    }
    Some(levels.iter().take(count).map(|(_, s)| s).sum())
}

fn compute_imbalance(bid: Option<f64>, ask: Option<f64>) -> Option<f64> {
    match (bid, ask) {
        (Some(b), Some(a)) => {
            let total = b + a;
            if total > 0.0 {
                Some((b - a) / total)
            } else {
                Some(0.0)
            }
        }
        _ => None,
    }
}

/// 保存订单簿快照
pub async fn save_orderbook_snapshot(
    pool: &PgPool,
    snap: &OrderbookSnapshot,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        r#"INSERT INTO orderbook_snapshots
           (symbol, exchange, timestamp, best_bid, best_ask, spread, spread_bps,
            bid_depth_5, ask_depth_5, bid_depth_10, ask_depth_10, bid_depth_20, ask_depth_20,
            depth_imbalance_5, depth_imbalance_10, depth_imbalance_20,
            mid_price, bids, asks)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
           RETURNING snapshot_id"#,
    )
    .bind(&snap.symbol)
    .bind(&snap.exchange)
    .bind(snap.timestamp)
    .bind(snap.best_bid)
    .bind(snap.best_ask)
    .bind(snap.spread)
    .bind(snap.spread_bps)
    .bind(snap.bid_depth_5)
    .bind(snap.ask_depth_5)
    .bind(snap.bid_depth_10)
    .bind(snap.ask_depth_10)
    .bind(snap.bid_depth_20)
    .bind(snap.ask_depth_20)
    .bind(snap.depth_imbalance_5)
    .bind(snap.depth_imbalance_10)
    .bind(snap.depth_imbalance_20)
    .bind(snap.mid_price)
    .bind(&snap.bids)
    .bind(&snap.asks)
    .fetch_one(pool)
    .await?;

    Ok(row.get("snapshot_id"))
}

/// 查询最新订单簿快照
pub async fn get_latest_orderbook(
    pool: &PgPool,
    symbol: &str,
) -> Result<Option<OrderbookSnapshot>, sqlx::Error> {
    sqlx::query_as::<_, OrderbookSnapshot>(
        "SELECT * FROM orderbook_snapshots WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1",
    )
    .bind(symbol)
    .fetch_optional(pool)
    .await
}

// ============================================================
// 逐笔成交存储
// ============================================================

/// 保存逐笔成交
pub async fn save_trade_tick(
    pool: &PgPool,
    tick: &TradeTick,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        r#"INSERT INTO trade_ticks
           (symbol, exchange, trade_id, timestamp, price, size, notional, side, is_buyer_maker)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           RETURNING tick_id"#,
    )
    .bind(&tick.symbol)
    .bind(&tick.exchange)
    .bind(&tick.trade_id)
    .bind(tick.timestamp)
    .bind(tick.price)
    .bind(tick.size)
    .bind(tick.notional)
    .bind(&tick.side)
    .bind(tick.is_buyer_maker)
    .fetch_one(pool)
    .await?;

    Ok(row.get("tick_id"))
}

/// 批量保存逐笔成交
pub async fn save_trade_ticks_batch(
    pool: &PgPool,
    ticks: &[TradeTick],
) -> Result<u64, sqlx::Error> {
    let mut count = 0u64;
    for tick in ticks {
        let _ = save_trade_tick(pool, tick).await?;
        count += 1;
    }
    Ok(count)
}

/// 查询逐笔成交
pub async fn list_trade_ticks(
    pool: &PgPool,
    symbol: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<TradeTick>, sqlx::Error> {
    sqlx::query_as::<_, TradeTick>(
        r#"SELECT * FROM trade_ticks
           WHERE symbol = $1 AND timestamp >= $2 AND timestamp <= $3
           ORDER BY timestamp ASC LIMIT $4"#,
    )
    .bind(symbol)
    .bind(start_time)
    .bind(end_time)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// 计算 CVD（累计成交量差）
pub fn compute_cvd(ticks: &[TradeTick]) -> f64 {
    ticks
        .iter()
        .map(|t| {
            let signed_size = if t.side == "buy" { t.size } else { -t.size };
            signed_size
        })
        .sum()
}

// ============================================================
// 清算事件存储
// ============================================================

/// 保存清算事件
pub async fn save_liquidation_event(
    pool: &PgPool,
    event: &LiquidationEvent,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        r#"INSERT INTO liquidation_events
           (symbol, exchange, timestamp, side, price, size, notional, liquidation_type)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING event_id"#,
    )
    .bind(&event.symbol)
    .bind(&event.exchange)
    .bind(event.timestamp)
    .bind(&event.side)
    .bind(event.price)
    .bind(event.size)
    .bind(event.notional)
    .bind(&event.liquidation_type)
    .fetch_one(pool)
    .await?;

    Ok(row.get("event_id"))
}

/// 查询清算事件
pub async fn list_liquidation_events(
    pool: &PgPool,
    symbol: Option<&str>,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<LiquidationEvent>, sqlx::Error> {
    if let Some(sym) = symbol {
        sqlx::query_as::<_, LiquidationEvent>(
            r#"SELECT * FROM liquidation_events
               WHERE symbol = $1 AND timestamp >= $2 AND timestamp <= $3
               ORDER BY timestamp DESC LIMIT $4"#,
        )
        .bind(sym)
        .bind(start_time)
        .bind(end_time)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, LiquidationEvent>(
            r#"SELECT * FROM liquidation_events
               WHERE timestamp >= $1 AND timestamp <= $2
               ORDER BY timestamp DESC LIMIT $3"#,
        )
        .bind(start_time)
        .bind(end_time)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

// ============================================================
// 基差数据存储
// ============================================================

/// 计算基差
pub fn compute_basis(
    spot_price: f64,
    perp_price: f64,
    futures_price: Option<f64>,
    funding_rate: Option<f64>,
) -> (f64, f64, Option<f64>, Option<f64>) {
    let perp_basis = perp_price - spot_price;
    let perp_basis_pct = if spot_price > 0.0 {
        (perp_basis / spot_price) * 100.0
    } else {
        0.0
    };

    let (futures_basis, futures_basis_pct) = if let Some(fp) = futures_price {
        let fb = fp - spot_price;
        let fbp = if spot_price > 0.0 {
            (fb / spot_price) * 100.0
        } else {
            0.0
        };
        (Some(fb), Some(fbp))
    } else {
        (None, None)
    };

    (perp_basis, perp_basis_pct, futures_basis, futures_basis_pct)
}

/// 保存基差数据
pub async fn save_basis_data(
    pool: &PgPool,
    data: &BasisData,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        r#"INSERT INTO basis_data
           (symbol, exchange, timestamp, spot_price, perp_price, futures_price, futures_expiry,
            perp_basis, perp_basis_pct, futures_basis, futures_basis_pct,
            funding_rate, funding_rate_annualized)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
           RETURNING basis_id"#,
    )
    .bind(&data.symbol)
    .bind(&data.exchange)
    .bind(data.timestamp)
    .bind(data.spot_price)
    .bind(data.perp_price)
    .bind(data.futures_price)
    .bind(data.futures_expiry)
    .bind(data.perp_basis)
    .bind(data.perp_basis_pct)
    .bind(data.futures_basis)
    .bind(data.futures_basis_pct)
    .bind(data.funding_rate)
    .bind(data.funding_rate_annualized)
    .fetch_one(pool)
    .await?;

    Ok(row.get("basis_id"))
}

/// 查询基差数据
pub async fn list_basis_data(
    pool: &PgPool,
    symbol: &str,
    limit: i64,
) -> Result<Vec<BasisData>, sqlx::Error> {
    sqlx::query_as::<_, BasisData>(
        "SELECT * FROM basis_data WHERE symbol = $1 ORDER BY timestamp DESC LIMIT $2",
    )
    .bind(symbol)
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_orderbook_snapshot() {
        let bids = vec![
            (100.0, 1.5),
            (99.5, 2.0),
            (99.0, 3.0),
            (98.5, 1.0),
            (98.0, 0.5),
        ];
        let asks = vec![
            (100.5, 1.0),
            (101.0, 2.0),
            (101.5, 1.5),
            (102.0, 0.5),
            (102.5, 1.0),
        ];

        let snap = build_orderbook_snapshot("BTC-USDT-SWAP", "okx", &bids, &asks, Utc::now());

        assert_eq!(snap.best_bid, 100.0);
        assert_eq!(snap.best_ask, 100.5);
        assert!((snap.spread - 0.5).abs() < 1e-9);
        assert!((snap.mid_price - 100.25).abs() < 1e-9);
        assert!(snap.spread_bps > 0.0);

        assert_eq!(snap.bid_depth_5, Some(8.0)); // 1.5+2.0+3.0+1.0+0.5
        assert_eq!(snap.ask_depth_5, Some(6.0)); // 1.0+2.0+1.5+0.5+1.0

        let imbalance = snap.depth_imbalance_5.unwrap();
        assert!(imbalance > 0.0, "买盘深度大于卖盘，imbalance 应为正");
    }

    #[test]
    fn test_build_orderbook_empty() {
        let snap = build_orderbook_snapshot("BTC-USDT", "okx", &[], &[], Utc::now());
        assert_eq!(snap.best_bid, 0.0);
        assert_eq!(snap.best_ask, 0.0);
        assert_eq!(snap.spread, 0.0);
    }

    #[test]
    fn test_compute_cvd() {
        let now = Utc::now();
        let ticks = vec![
            TradeTick {
                tick_id: 0, symbol: "BTC".into(), exchange: "okx".into(),
                trade_id: None, timestamp: now, price: 100.0, size: 1.0,
                notional: 100.0, side: "buy".into(), is_buyer_maker: false,
                created_at: now,
            },
            TradeTick {
                tick_id: 0, symbol: "BTC".into(), exchange: "okx".into(),
                trade_id: None, timestamp: now, price: 101.0, size: 2.0,
                notional: 202.0, side: "sell".into(), is_buyer_maker: true,
                created_at: now,
            },
            TradeTick {
                tick_id: 0, symbol: "BTC".into(), exchange: "okx".into(),
                trade_id: None, timestamp: now, price: 102.0, size: 0.5,
                notional: 51.0, side: "buy".into(), is_buyer_maker: false,
                created_at: now,
            },
        ];

        let cvd = compute_cvd(&ticks);
        // 1.0 - 2.0 + 0.5 = -0.5
        assert!((cvd - (-0.5)).abs() < 1e-9);
    }

    #[test]
    fn test_compute_basis() {
        let (perp_basis, perp_pct, fut_basis, fut_pct) =
            compute_basis(100.0, 101.0, Some(105.0), Some(0.0001));

        assert!((perp_basis - 1.0).abs() < 1e-9);
        assert!((perp_pct - 1.0).abs() < 1e-9);
        assert_eq!(fut_basis, Some(5.0));
        assert!((fut_pct.unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_compute_basis_no_futures() {
        let (perp_basis, perp_pct, fut_basis, fut_pct) =
            compute_basis(50000.0, 50100.0, None, None);

        assert!((perp_basis - 100.0).abs() < 1e-9);
        assert!((perp_pct - 0.2).abs() < 1e-6);
        assert_eq!(fut_basis, None);
        assert_eq!(fut_pct, None);
    }

    #[test]
    fn test_compute_imbalance() {
        assert!((compute_imbalance(Some(10.0), Some(10.0)).unwrap() - 0.0).abs() < 1e-9);
        assert!((compute_imbalance(Some(15.0), Some(5.0)).unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(compute_imbalance(None, Some(5.0)), None);
    }

    #[test]
    fn test_cumulative_depth() {
        let levels = vec![(100.0, 1.0), (99.0, 2.0), (98.0, 3.0)];
        assert_eq!(cumulative_depth(&levels, 2), Some(3.0));
        assert_eq!(cumulative_depth(&levels, 5), Some(6.0));
        assert_eq!(cumulative_depth(&[], 5), None);
    }
}
