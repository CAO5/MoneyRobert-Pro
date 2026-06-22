//! Feature Store & Market Regime REST API
//! 特征存储与市场状态 API
//!
//! 提供端点：
//! - GET /features/definitions                查询所有特征定义
//! - GET /features/values                     查询特征值时间序列
//! - GET /regimes/history                     查询市场状态历史
//! - GET /regimes/latest/{symbol}             查询某标的最新市场状态
//! - POST /regimes/aggregate-daily            触发每日 OHLCV 聚合

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::features::{FeatureStore, RegimeClassifier};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/definitions", get(list_definitions))
        .route("/values", get(query_values))
        .route("/regimes/history", get(query_regimes))
        .route("/regimes/latest/{symbol}", get(latest_regime))
        .route("/regimes/aggregate-daily", post(aggregate_daily))
}

#[derive(Debug, Serialize)]
struct FeatureDefinitionResponse {
    feature_id: String,
    name: String,
    description: Option<String>,
    category: String,
    version: String,
    parameters: serde_json::Value,
    unit: Option<String>,
}

async fn list_definitions(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<FeatureDefinitionResponse>>> {
    let defs = FeatureStore::list_definitions(&state.db_pool)
        .await
        .map_err(|e| AppError::Internal(format!("list feature definitions failed: {}", e)))?;

    let resp = defs
        .into_iter()
        .map(|d| FeatureDefinitionResponse {
            feature_id: d.feature_id.to_string(),
            name: d.name,
            description: d.description,
            category: d.category,
            version: d.version,
            parameters: d.parameters,
            unit: d.unit,
        })
        .collect();
    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
struct QueryValuesParams {
    feature: String,
    symbol: String,
    start_time: String,
    end_time: String,
}

#[derive(Debug, Serialize)]
struct FeatureValueResponse {
    feature_name: String,
    symbol: String,
    timestamp: String,
    value: f64,
    metadata: Option<serde_json::Value>,
}

async fn query_values(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<QueryValuesParams>,
) -> Result<Json<Vec<FeatureValueResponse>>> {
    let start: DateTime<Utc> = params
        .start_time
        .parse()
        .map_err(|e| AppError::Validation(format!("invalid start_time: {}", e)))?;
    let end: DateTime<Utc> = params
        .end_time
        .parse()
        .map_err(|e| AppError::Validation(format!("invalid end_time: {}", e)))?;

    let values = FeatureStore::query_feature_values(
        &state.db_pool,
        &params.feature,
        &params.symbol,
        start,
        end,
    )
    .await
    .map_err(|e| AppError::Internal(format!("query feature values failed: {}", e)))?;

    let resp = values
        .into_iter()
        .map(|v| FeatureValueResponse {
            feature_name: v.feature_name,
            symbol: v.symbol,
            timestamp: v.timestamp.to_rfc3339(),
            value: v.value,
            metadata: v.metadata,
        })
        .collect();
    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
struct QueryRegimesParams {
    symbol: String,
    start_time: String,
    end_time: String,
}

#[derive(Debug, Serialize)]
struct RegimeResponse {
    regime: String,
    confidence: f64,
    adx: f64,
    volatility_percentile: f64,
    return_percentile: f64,
    timestamp: String,
}

async fn query_regimes(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<QueryRegimesParams>,
) -> Result<Json<Vec<RegimeResponse>>> {
    let start: DateTime<Utc> = params
        .start_time
        .parse()
        .map_err(|e| AppError::Validation(format!("invalid start_time: {}", e)))?;
    let end: DateTime<Utc> = params
        .end_time
        .parse()
        .map_err(|e| AppError::Validation(format!("invalid end_time: {}", e)))?;

    let snapshots = FeatureStore::query_regimes(&state.db_pool, &params.symbol, start, end)
        .await
        .map_err(|e| AppError::Internal(format!("query regimes failed: {}", e)))?;

    let resp = snapshots
        .into_iter()
        .map(|s| RegimeResponse {
            regime: s.regime.as_str().to_string(),
            confidence: s.confidence,
            adx: s.adx,
            volatility_percentile: s.volatility_percentile,
            return_percentile: s.return_percentile,
            timestamp: s.timestamp.to_rfc3339(),
        })
        .collect();
    Ok(Json(resp))
}

#[derive(Debug, Serialize)]
struct LatestRegimeResponse {
    symbol: String,
    regime: String,
    confidence: f64,
    adx: f64,
    volatility_percentile: f64,
    return_percentile: f64,
    timestamp: Option<String>,
}

/// 查询某标的最新市场状态：优先从 market_regimes 表读取最新一条；
/// 若无记录，则从 klines 表读取最近 200 根 1H K 线实时计算。
async fn latest_regime(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<LatestRegimeResponse>> {
    // 1) 尝试从数据库读取最新一条记录
    let row = sqlx::query(
        r#"SELECT regime, confidence, adx, volatility_percentile, return_percentile, timestamp
           FROM market_regimes
           WHERE symbol = $1
           ORDER BY timestamp DESC
           LIMIT 1"#,
    )
    .bind(&symbol)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Internal(format!("query latest regime failed: {}", e)))?;

    if let Some(row) = row {
        let regime_str: String = row.get("regime");
        let ts: Option<chrono::DateTime<Utc>> = row.get("timestamp");
        return Ok(Json(LatestRegimeResponse {
            symbol,
            regime: regime_str,
            confidence: row.get("confidence"),
            adx: row.get("adx"),
            volatility_percentile: row.get("volatility_percentile"),
            return_percentile: row.get("return_percentile"),
            timestamp: ts.map(|t| t.to_rfc3339()),
        }));
    }

    // 2) 数据库无记录，从 klines 实时计算
    let kline_rows = sqlx::query(
        r#"SELECT open, high, low, close, volume
           FROM klines
           WHERE symbol = $1 AND interval = '1H'
           ORDER BY open_time DESC
           LIMIT 200"#,
    )
    .bind(&symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Internal(format!("query klines for regime failed: {}", e)))?;

    if kline_rows.is_empty() {
        return Ok(Json(LatestRegimeResponse {
            symbol,
            regime: "unknown".into(),
            confidence: 0.0,
            adx: 0.0,
            volatility_percentile: 0.0,
            return_percentile: 0.0,
            timestamp: None,
        }));
    }

    // 按时间升序排列
    let mut klines: Vec<(f64, f64, f64, f64, f64)> = kline_rows
        .into_iter()
        .map(|r| {
            (
                r.get::<f64, _>("open"),
                r.get::<f64, _>("high"),
                r.get::<f64, _>("low"),
                r.get::<f64, _>("close"),
                r.get::<f64, _>("volume"),
            )
        })
        .collect();
    klines.reverse();

    let classifier = RegimeClassifier::with_defaults();
    let snapshot = classifier.classify(&klines);
    match snapshot {
        Some(s) => Ok(Json(LatestRegimeResponse {
            symbol,
            regime: s.regime.as_str().to_string(),
            confidence: s.confidence,
            adx: s.adx,
            volatility_percentile: s.volatility_percentile,
            return_percentile: s.return_percentile,
            timestamp: Some(s.timestamp.to_rfc3339()),
        })),
        None => Ok(Json(LatestRegimeResponse {
            symbol,
            regime: "insufficient_data".into(),
            confidence: 0.0,
            adx: 0.0,
            volatility_percentile: 0.0,
            return_percentile: 0.0,
            timestamp: None,
        })),
    }
}

#[derive(Debug, Deserialize)]
struct AggregateDailyParams {
    symbol: String,
}

#[derive(Debug, Serialize)]
struct AggregateDailyResponse {
    symbol: String,
    rows_affected: u64,
}

async fn aggregate_daily(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(params): Json<AggregateDailyParams>,
) -> Result<Json<AggregateDailyResponse>> {
    let n = FeatureStore::aggregate_daily_ohlcv(&state.db_pool, &params.symbol)
        .await
        .map_err(|e| AppError::Internal(format!("aggregate daily ohlcv failed: {}", e)))?;
    Ok(Json(AggregateDailyResponse {
        symbol: params.symbol,
        rows_affected: n,
    }))
}
