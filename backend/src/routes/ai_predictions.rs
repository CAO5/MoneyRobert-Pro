use axum::{
    extract::{State, Query, Path},
    routing::{get, post},
    Router,
    Json,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_prediction))
        .route("/", get(list_predictions))
        .route("/{prediction_id}", get(get_prediction))
        .route("/{prediction_id}/cancel", post(cancel_prediction))
        .route("/statistics", get(get_statistics))
        .route("/statistics/summary", get(get_statistics_summary))
}

#[derive(Debug, Deserialize)]
struct CreatePredictionRequest {
    symbol: String,
    direction: String,
    entry_price: f64,
    current_price: f64,
    stop_loss: f64,
    take_profit: serde_json::Value,
    leverage: i32,
    position_size_percent: f64,
    risk_level: String,
    confidence_score: f64,
    holding_period: Option<String>,
    reasoning: Option<String>,
    ai_provider: String,
    model_name: String,
}

async fn create_prediction(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreatePredictionRequest>,
) -> Result<Json<serde_json::Value>> {
    let prediction = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO ai_prediction_trades (user_id, symbol, direction, entry_price, current_price, stop_loss, take_profit, leverage, position_size_percent, risk_level, confidence_score, holding_period, reasoning, status, result, ai_provider, model_name, expires_at)
        VALUES ($1, $2, $3::strategy_direction_enum, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'pending'::ai_prediction_status_enum, 'pending'::ai_prediction_result_enum, $14, $15, NOW() + INTERVAL '24 hours') RETURNING id, symbol, direction::text as direction, confidence_score::float8, status::text as status, created_at)
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id as i32)
    .bind(req.symbol)
    .bind(req.direction)
    .bind(req.entry_price)
    .bind(req.current_price)
    .bind(req.stop_loss)
    .bind(req.take_profit)
    .bind(req.leverage)
    .bind(req.position_size_percent)
    .bind(req.risk_level)
    .bind(req.confidence_score)
    .bind(req.holding_period)
    .bind(req.reasoning)
    .bind(req.ai_provider)
    .bind(req.model_name)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": prediction})))
}

#[derive(Debug, Deserialize)]
struct PredictionQuery {
    symbol: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_predictions(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PredictionQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let predictions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, direction::text as direction, confidence_score::float8, risk_level, status::text as status, created_at FROM ai_prediction_trades
            WHERE user_id = $1 AND ($2::text IS NULL OR symbol = $2) AND ($3::text IS NULL OR status::text = $3)
            ORDER BY created_at DESC LIMIT $4 OFFSET $5
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .bind(query.symbol)
    .bind(query.status)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": predictions, "page": page, "page_size": page_size})))
}

async fn get_prediction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(prediction_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let prediction = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, direction::text as direction, entry_price::float8, current_price::float8, stop_loss::float8, take_profit, leverage, position_size_percent::float8, risk_level, confidence_score::float8, status::text as status, result::text as result, pnl_percent::float8, created_at FROM ai_prediction_trades WHERE id = $1 AND user_id = $2
        ) AS sq"#,
    )
    .bind(prediction_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Prediction not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": prediction})))
}

async fn cancel_prediction(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(prediction_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE ai_prediction_trades SET status = 'cancelled'::ai_prediction_status_enum WHERE id = $1 AND user_id = $2 AND status = 'pending'::ai_prediction_status_enum RETURNING id"#,
    )
    .bind(prediction_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Prediction not found or cannot be cancelled".to_string()))?;

    Ok(Json(serde_json::json!({"message": "Prediction cancelled"})))
}

async fn get_statistics(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM ai_prediction_trades WHERE user_id = $1"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let correct = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM ai_prediction_trades WHERE user_id = $1 AND result = 'win'::ai_prediction_result_enum"#,
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
        "total_predictions": total,
        "correct_predictions": correct,
        "win_rate": win_rate,
    })))
}

async fn get_statistics_summary(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let d7 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM ai_prediction_trades WHERE user_id = $1 AND result = 'win'::ai_prediction_result_enum AND created_at > NOW() - INTERVAL '7 days'"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let d30 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM ai_prediction_trades WHERE user_id = $1 AND result = 'win'::ai_prediction_result_enum AND created_at > NOW() - INTERVAL '30 days'"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "7d": {"correct": d7},
        "30d": {"correct": d30},
    })))
}
