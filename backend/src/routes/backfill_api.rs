use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Router, Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::backfill::{BackfillRequest, HistoryBackfiller};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/backfill", post(backfill_handler))
        .route("/backfill/batch", post(backfill_batch_handler))
        .route("/gaps", get(detect_gaps_handler))
        .route("/coverage/{symbol}", get(get_coverage_handler))
        .route("/jobs", get(list_backfill_jobs_handler))
        .route("/jobs/{job_id}", get(get_backfill_job_handler))
}

/// 单个 symbol + interval 回填
async fn backfill_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<BackfillRequest>,
) -> Result<Json<serde_json::Value>> {
    let backfiller = HistoryBackfiller::new(state.db_pool.clone());

    // 创建回填任务记录
    let job_id = uuid::Uuid::new_v4();
    let from_ts = parse_to_timestamp(&req.from)
        .map_err(|e| AppError::Validation(format!("Invalid from: {}", e)))?;
    let to_ts = parse_to_timestamp(&req.to)
        .map_err(|e| AppError::Validation(format!("Invalid to: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO backfill_jobs (job_id, symbol, bar, from_time, to_time, status, started_at)
           VALUES ($1, $2, $3, $4, $5, 'running', NOW())"#,
    )
    .bind(job_id)
    .bind(&req.symbol)
    .bind(&req.bar)
    .bind(from_ts)
    .bind(to_ts)
    .execute(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    // 执行回填
    let result = backfiller
        .backfill(&req)
        .await
        .map_err(|e| AppError::Internal(e))?;

    // 更新任务记录
    let status = if result.errors.is_empty() { "completed" } else { "partial" };
    sqlx::query(
        r#"UPDATE backfill_jobs
           SET status = $2, fetched_count = $3, inserted_count = $4, updated_count = $5,
               skipped_count = $6, gaps_detected = $7, gaps_filled = $8,
               elapsed_secs = $9, errors = $10, completed_at = NOW()
           WHERE job_id = $1"#,
    )
    .bind(job_id)
    .bind(status)
    .bind(result.fetched as i32)
    .bind(result.inserted as i32)
    .bind(result.updated as i32)
    .bind(result.skipped as i32)
    .bind(result.gaps_detected as i32)
    .bind(result.gaps_filled as i32)
    .bind(result.elapsed_secs)
    .bind(serde_json::to_value(&result.errors).unwrap_or(serde_json::Value::Array(vec![])))
    .execute(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "data": result,
    })))
}

#[derive(Debug, Deserialize)]
struct BatchBackfillRequest {
    symbols: Vec<String>,
    bars: Vec<String>,
    from: String,
    to: String,
}

/// 批量回填多个 symbol + interval
async fn backfill_batch_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<BatchBackfillRequest>,
) -> Result<Json<serde_json::Value>> {
    let backfiller = HistoryBackfiller::new(state.db_pool.clone());

    let symbols: Vec<&str> = req.symbols.iter().map(|s| s.as_str()).collect();
    let bars: Vec<&str> = req.bars.iter().map(|s| s.as_str()).collect();

    let results = backfiller
        .backfill_batch(&symbols, &bars, &req.from, &req.to)
        .await;

    Ok(Json(serde_json::json!({
        "data": results,
        "total": results.len(),
    })))
}

#[derive(Debug, Deserialize)]
struct GapDetectionRequest {
    symbol: String,
    bar: String,
    from: String,
    to: String,
}

/// 检测数据缺口
async fn detect_gaps_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(req): Query<GapDetectionRequest>,
) -> Result<Json<serde_json::Value>> {
    let backfiller = HistoryBackfiller::new(state.db_pool.clone());

    let from_ms = crate::backfill::HistoryBackfiller::parse_timestamp(&req.from)
        .map_err(|e| AppError::Validation(format!("Invalid from: {}", e)))?;
    let to_ms = crate::backfill::HistoryBackfiller::parse_timestamp(&req.to)
        .map_err(|e| AppError::Validation(format!("Invalid to: {}", e)))?;

    let gaps = backfiller
        .detect_gaps(&req.symbol, &req.bar, from_ms, to_ms)
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(serde_json::json!({
        "data": gaps,
        "total_gaps": gaps.len(),
        "total_missing": gaps.iter().map(|g| g.missing_count).sum::<usize>(),
    })))
}

/// 获取数据覆盖范围
async fn get_coverage_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(params): Query<CoverageQuery>,
) -> Result<Json<serde_json::Value>> {
    let backfiller = HistoryBackfiller::new(state.db_pool.clone());
    let coverage = backfiller
        .get_data_coverage(&symbol, &params.bar)
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(serde_json::json!({"data": coverage})))
}

#[derive(Debug, Deserialize)]
struct CoverageQuery {
    bar: String,
}

/// 列出回填任务
async fn list_backfill_jobs_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query(
        r#"SELECT job_id, symbol, bar, from_time, to_time, status,
                  fetched_count, inserted_count, updated_count, skipped_count,
                  gaps_detected, gaps_filled, elapsed_secs,
                  started_at, completed_at, created_at
           FROM backfill_jobs
           ORDER BY created_at DESC LIMIT 50"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    let jobs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "job_id": r.get::<uuid::Uuid, _>("job_id"),
                "symbol": r.get::<String, _>("symbol"),
                "bar": r.get::<String, _>("bar"),
                "from_time": r.get::<chrono::DateTime<chrono::Utc>, _>("from_time"),
                "to_time": r.get::<chrono::DateTime<chrono::Utc>, _>("to_time"),
                "status": r.get::<String, _>("status"),
                "fetched_count": r.get::<i32, _>("fetched_count"),
                "inserted_count": r.get::<i32, _>("inserted_count"),
                "updated_count": r.get::<i32, _>("updated_count"),
                "skipped_count": r.get::<i32, _>("skipped_count"),
                "gaps_detected": r.get::<i32, _>("gaps_detected"),
                "gaps_filled": r.get::<i32, _>("gaps_filled"),
                "elapsed_secs": r.get::<Option<f64>, _>("elapsed_secs"),
                "started_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
                "completed_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({"data": jobs, "total": jobs.len()})))
}

/// 获取单个回填任务详情
async fn get_backfill_job_handler(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>> {
    let row = sqlx::query(
        r#"SELECT job_id, symbol, bar, from_time, to_time, status,
                  fetched_count, inserted_count, updated_count, skipped_count,
                  gaps_detected, gaps_filled, elapsed_secs, error_message, errors,
                  started_at, completed_at, created_at
           FROM backfill_jobs WHERE job_id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound(format!("Backfill job {} not found", job_id)))?;

    Ok(Json(serde_json::json!({
        "data": {
            "job_id": row.get::<uuid::Uuid, _>("job_id"),
            "symbol": row.get::<String, _>("symbol"),
            "bar": row.get::<String, _>("bar"),
            "from_time": row.get::<chrono::DateTime<chrono::Utc>, _>("from_time"),
            "to_time": row.get::<chrono::DateTime<chrono::Utc>, _>("to_time"),
            "status": row.get::<String, _>("status"),
            "fetched_count": row.get::<i32, _>("fetched_count"),
            "inserted_count": row.get::<i32, _>("inserted_count"),
            "updated_count": row.get::<i32, _>("updated_count"),
            "skipped_count": row.get::<i32, _>("skipped_count"),
            "gaps_detected": row.get::<i32, _>("gaps_detected"),
            "gaps_filled": row.get::<i32, _>("gaps_filled"),
            "elapsed_secs": row.get::<Option<f64>, _>("elapsed_secs"),
            "error_message": row.get::<Option<String>, _>("error_message"),
            "errors": row.get::<serde_json::Value, _>("errors"),
            "started_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
            "completed_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        }
    })))
}

/// 解析时间字符串为 chrono::DateTime<Utc>
fn parse_to_timestamp(s: &str) -> std::result::Result<chrono::DateTime<chrono::Utc>, String> {
    // 尝试 ISO 8601
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    // 尝试毫秒时间戳
    if let Ok(ms) = s.parse::<i64>() {
        if let Some(ts) = chrono::DateTime::from_timestamp(ms / 1000, 0) {
            return Ok(ts);
        }
    }
    // 尝试 "YYYY-MM-DD"
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = date.and_hms_opt(0, 0, 0).unwrap();
        return Ok(dt.and_utc());
    }
    Err(format!("Cannot parse timestamp: {}", s))
}
