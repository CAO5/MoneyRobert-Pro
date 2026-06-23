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
        .route("/quality", get(query_quality))
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

// =========================================================
// 数据质量查询（data_quality_snapshots）
// =========================================================

/// 数据质量查询参数
/// - symbol: 必填，标的符号
/// - start_time / end_time: 可选，时间范围；若均未提供则返回最新一条快照
#[derive(Debug, Deserialize)]
struct QueryQualityParams {
    symbol: String,
    start_time: Option<String>,
    end_time: Option<String>,
}

/// 数据质量快照响应
#[derive(Debug, Serialize)]
struct DataQualityResponse {
    symbol: String,
    data_source: String,
    snapshot_time: String,
    period_start: String,
    period_end: String,
    /// 数据新鲜度（秒），距上次更新经过的时间
    freshness_sec: Option<f64>,
    gap_count: i32,
    /// 缺口率（0~1）
    gap_ratio: f64,
    outlier_count: i32,
    /// 异常值率（0~1）
    outlier_ratio: f64,
    expected_points: i32,
    actual_points: i32,
    /// 覆盖率（0~1）
    coverage_ratio: f64,
    backfill_status: String,
    last_backfill_time: Option<String>,
    /// 质量等级：excellent / good / fair / poor / unknown
    quality_grade: String,
    metadata: Option<serde_json::Value>,
}

impl From<crate::features::DataQualitySnapshot> for DataQualityResponse {
    fn from(s: crate::features::DataQualitySnapshot) -> Self {
        Self {
            symbol: s.symbol,
            data_source: s.data_source,
            snapshot_time: s.snapshot_time.to_rfc3339(),
            period_start: s.period_start.to_rfc3339(),
            period_end: s.period_end.to_rfc3339(),
            freshness_sec: s.freshness_sec,
            gap_count: s.gap_count,
            gap_ratio: s.gap_ratio,
            outlier_count: s.outlier_count,
            outlier_ratio: s.outlier_ratio,
            expected_points: s.expected_points,
            actual_points: s.actual_points,
            coverage_ratio: s.coverage_ratio,
            backfill_status: s.backfill_status,
            last_backfill_time: s.last_backfill_time.map(|t| t.to_rfc3339()),
            quality_grade: s.quality_grade,
            metadata: s.metadata,
        }
    }
}

/// 查询数据质量快照
///
/// 行为：
/// - 若提供 start_time 与 end_time，返回该时间范围内的所有快照（按时间倒序）
/// - 若未提供时间范围，返回该 symbol 的最新一条快照
///
/// 用途：在决策卡与回测可信等级评估中作为 data_freshness 门禁的数据来源。
async fn query_quality(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<QueryQualityParams>,
) -> Result<Json<serde_json::Value>> {
    // 分支：未提供时间范围，返回最新一条
    if params.start_time.is_none() && params.end_time.is_none() {
        let snapshot = FeatureStore::get_latest_data_quality(&state.db_pool, &params.symbol)
            .await
            .map_err(|e| AppError::Internal(format!("query latest data quality failed: {}", e)))?;

        return Ok(Json(serde_json::json!({
            "symbol": params.symbol,
            "latest": snapshot.map(DataQualityResponse::from),
        })));
    }

    // 分支：提供时间范围，返回历史快照列表
    let now = Utc::now();
    let start: DateTime<Utc> = match params.start_time.as_deref() {
        Some(s) => s
            .parse()
            .map_err(|e| AppError::Validation(format!("invalid start_time: {}", e)))?,
        None => now - chrono::Duration::days(30),
    };
    let end: DateTime<Utc> = match params.end_time.as_deref() {
        Some(s) => s
            .parse()
            .map_err(|e| AppError::Validation(format!("invalid end_time: {}", e)))?,
        None => now,
    };

    if end < start {
        return Err(AppError::Validation("end_time must be >= start_time".into()));
    }

    let snapshots = FeatureStore::query_data_quality(&state.db_pool, &params.symbol, start, end)
        .await
        .map_err(|e| AppError::Internal(format!("query data quality failed: {}", e)))?;

    let resp: Vec<DataQualityResponse> = snapshots.into_iter().map(DataQualityResponse::from).collect();
    Ok(Json(serde_json::json!({
        "symbol": params.symbol,
        "start_time": start.to_rfc3339(),
        "end_time": end.to_rfc3339(),
        "snapshots": resp,
    })))
}
