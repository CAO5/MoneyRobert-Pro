use axum::{
    extract::{State, Query, Path},
    routing::get,
    Router,
    Json,
};
use chrono::{NaiveDateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row};

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::routes::trading::get_okx_client;
use crate::schemas::success_response;
use crate::state::AppState;

const CST: FixedOffset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();

fn to_cst(nd: NaiveDateTime) -> String {
    let utc_dt = nd.and_utc();
    let cst_dt = utc_dt.with_timezone(&CST);
    cst_dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn to_utc_ts(nd: NaiveDateTime) -> i64 {
    nd.and_utc().timestamp()
}

fn now_cst() -> String {
    Utc::now().with_timezone(&CST).format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/klines/{symbol}", get(get_klines_by_symbol))
        .route("/klines", get(get_klines))
        .route("/tickers", get(get_tickers))
        .route("/funding-rate/{symbol}", get(get_funding_rate_by_symbol))
        .route("/funding-rates", get(get_funding_rates))
        .route("/open-interest/{symbol}", get(get_open_interest_by_symbol))
        .route("/open-interests", get(get_open_interests))
        .route("/long-short-ratio/{symbol}", get(get_long_short_ratio_by_symbol))
        .route("/long-short-ratio", get(get_long_short_ratio))
        .route("/sentiment", get(get_market_sentiment))
        .route("/ticker/{symbol}", get(get_ticker_by_symbol))
        .route("/candles/{symbol}", get(get_candles_by_symbol))
        .route("/popular-symbols", get(get_popular_symbols))
        .route("/status", get(get_sync_status))
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct KlineData {
    pub id: i64,
    pub symbol: String,
    pub interval: String,
    pub open_time: NaiveDateTime,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct KlineQuery {
    pub symbol: Option<String>,
    pub interval: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct KlineSymbolQuery {
    pub interval: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn get_klines_by_symbol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(query): Query<KlineSymbolQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(100).min(2000) as usize;
    let interval = query.interval.unwrap_or_else(|| "1H".to_string());

    // Try to fetch from OKX API first for real-time data
    let okx_data = match crate::routes::trading::get_okx_client(&state, user.user_id).await {
        Ok(client) => {
            match client.get_candles(&symbol, &interval, Some(limit)).await {
                Ok(candles) => {
                    let klines: Vec<serde_json::Value> = candles.iter().map(|c| {
                        let ts: i64 = c.ts.as_deref().unwrap_or("0").parse().unwrap_or(0) / 1000;
                        let open: f64 = c.o.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                        let high: f64 = c.h.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                        let low: f64 = c.l.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                        let close: f64 = c.c.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                        let vol: f64 = c.vol.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                        serde_json::json!({
                            "open_time": ts,
                            "open": open,
                            "high": high,
                            "low": low,
                            "close": close,
                            "volume": vol,
                        })
                    }).collect();
                    Some(klines)
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch klines from OKX for {}: {}, falling back to DB", symbol, e);
                    None
                }
            }
        }
        Err(_) => None,
    };

    if let Some(klines) = okx_data {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": "OKX real-time data",
            "data": klines,
            "total": klines.len(),
            "source": "okx",
        })));
    }

    // Fallback to database
    let rows = sqlx::query(
        r#"
        SELECT id, symbol, "interval", open_time, open::float8, high::float8, low::float8, close::float8, volume::float8
        FROM klines
        WHERE symbol = $1 AND "interval" = $2
        ORDER BY open_time DESC
        LIMIT $3
        "#,
    )
    .bind(&symbol)
    .bind(&interval)
    .bind(limit as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let klines: Vec<serde_json::Value> = rows.iter().map(|row| {
        let open_time: NaiveDateTime = row.get::<NaiveDateTime, _>("open_time");
        let open_time_ts = open_time.and_utc().timestamp();
        serde_json::json!({
            "open_time": open_time_ts,
            "open": row.get::<f64, _>("open"),
            "high": row.get::<f64, _>("high"),
            "low": row.get::<f64, _>("low"),
            "close": row.get::<f64, _>("close"),
            "volume": row.get::<f64, _>("volume"),
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Database cached data",
        "data": klines,
        "total": klines.len(),
        "source": "database",
    })))
}

async fn get_klines(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<KlineQuery>,
) -> Result<Json<serde_json::Value>> {
    let symbol = query.symbol.unwrap_or_else(|| "BTC-USDT-SWAP".to_string());
    let limit = query.limit.unwrap_or(100).min(2000);
    let offset = query.offset.unwrap_or(0);
    let interval = query.interval.unwrap_or_else(|| "1H".to_string());

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM klines WHERE symbol = $1 AND "interval" = $2"#,
    )
    .bind(&symbol)
    .bind(&interval)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let rows = sqlx::query(
        r#"
        SELECT id, symbol, "interval", open_time, open::float8, high::float8, low::float8, close::float8, volume::float8, created_at
        FROM klines
        WHERE symbol = $1 AND "interval" = $2
        ORDER BY open_time DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(&symbol)
    .bind(&interval)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let klines: Vec<serde_json::Value> = rows.iter().map(|row| {
        let open_time: NaiveDateTime = row.get::<NaiveDateTime, _>("open_time");
        serde_json::json!({
            "id": row.get::<i64, _>("id"),
            "symbol": row.get::<String, _>("symbol"),
            "interval": row.get::<String, _>("interval"),
            "open_time": to_utc_ts(open_time),
            "open": row.get::<f64, _>("open"),
            "high": row.get::<f64, _>("high"),
            "low": row.get::<f64, _>("low"),
            "close": row.get::<f64, _>("close"),
            "volume": row.get::<f64, _>("volume"),
            "is_closed": true,
        })
    }).collect();

    let page = (offset / limit.max(1)) as i32 + 1;
    let total_pages = ((total as f64) / (limit as f64)).ceil() as i32;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Success",
        "data": klines,
        "total": total,
        "page": page,
        "page_size": limit,
        "total_pages": total_pages,
        "timestamp": now_cst()
    })))
}

async fn get_tickers(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let tickers = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT row_to_json(sq) FROM (
            SELECT
                th.symbol,
                COALESCE(th.last, 0)::float8 as last_price,
                COALESCE(th.high_24h, 0)::float8 as high_24h,
                COALESCE(th.low_24h, 0)::float8 as low_24h,
                COALESCE(th.volume_24h, 0)::float8 as volume_24h,
                COALESCE(th.open_24h, 0)::float8 as open_24h,
                COALESCE(
                    (th.last - th.open_24h) / NULLIF(th.open_24h, 0) * 100,
                    0.0
                )::float8 as price_change_percent,
                COALESCE(th.best_bid, 0)::float8 as best_bid,
                COALESCE(th.best_ask, 0)::float8 as best_ask,
                to_char(th.timestamp AT TIME ZONE 'UTC' AT TIME ZONE 'Asia/Shanghai', 'YYYY-MM-DD HH24:MI:SS') as timestamp
            FROM ticker_history th
            INNER JOIN (
                SELECT symbol, MAX(timestamp) as max_ts
                FROM ticker_history
                GROUP BY symbol
            ) latest ON th.symbol = latest.symbol AND th.timestamp = latest.max_ts
            ORDER BY th.symbol
            LIMIT 100
        ) AS sq
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let tickers_arr: Vec<serde_json::Value> = tickers.into_iter().map(|t| {
        let last = t["last_price"].as_f64().unwrap_or(0.0);
        let open_24h = t["open_24h"].as_f64().unwrap_or(0.0);
        let change_24h = last - open_24h;
        let change_percent_24h = if open_24h != 0.0 {
            (last - open_24h) / open_24h * 100.0
        } else {
            0.0
        };
        serde_json::json!({
            "symbol": t["symbol"].as_str().unwrap_or_default(),
            "last": last,
            "best_bid": t["best_bid"].as_f64().unwrap_or(0.0),
            "best_ask": t["best_ask"].as_f64().unwrap_or(0.0),
            "open_24h": open_24h,
            "high_24h": t["high_24h"].as_f64().unwrap_or(0.0),
            "low_24h": t["low_24h"].as_f64().unwrap_or(0.0),
            "volume_24h": t["volume_24h"].as_f64().unwrap_or(0.0),
            "change_24h": change_24h,
            "change_percent_24h": change_percent_24h,
            "timestamp": t["timestamp"].as_str().unwrap_or_default(),
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "tickers": tickers_arr
    })))
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct FundingRateData {
    symbol: String,
    funding_rate: f64,
    funding_time: NaiveDateTime,
    realized_rate: Option<f64>,
    avg_premium_index: Option<f64>,
    created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
struct FundingRateQuery {
    symbol: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FundingRateSymbolQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    start_time: Option<String>,
    end_time: Option<String>,
}

async fn get_funding_rate_by_symbol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(query): Query<FundingRateSymbolQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM funding_rate_history WHERE symbol = $1"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let rates = sqlx::query(
        r#"
        SELECT DISTINCT ON (funding_time)
            symbol,
            funding_rate::float8,
            funding_time,
            realized_rate::float8,
            avg_premium_index::float8,
            created_at
        FROM funding_rate_history
        WHERE symbol = $1
        ORDER BY funding_time DESC, created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&symbol)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = rates.iter().map(|row| {
        serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "funding_rate": row.get::<f64, _>("funding_rate"),
            "funding_time": to_utc_ts(row.get::<NaiveDateTime, _>("funding_time")),
            "realized_rate": row.get::<Option<f64>, _>("realized_rate"),
            "avg_premium_index": row.get::<Option<f64>, _>("avg_premium_index"),
        })
    }).collect();

    let page = (offset / limit.max(1)) as i32 + 1;
    let total_pages = ((total as f64) / (limit as f64)).ceil() as i32;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Success",
        "data": items,
        "total": total,
        "page": page,
        "page_size": limit,
        "total_pages": total_pages,
        "timestamp": now_cst()
    })))
}

async fn get_funding_rates(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<FundingRateQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(50).min(500);

    let rates = sqlx::query(
        r#"
        SELECT
            symbol,
            funding_rate::float8,
            funding_time,
            realized_rate::float8,
            avg_premium_index::float8,
            created_at
        FROM funding_rate_history
        WHERE ($1::text IS NULL OR symbol = $1)
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(query.symbol)
    .bind(limit as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let rates_arr: Vec<serde_json::Value> = rates.iter().map(|row| {
        serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "funding_rate": row.get::<f64, _>("funding_rate"),
            "funding_time": to_utc_ts(row.get::<NaiveDateTime, _>("funding_time")),
            "next_funding_time": null,
            "mark_price": null,
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "rates": rates_arr
    })))
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct OpenInterestData {
    symbol: String,
    open_interest: f64,
    open_interest_value: Option<f64>,
    timestamp: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
struct OpenInterestQuery {
    symbol: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OpenInterestSymbolQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn get_open_interest_by_symbol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(query): Query<OpenInterestSymbolQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(100).min(1000) as usize;

    // Extract currency code from symbol (e.g. "BTC-USDT-SWAP" -> "BTC")
    let ccy = symbol.split('-').next().unwrap_or(&symbol).to_string();

    // Try OKX API first for real-time data
    let okx_data = match get_okx_client(&state, user.user_id).await {
        Ok(client) => {
            match client.get_raw(
                "/api/v5/rubik/stat/contracts/open-interest-volume",
                Some(&[("ccy", ccy.clone()), ("period", "5m".to_string())]),
            ).await {
                Ok(data) => {
                    if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                        let items: Vec<serde_json::Value> = arr.iter()
                            .take(limit)
                            .map(|item| {
                                let parts = item.as_array();
                                let ts = parts.and_then(|p| p.get(0)).and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0) / 1000;
                                let oi = parts.and_then(|p| p.get(1)).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok())
                                    .or_else(|| parts.and_then(|p| p.get(1)).and_then(|v| v.as_f64()))
                                    .unwrap_or(0.0);
                                let vol = parts.and_then(|p| p.get(2)).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok())
                                    .or_else(|| parts.and_then(|p| p.get(2)).and_then(|v| v.as_f64()));
                                serde_json::json!({
                                    "symbol": symbol,
                                    "open_interest": oi,
                                    "open_interest_value": vol,
                                    "timestamp": ts,
                                })
                            }).collect();
                        Some(items)
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch open interest from OKX for {}: {}, falling back to DB", symbol, e);
                    None
                }
            }
        }
        Err(_) => None,
    };

    if let Some(items) = okx_data {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": "OKX real-time data",
            "data": items,
            "total": items.len(),
            "source": "okx",
        })));
    }

    // Fallback to database
    let offset = query.offset.unwrap_or(0);

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM open_interests WHERE symbol = $1"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let data = sqlx::query(
        r#"
        SELECT
            symbol,
            open_interest::float8,
            open_interest_value::float8,
            timestamp
        FROM open_interests
        WHERE symbol = $1
        ORDER BY timestamp DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&symbol)
    .bind(limit as i64)
    .bind(offset)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = data.iter().map(|row| {
        serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "open_interest": row.get::<f64, _>("open_interest"),
            "open_interest_value": row.get::<Option<f64>, _>("open_interest_value"),
            "timestamp": to_utc_ts(row.get::<NaiveDateTime, _>("timestamp")),
        })
    }).collect();

    let page = (offset / limit.max(1) as i64) as i32 + 1;
    let total_pages = ((total as f64) / (limit as f64)).ceil() as i32;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Database cached data",
        "data": items,
        "total": total,
        "page": page,
        "page_size": limit,
        "total_pages": total_pages,
        "source": "database",
        "timestamp": now_cst()
    })))
}

async fn get_open_interests(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let data = sqlx::query(
        r#"
        SELECT
            symbol,
            open_interest::float8,
            open_interest_value::float8,
            timestamp
        FROM open_interests
        ORDER BY timestamp DESC
        LIMIT 100
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = data.iter().map(|row| {
        serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "open_interest": row.get::<f64, _>("open_interest"),
            "open_interest_value": row.get::<Option<f64>, _>("open_interest_value"),
            "timestamp": to_utc_ts(row.get::<NaiveDateTime, _>("timestamp")),
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "interests": items
    })))
}

async fn get_open_interest(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<OpenInterestQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(50).min(500);

    let data = sqlx::query_as::<_, OpenInterestData>(
        r#"
        SELECT
            symbol,
            open_interest::float8,
            open_interest_value::float8,
            timestamp
        FROM open_interests
        WHERE ($1::text IS NULL OR symbol = $1)
        ORDER BY timestamp DESC
        LIMIT $2
        "#,
    )
    .bind(query.symbol)
    .bind(limit as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::to_value(&data).unwrap()))
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct LongShortRatio {
    symbol: String,
    long_ratio: f64,
    short_ratio: f64,
    long_short_ratio: Option<f64>,
    timestamp: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
struct LongShortRatioQuery {
    symbol: Option<String>,
    limit: Option<i64>,
    period: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LongShortRatioSymbolQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    period: Option<String>,
}

async fn get_long_short_ratio_by_symbol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(query): Query<LongShortRatioSymbolQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(100).min(1000) as usize;
    let period = query.period.unwrap_or_else(|| "5m".to_string());

    // Extract currency code from symbol (e.g. "BTC-USDT-SWAP" -> "BTC")
    let ccy = symbol.split('-').next().unwrap_or(&symbol).to_string();

    // Try OKX API first for real-time data
    let okx_data = match get_okx_client(&state, user.user_id).await {
        Ok(client) => {
            match client.get_raw(
                "/api/v5/rubik/stat/contracts/long-short-account-ratio",
                Some(&[("ccy", ccy.clone()), ("period", period.clone())]),
            ).await {
                Ok(data) => {
                    if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                        let items: Vec<serde_json::Value> = arr.iter()
                            .take(limit)
                            .map(|item| {
                                let parts = item.as_array();
                                let ts = parts.and_then(|p| p.get(0)).and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0) / 1000;
                                let long_ratio = parts.and_then(|p| p.get(1)).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                let short_ratio = parts.and_then(|p| p.get(2)).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                                let ls_ratio = parts.and_then(|p| p.get(3)).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok())
                                    .or_else(|| parts.and_then(|p| p.get(3)).and_then(|v| v.as_f64()));
                                serde_json::json!({
                                    "symbol": symbol,
                                    "long_ratio": long_ratio,
                                    "short_ratio": short_ratio,
                                    "long_short_ratio": ls_ratio,
                                    "timestamp": ts,
                                })
                            }).collect();
                        Some(items)
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch long-short ratio from OKX for {}: {}, falling back to DB", symbol, e);
                    None
                }
            }
        }
        Err(_) => None,
    };

    if let Some(items) = okx_data {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": "OKX real-time data",
            "data": items,
            "total": items.len(),
            "source": "okx",
        })));
    }

    // Fallback to database
    let offset = query.offset.unwrap_or(0);

    let total: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM long_short_ratio_history WHERE symbol = $1"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let data = sqlx::query(
        r#"
        SELECT DISTINCT ON (timestamp)
            symbol, long_ratio::float8, short_ratio::float8, long_short_ratio::float8, timestamp
        FROM long_short_ratio_history
        WHERE symbol = $1
        ORDER BY timestamp DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&symbol)
    .bind(limit as i64)
    .bind(offset)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = data.iter().map(|row| {
        serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "long_ratio": row.get::<f64, _>("long_ratio"),
            "short_ratio": row.get::<f64, _>("short_ratio"),
            "long_short_ratio": row.get::<Option<f64>, _>("long_short_ratio"),
            "timestamp": to_utc_ts(row.get::<NaiveDateTime, _>("timestamp")),
        })
    }).collect();

    let page = (offset / limit.max(1) as i64) as i32 + 1;
    let total_pages = ((total as f64) / (limit as f64)).ceil() as i32;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Database cached data",
        "data": items,
        "total": total,
        "page": page,
        "page_size": limit,
        "total_pages": total_pages,
        "source": "database",
        "timestamp": now_cst()
    })))
}

async fn get_long_short_ratio(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LongShortRatioQuery>,
) -> Result<Json<serde_json::Value>> {
    let limit = query.limit.unwrap_or(50).min(500);

    let data = sqlx::query(
        r#"
        SELECT symbol, long_ratio::float8, short_ratio::float8, long_short_ratio::float8, timestamp
        FROM long_short_ratio_history
        WHERE ($1::text IS NULL OR symbol = $1)
        ORDER BY timestamp DESC
        LIMIT $2
        "#,
    )
    .bind(query.symbol)
    .bind(limit as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let ratios_arr: Vec<serde_json::Value> = data.iter().map(|row| {
        serde_json::json!({
            "symbol": row.get::<String, _>("symbol"),
            "ratio": row.get::<Option<f64>, _>("long_short_ratio"),
            "long_ratio": row.get::<f64, _>("long_ratio"),
            "short_ratio": row.get::<f64, _>("short_ratio"),
            "timestamp": to_utc_ts(row.get::<NaiveDateTime, _>("timestamp")),
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "ratios": ratios_arr
    })))
}

#[derive(Debug, Serialize)]
struct MarketSentiment {
    symbol: String,
    fear_greed_index: Option<i32>,
    sentiment_score: Option<f64>,
    long_short_ratio: Option<f64>,
    funding_rate_avg: Option<f64>,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct SentimentQuery {
    symbol: Option<String>,
}

async fn get_market_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SentimentQuery>,
) -> Result<Json<MarketSentiment>> {
    let symbol = query.symbol.unwrap_or_else(|| "BTC-USDT".to_string());

    let ls_ratio = sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT long_short_ratio::float8 FROM long_short_ratio_history WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1"#,
    )
    .bind(&symbol)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .flatten();

    let funding_avg = sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT AVG(funding_rate)::float8 FROM funding_rate_history WHERE symbol = $1 AND created_at > NOW() - INTERVAL '8 hours'"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let sentiment_score = sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT AVG(sentiment_score)::float8 FROM sentiment_data WHERE symbol = $1 AND is_active = true"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(MarketSentiment {
        symbol,
        fear_greed_index: None,
        sentiment_score,
        long_short_ratio: ls_ratio,
        funding_rate_avg: funding_avg,
        timestamp: now_cst(),
    }))
}

async fn get_ticker_by_symbol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let ticker = sqlx::query(
        r#"
        SELECT symbol,
            COALESCE(last, 0)::float8 as last_price,
            COALESCE(high_24h, 0)::float8 as high_24h,
            COALESCE(low_24h, 0)::float8 as low_24h,
            COALESCE(volume_24h, 0)::float8 as volume_24h,
            COALESCE(open_24h, 0)::float8 as open_24h,
            COALESCE(best_bid, 0)::float8 as best_bid,
            COALESCE(best_ask, 0)::float8 as best_ask,
            timestamp
        FROM ticker_history
        WHERE symbol = $1
        ORDER BY timestamp DESC
        LIMIT 1
        "#,
    )
    .bind(symbol)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Ticker not found".to_string()))?;

    let last: f64 = ticker.get("last_price");
    let open_24h: f64 = ticker.get("open_24h");
    let change_24h = last - open_24h;
    let change_percent_24h = if open_24h != 0.0 {
        (last - open_24h) / open_24h * 100.0
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "symbol": ticker.get::<String, _>("symbol"),
        "last": last,
        "best_bid": ticker.get::<f64, _>("best_bid"),
        "best_ask": ticker.get::<f64, _>("best_ask"),
        "open_24h": open_24h,
        "high_24h": ticker.get::<f64, _>("high_24h"),
        "low_24h": ticker.get::<f64, _>("low_24h"),
        "volume_24h": ticker.get::<f64, _>("volume_24h"),
        "change_24h": change_24h,
        "change_percent_24h": change_percent_24h,
        "timestamp": ticker.get::<Option<NaiveDateTime>, _>("timestamp").map(|t| to_utc_ts(t)).unwrap_or_default(),
    })))
}

#[derive(Debug, Deserialize)]
struct CandlesQuery {
    interval: Option<String>,
    limit: Option<i64>,
}

async fn get_candles_by_symbol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(query): Query<CandlesQuery>,
) -> Result<Json<Vec<KlineData>>> {
    let interval = query.interval.unwrap_or_else(|| "1H".to_string());
    let limit = query.limit.unwrap_or(100).min(1000);

    let rows = sqlx::query(
        r#"
        SELECT id, symbol, "interval", open_time, open::float8, high::float8, low::float8, close::float8, volume::float8, created_at
        FROM klines
        WHERE symbol = $1 AND "interval" = $2
        ORDER BY open_time DESC
        LIMIT $3
        "#,
    )
    .bind(symbol)
    .bind(interval)
    .bind(limit as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let klines: Vec<KlineData> = rows.iter().map(|row| KlineData {
        id: row.get("id"),
        symbol: row.get("symbol"),
        interval: row.get("interval"),
        open_time: row.get("open_time"),
        open: row.get("open"),
        high: row.get("high"),
        low: row.get("low"),
        close: row.get("close"),
        volume: row.get("volume"),
        created_at: row.get("created_at"),
    }).collect();

    Ok(Json(klines))
}

async fn get_popular_symbols(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let symbols = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT symbol FROM (
            SELECT DISTINCT symbol FROM klines
            UNION
            SELECT DISTINCT symbol FROM ticker_history
        ) sub
        ORDER BY symbol
        LIMIT 30
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "symbols": symbols
    })))
}

async fn get_sync_status(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Sync status retrieved",
        "data": []
    })))
}
