use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Router, Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::exchanges::{binance::BinanceClient, ExchangeMarketData};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/prices/{symbol}", get(get_cross_exchange_prices))
        .route("/spreads/{symbol}", get(get_cross_exchange_spreads))
        .route("/collect/{symbol}", post(collect_cross_exchange_data))
        .route("/klines/{symbol}", get(get_exchange_klines))
}

/// 获取跨交易所价格快照
async fn get_cross_exchange_prices(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query(
        r#"SELECT exchange, last_price::float8, bid_price::float8, ask_price::float8,
                  volume_24h::float8, quote_volume_24h::float8, high_24h::float8, low_24h::float8,
                  timestamp
           FROM cross_exchange_prices
           WHERE symbol = $1 AND timestamp > NOW() - INTERVAL '1 hour'
           ORDER BY timestamp DESC"#,
    )
    .bind(&symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    let prices: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "exchange": r.get::<String, _>("exchange"),
                "last_price": r.get::<f64, _>("last_price"),
                "bid_price": r.get::<f64, _>("bid_price"),
                "ask_price": r.get::<f64, _>("ask_price"),
                "volume_24h": r.get::<f64, _>("volume_24h"),
                "quote_volume_24h": r.get::<f64, _>("quote_volume_24h"),
                "high_24h": r.get::<f64, _>("high_24h"),
                "low_24h": r.get::<f64, _>("low_24h"),
                "timestamp": r.get::<chrono::DateTime<chrono::Utc>, _>("timestamp"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "prices": prices,
        "count": prices.len(),
    })))
}

/// 获取跨交易所价差历史
async fn get_cross_exchange_spreads(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(params): Query<SpreadQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = params.limit.unwrap_or(50);
    let rows = sqlx::query(
        r#"SELECT exchange_a, exchange_b, price_a::float8, price_b::float8,
                  spread::float8, spread_pct::float8,
                  best_bid_exchange, best_ask_exchange, timestamp
           FROM cross_exchange_spreads
           WHERE symbol = $1
           ORDER BY timestamp DESC
           LIMIT $2"#,
    )
    .bind(&symbol)
    .bind(limit as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    let spreads: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "exchange_a": r.get::<String, _>("exchange_a"),
                "exchange_b": r.get::<String, _>("exchange_b"),
                "price_a": r.get::<f64, _>("price_a"),
                "price_b": r.get::<f64, _>("price_b"),
                "spread": r.get::<f64, _>("spread"),
                "spread_pct": r.get::<f64, _>("spread_pct"),
                "best_bid_exchange": r.get::<Option<String>, _>("best_bid_exchange"),
                "best_ask_exchange": r.get::<Option<String>, _>("best_ask_exchange"),
                "timestamp": r.get::<chrono::DateTime<chrono::Utc>, _>("timestamp"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "spreads": spreads,
        "count": spreads.len(),
    })))
}

#[derive(Debug, Deserialize)]
struct SpreadQuery {
    limit: Option<u32>,
}

/// 触发跨交易所数据采集
///
/// 从 Binance 获取行情数据并存储，同时计算与 OKX 的价差
async fn collect_cross_exchange_data(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let binance = BinanceClient::new();

    // 从 Binance 获取 ticker
    let binance_ticker = binance
        .get_ticker(&symbol)
        .await
        .map_err(|e| AppError::ExternalApi {
            service: "Binance".to_string(),
            message: e,
        })?;

    // 存储 Binance 价格
    sqlx::query(
        r#"INSERT INTO cross_exchange_prices
           (symbol, exchange, last_price, bid_price, ask_price, volume_24h,
            quote_volume_24h, high_24h, low_24h, timestamp)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
    )
    .bind(&symbol)
    .bind("binance")
    .bind(binance_ticker.last_price)
    .bind(binance_ticker.bid_price)
    .bind(binance_ticker.ask_price)
    .bind(binance_ticker.volume_24h)
    .bind(binance_ticker.quote_volume_24h)
    .bind(binance_ticker.high_24h)
    .bind(binance_ticker.low_24h)
    .bind(binance_ticker.timestamp)
    .execute(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    // 从数据库获取最新的 OKX ticker
    let okx_row = sqlx::query(
        r#"SELECT last_price::float8, bid_price::float8, ask_price::float8, volume_24h::float8
           FROM cross_exchange_prices
           WHERE symbol = $1 AND exchange = 'okx'
           ORDER BY timestamp DESC LIMIT 1"#,
    )
    .bind(&symbol)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    let okx_price = okx_row.as_ref().and_then(|r| r.get::<Option<f64>, _>("last_price"));

    // 如果有 OKX 价格，计算价差
    let spread_info = if let Some(okx_p) = okx_price {
        let binance_p = binance_ticker.last_price;
        let spread = (binance_p - okx_p).abs();
        let spread_pct = if okx_p > 0.0 {
            spread / okx_p * 100.0
        } else {
            0.0
        };
        let best_bid_exchange = if binance_ticker.bid_price > okx_row.as_ref().map(|r| r.get::<f64, _>("bid_price")).unwrap_or(0.0) {
            "binance"
        } else {
            "okx"
        };
        let best_ask_exchange = if binance_ticker.ask_price < okx_row.as_ref().map(|r| r.get::<f64, _>("ask_price")).unwrap_or(f64::MAX) {
            "binance"
        } else {
            "okx"
        };

        // 存储价差
        sqlx::query(
            r#"INSERT INTO cross_exchange_spreads
               (symbol, exchange_a, exchange_b, price_a, price_b, spread, spread_pct,
                best_bid_exchange, best_ask_exchange, timestamp)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())"#,
        )
        .bind(&symbol)
        .bind("okx")
        .bind("binance")
        .bind(okx_p)
        .bind(binance_p)
        .bind(spread)
        .bind(spread_pct)
        .bind(best_bid_exchange)
        .bind(best_ask_exchange)
        .execute(&state.db_pool)
        .await
        .map_err(AppError::Database)?;

        serde_json::json!({
            "okx_price": okx_p,
            "binance_price": binance_p,
            "spread": spread,
            "spread_pct": spread_pct,
            "best_bid_exchange": best_bid_exchange,
            "best_ask_exchange": best_ask_exchange,
        })
    } else {
        serde_json::json!({
            "binance_price": binance_ticker.last_price,
            "note": "No OKX price available for comparison"
        })
    };

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "binance": {
            "last_price": binance_ticker.last_price,
            "bid_price": binance_ticker.bid_price,
            "ask_price": binance_ticker.ask_price,
            "volume_24h": binance_ticker.volume_24h,
            "high_24h": binance_ticker.high_24h,
            "low_24h": binance_ticker.low_24h,
        },
        "spread": spread_info,
    })))
}

/// 获取多交易所 K 线数据
async fn get_exchange_klines(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(params): Query<KlineQuery>,
) -> Result<Json<serde_json::Value>> {
    let exchange = params.exchange.as_deref().unwrap_or("binance");
    let interval = params.interval.as_deref().unwrap_or("1H");
    let limit = params.limit.unwrap_or(100) as i64;

    let rows = sqlx::query(
        r#"SELECT exchange, "interval", open_time, open::float8, high::float8, low::float8,
                  close::float8, volume::float8, quote_volume::float8, is_closed
           FROM exchange_klines
           WHERE symbol = $1 AND exchange = $2 AND "interval" = $3
           ORDER BY open_time DESC LIMIT $4"#,
    )
    .bind(&symbol)
    .bind(exchange)
    .bind(interval)
    .bind(limit)
    .fetch_all(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    let klines: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "exchange": r.get::<String, _>("exchange"),
                "interval": r.get::<String, _>("interval"),
                "open_time": r.get::<chrono::NaiveDateTime, _>("open_time"),
                "open": r.get::<f64, _>("open"),
                "high": r.get::<f64, _>("high"),
                "low": r.get::<f64, _>("low"),
                "close": r.get::<f64, _>("close"),
                "volume": r.get::<f64, _>("volume"),
                "quote_volume": r.get::<f64, _>("quote_volume"),
                "is_closed": r.get::<bool, _>("is_closed"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "exchange": exchange,
        "interval": interval,
        "klines": klines,
        "count": klines.len(),
    })))
}

#[derive(Debug, Deserialize)]
struct KlineQuery {
    exchange: Option<String>,
    interval: Option<String>,
    limit: Option<u32>,
}
