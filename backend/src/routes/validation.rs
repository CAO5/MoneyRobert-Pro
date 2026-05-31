use axum::{
    extract::{State, Query, Path},
    routing::{get, post},
    Router,
    Json,
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/validate", post(validate_prediction))
        .route("/results/{validation_id}", get(get_validation_result))
        .route("/results", get(list_validation_results))
        .route("/statistics", get(get_statistics))
        .route("/confidence-analysis", get(get_confidence_analysis))
        .route("/pattern-analysis", get(get_pattern_analysis))
        .route("/recent-performance", get(get_recent_performance))
        .route("/confidence-threshold", get(get_confidence_threshold))
        .route("/direction-analysis", get(get_direction_analysis))
}

#[derive(Debug, Deserialize)]
struct ValidateRequest {
    prediction_id: i32,
    actual_price: f64,
}

async fn validate_prediction(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<serde_json::Value>> {
    let prediction = sqlx::query(
        r#"SELECT id, symbol, direction::text as direction, confidence_score::float8, entry_price::float8 FROM ai_prediction_trades WHERE id = $1 AND user_id = $2"#,
    )
    .bind(req.prediction_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Prediction not found".to_string()))?;

    let direction: String = prediction.get("direction");
    let entry_price: f64 = prediction.get("entry_price");
    let symbol: String = prediction.get("symbol");
    let confidence_score: Option<f64> = prediction.get("confidence_score");

    let is_correct = match direction.as_str() {
        "LONG" => req.actual_price > entry_price,
        "SHORT" => req.actual_price < entry_price,
        _ => false,
    };

    let outcome = if is_correct { "WIN" } else { "LOSS" };
    let status_value = if is_correct { "TAKE_PROFIT_HIT" } else { "STOP_LOSS_HIT" };
    let validation_id = Uuid::new_v4().to_string();

    let result = sqlx::query(
        r#"INSERT INTO validation_records (validation_id, validation_type, status, symbol, entry_price, direction, exit_price, outcome, predicted_confidence, source_prediction_id, validation_time) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW()) RETURNING id"#,
    )
    .bind(&validation_id)
    .bind("price_validation")
    .bind("completed")
    .bind(&symbol)
    .bind(entry_price)
    .bind(&direction)
    .bind(req.actual_price)
    .bind(outcome)
    .bind(confidence_score)
    .bind(req.prediction_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    sqlx::query(
        r#"UPDATE ai_prediction_trades SET result = $1::ai_prediction_result_enum, status = $2::ai_prediction_status_enum, updated_at = NOW() WHERE id = $3"#,
    )
    .bind(outcome)
    .bind(status_value)
    .bind(req.prediction_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "validation_id": validation_id,
        "is_correct": is_correct,
        "actual_price": req.actual_price,
    })))
}

async fn get_validation_result(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(validation_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, validation_id, validation_type, status, symbol, entry_price::float8, direction, exit_price::float8, outcome, predicted_confidence::float8, actual_confidence::float8, source_prediction_id, validation_time, completion_time FROM validation_records WHERE validation_id = $1
        ) AS sq"#,
    )
    .bind(validation_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Validation result not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": result})))
}

#[derive(Debug, Deserialize)]
struct ValidationQuery {
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_validation_results(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ValidationQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let results = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT vr.id, vr.validation_id, vr.validation_type, vr.status, vr.symbol, vr.direction, vr.exit_price::float8, vr.outcome, vr.predicted_confidence::float8, vr.source_prediction_id, vr.validation_time
            FROM validation_records vr
            JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id
            WHERE apt.user_id = $1
            ORDER BY vr.validation_time DESC LIMIT $2 OFFSET $3
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": results, "page": page, "page_size": page_size})))
}

async fn get_statistics(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM validation_records vr JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id WHERE apt.user_id = $1"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let correct = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM validation_records vr JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id WHERE apt.user_id = $1 AND vr.outcome = 'WIN'"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let win_rate = if total > 0 {
        correct as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "total_validations": total,
        "correct": correct,
        "win_rate": win_rate,
    })))
}

async fn get_confidence_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let analysis = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT
                CASE
                    WHEN apt.confidence_score < 0.5 THEN 'low'
                    WHEN apt.confidence_score < 0.7 THEN 'medium'
                    WHEN apt.confidence_score < 0.85 THEN 'high'
                    ELSE 'very_high'
                END as confidence_bucket,
                COUNT(*) as total,
                SUM(CASE WHEN vr.outcome = 'WIN' THEN 1 ELSE 0 END) as correct
            FROM validation_records vr
            JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id
            WHERE apt.user_id = $1
            GROUP BY confidence_bucket
            ORDER BY confidence_bucket
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"analysis": analysis})))
}

async fn get_pattern_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let patterns = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT
                apt.direction::text as direction,
                apt.risk_level,
                COUNT(*) as total,
                SUM(CASE WHEN vr.outcome = 'WIN' THEN 1 ELSE 0 END) as correct,
                AVG(apt.confidence_score)::float8 as avg_confidence
            FROM validation_records vr
            JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id
            WHERE apt.user_id = $1
            GROUP BY apt.direction, apt.risk_level
            ORDER BY apt.direction, apt.risk_level
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"patterns": patterns})))
}

async fn get_recent_performance(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let recent = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM validation_records vr JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id WHERE apt.user_id = $1 AND vr.outcome = 'WIN' AND vr.validation_time > NOW() - INTERVAL '7 days'"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let total_recent = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM validation_records vr JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id WHERE apt.user_id = $1 AND vr.validation_time > NOW() - INTERVAL '7 days'"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let win_rate = if total_recent > 0 {
        recent as f64 / total_recent as f64 * 100.0
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "7d_win_rate": win_rate,
        "7d_total": total_recent,
        "7d_correct": recent,
    })))
}

async fn get_confidence_threshold(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let thresholds = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT
                MIN(apt.confidence_score)::float8 FILTER (WHERE vr.outcome = 'WIN') as min_correct_confidence,
                MAX(apt.confidence_score)::float8 FILTER (WHERE vr.outcome = 'LOSS') as max_incorrect_confidence,
                AVG(apt.confidence_score)::float8 FILTER (WHERE vr.outcome = 'WIN') as avg_correct_confidence
            FROM validation_records vr
            JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id
            WHERE apt.user_id = $1
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"thresholds": thresholds})))
}

async fn get_direction_analysis(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let analysis = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT
                apt.direction::text as direction,
                COUNT(*) as total,
                SUM(CASE WHEN vr.outcome = 'WIN' THEN 1 ELSE 0 END) as correct,
                AVG(CASE WHEN vr.outcome = 'WIN' THEN 1.0 ELSE 0.0 END) * 100 as win_rate
            FROM validation_records vr
            JOIN ai_prediction_trades apt ON vr.source_prediction_id = apt.id
            WHERE apt.user_id = $1
            GROUP BY apt.direction
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"analysis": analysis})))
}
