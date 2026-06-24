//! Data Quality Monitoring REST API
//! 数据质量监控 API
//!
//! 依据《系统评估与演进规划》系统架构师视角：
//!   "错误大量用 .ok() 吞掉，缺少数据质量告警"
//!
//! 提供端点：
//! - GET  /data-quality/overview              获取所有数据源质量概览
//! - GET  /data-quality/{symbol}             获取指定 symbol 的质量详情
//! - GET  /data-quality/alerts                获取异常数据源告警列表
//! - POST /data-quality/scan                  手动触发质量扫描
//! - GET  /data-quality/sources               获取所有数据源定义

use crate::data_quality;
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(get_overview))
        .route("/{symbol}", get(get_symbol_quality))
        .route("/alerts/list", get(get_alerts))
        .route("/scan", post(trigger_scan))
        .route("/sources", get(list_sources))
}

/// 获取所有数据源质量概览
async fn get_overview(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let overview = data_quality::get_quality_overview(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    let total = overview.len();
    let critical = overview
        .iter()
        .filter(|v| v.get("quality_grade").and_then(|g| g.as_str()) == Some("D"))
        .count();
    let warning = overview
        .iter()
        .filter(|v| v.get("quality_grade").and_then(|g| g.as_str()) == Some("C"))
        .count();
    let healthy = overview
        .iter()
        .filter(|v| {
            matches!(
                v.get("quality_grade").and_then(|g| g.as_str()),
                Some("A") | Some("B")
            )
        })
        .count();

    Ok(Json(serde_json::json!({
        "total_sources": total,
        "healthy": healthy,
        "warning": warning,
        "critical": critical,
        "overview": overview,
    })))
}

/// 获取指定 symbol 的质量详情
#[derive(Debug, Deserialize)]
struct SymbolQualityQuery {
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

async fn get_symbol_quality(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(q): Query<SymbolQualityQuery>,
) -> Result<Json<serde_json::Value>> {
    let end = q.end_time.unwrap_or_else(Utc::now);
    let start = q.start_time.unwrap_or(end - chrono::Duration::hours(24));
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);

    let snapshots = crate::features::store::FeatureStore::query_data_quality(
        &state.db_pool,
        &symbol,
        start,
        end,
    )
    .await
    .map_err(|e| AppError::Database(e))?;

    let limited: Vec<_> = snapshots.into_iter().take(limit as usize).collect();

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "start_time": start.to_rfc3339(),
        "end_time": end.to_rfc3339(),
        "count": limited.len(),
        "snapshots": limited,
    })))
}

/// 获取异常数据源告警列表
async fn get_alerts(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let alerts = data_quality::get_critical_data_sources(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    let critical_count = alerts
        .iter()
        .filter(|a| a.get("severity").and_then(|s| s.as_str()) == Some("critical"))
        .count();
    let warning_count = alerts
        .iter()
        .filter(|a| a.get("severity").and_then(|s| s.as_str()) == Some("warning"))
        .count();

    Ok(Json(serde_json::json!({
        "total_alerts": alerts.len(),
        "critical": critical_count,
        "warning": warning_count,
        "alerts": alerts,
    })))
}

/// 手动触发质量扫描
#[derive(Debug, Deserialize)]
struct ScanRequest {
    /// 回溯小时数（默认 1 小时）
    lookback_hours: Option<i64>,
    /// 指定 symbol 列表（默认全部）
    symbols: Option<Vec<String>>,
}

async fn trigger_scan(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<serde_json::Value>> {
    let lookback = req.lookback_hours.unwrap_or(1).clamp(1, 168);

    let symbols: Vec<String> = req.symbols.unwrap_or_else(|| {
        vec![
            "BTC-USDT-SWAP".into(),
            "ETH-USDT-SWAP".into(),
            "SOL-USDT-SWAP".into(),
            "DOGE-USDT-SWAP".into(),
            "XRP-USDT-SWAP".into(),
            "ADA-USDT-SWAP".into(),
            "AVAX-USDT-SWAP".into(),
            "DOT-USDT-SWAP".into(),
            "LINK-USDT-SWAP".into(),
            "MATIC-USDT-SWAP".into(),
            "UNI-USDT-SWAP".into(),
            "ATOM-USDT-SWAP".into(),
            "LTC-USDT-SWAP".into(),
            "FIL-USDT-SWAP".into(),
            "APT-USDT-SWAP".into(),
            "ARB-USDT-SWAP".into(),
            "OP-USDT-SWAP".into(),
            "NEAR-USDT-SWAP".into(),
            "SUI-USDT-SWAP".into(),
            "PEPE-USDT-SWAP".into(),
        ]
    });

    let reports = data_quality::run_quality_scan(&state.db_pool, &symbols, lookback)
        .await
        .map_err(|e| AppError::Database(e))?;

    let total = reports.len();
    let grade_a = reports.iter().filter(|r| r.quality_grade == "A").count();
    let grade_b = reports.iter().filter(|r| r.quality_grade == "B").count();
    let grade_c = reports.iter().filter(|r| r.quality_grade == "C").count();
    let grade_d = reports.iter().filter(|r| r.quality_grade == "D").count();

    Ok(Json(serde_json::json!({
        "scanned": total,
        "grade_distribution": {
            "A": grade_a,
            "B": grade_b,
            "C": grade_c,
            "D": grade_d,
        },
        "reports": reports,
    })))
}

/// 获取所有数据源定义
async fn list_sources(
    _user: CurrentUser,
) -> Result<Json<serde_json::Value>> {
    let sources = data_quality::default_data_sources();

    let sources_json: Vec<serde_json::Value> = sources
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "table": s.table,
                "time_column": s.time_column,
                "expected_interval_sec": s.expected_interval_sec,
                "value_column": s.value_column,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "count": sources_json.len(),
        "sources": sources_json,
    })))
}
