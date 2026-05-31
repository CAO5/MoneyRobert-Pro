use axum::{
    extract::{State, Query, Path},
    routing::{get, post, put, delete},
    Router,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::schemas::{success_response, MessageResponse};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_strategy))
        .route("/", get(list_strategies))
        .route("/symbols", get(get_strategy_symbols))
        .route("/{strategy_id}", get(get_strategy))
        .route("/{strategy_id}", put(update_strategy))
        .route("/{strategy_id}", delete(delete_strategy))
        .route("/{strategy_id}/execute", post(execute_strategy))
        .route("/{strategy_id}/cancel", post(cancel_strategy))
        .route("/{strategy_id}/pause", post(pause_strategy))
        .route("/{strategy_id}/resume", post(resume_strategy))
        .route("/{strategy_id}/risk-metrics", get(get_risk_metrics))
}

#[derive(Debug, Deserialize)]
struct CreateStrategyRequest {
    symbol: String,
    direction: String,
    entry_price: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    leverage: i32,
    position_size: f64,
}

async fn create_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateStrategyRequest>,
) -> Result<Json<serde_json::Value>> {
    let strategy = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO strategies (user_id, symbol, direction, entry_price, stop_loss, take_profit, leverage, position_size, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'created') RETURNING id, symbol, LOWER(direction::text), entry_price::float8, stop_loss::float8, take_profit::float8, leverage, position_size::float8, LOWER(status::text), created_at)
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id)
    .bind(req.symbol)
    .bind(req.direction)
    .bind(req.entry_price)
    .bind(req.stop_loss)
    .bind(req.take_profit)
    .bind(req.leverage)
    .bind(req.position_size)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(success_response("Strategy created successfully", strategy)))
}

#[derive(Debug, Deserialize)]
struct StrategyQuery {
    status: Option<String>,
    symbol: Option<String>,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_strategies(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<StrategyQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM strategies WHERE user_id = $1 AND ($2::text IS NULL OR LOWER(status::text) = LOWER($2)) AND ($3::text IS NULL OR symbol = $3) AND ($4::text IS NULL OR symbol ILIKE '%' || $4 || '%')"#,
    )
    .bind(user.user_id)
    .bind(query.status.clone())
    .bind(query.symbol.clone())
    .bind(query.search.clone())
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let strategies = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, LOWER(direction::text), entry_price::float8, stop_loss::float8, take_profit::float8, leverage, position_size::float8, LOWER(status::text), created_at, updated_at
            FROM strategies WHERE user_id = $1 AND ($2::text IS NULL OR LOWER(status::text) = LOWER($2)) AND ($3::text IS NULL OR symbol = $3) AND ($4::text IS NULL OR symbol ILIKE '%' || $4 || '%')
            ORDER BY created_at DESC LIMIT $5 OFFSET $6
        ) AS sq"#,
    )
    .bind(user.user_id)
    .bind(query.status)
    .bind(query.symbol)
    .bind(query.search)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(success_response("Strategies retrieved successfully", serde_json::json!({
        "items": strategies,
        "total": total,
        "page": page,
        "page_size": page_size
    }))))
}

async fn get_strategy_symbols(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let symbols = sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT symbol FROM strategies WHERE user_id = $1 ORDER BY symbol"#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(success_response("Symbols retrieved successfully", serde_json::json!(symbols))))
}

async fn get_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let strategy = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, LOWER(direction::text), entry_price::float8, stop_loss::float8, take_profit::float8, leverage, position_size::float8, LOWER(status::text), created_at, updated_at
            FROM strategies WHERE id = $1 AND user_id = $2
        ) AS sq"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Strategy not found".to_string()))?;

    Ok(Json(success_response("Strategy retrieved successfully", strategy)))
}

#[derive(Debug, Deserialize)]
struct UpdateStrategyRequest {
    direction: Option<String>,
    entry_price: Option<f64>,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    leverage: Option<i32>,
    position_size: Option<f64>,
}

async fn update_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
    Json(req): Json<UpdateStrategyRequest>,
) -> Result<Json<serde_json::Value>> {
    let strategy = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH upd AS (UPDATE strategies SET direction = COALESCE($3, direction), entry_price = COALESCE($4, entry_price), stop_loss = COALESCE($5, stop_loss), take_profit = COALESCE($6, take_profit), leverage = COALESCE($7, leverage), position_size = COALESCE($8, position_size), updated_at = NOW()
        WHERE id = $1 AND user_id = $2 RETURNING id, symbol, LOWER(direction::text), entry_price::float8, stop_loss::float8, take_profit::float8, leverage, position_size::float8, LOWER(status::text))
        SELECT row_to_json(upd) FROM upd"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .bind(req.direction)
    .bind(req.entry_price)
    .bind(req.stop_loss)
    .bind(req.take_profit)
    .bind(req.leverage)
    .bind(req.position_size)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Strategy not found".to_string()))?;

    Ok(Json(success_response("Strategy updated successfully", strategy)))
}

async fn delete_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"DELETE FROM strategies WHERE id = $1 AND user_id = $2"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Strategy not found".to_string()));
    }

    Ok(Json(serde_json::to_value(MessageResponse::new("Strategy deleted successfully")).unwrap()))
}

async fn execute_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE strategies SET status = 'active', updated_at = NOW() WHERE id = $1 AND user_id = $2 AND status::text IN ('paused', 'created') RETURNING id"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Strategy not found or cannot be executed".to_string()))?;

    Ok(Json(success_response("Strategy execution started", serde_json::json!({"strategy_id": strategy_id}))))
}

async fn cancel_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE strategies SET status = 'cancelled', updated_at = NOW() WHERE id = $1 AND user_id = $2 AND status::text IN ('active', 'paused') RETURNING id"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Strategy not found or cannot be cancelled".to_string()))?;

    Ok(Json(success_response("Strategy cancelled successfully", serde_json::json!({"strategy_id": strategy_id}))))
}

async fn pause_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE strategies SET status = 'paused', updated_at = NOW() WHERE id = $1 AND user_id = $2 AND status::text = 'active' RETURNING id"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Strategy not found or not active".to_string()))?;

    Ok(Json(success_response("Strategy paused successfully", serde_json::json!({"strategy_id": strategy_id}))))
}

async fn resume_strategy(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE strategies SET status = 'active', updated_at = NOW() WHERE id = $1 AND user_id = $2 AND status::text = 'paused' RETURNING id"#,
    )
    .bind(strategy_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Strategy not found or not paused".to_string()))?;

    Ok(Json(success_response("Strategy resumed successfully", serde_json::json!({"strategy_id": strategy_id}))))
}

async fn get_risk_metrics(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(_strategy_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let trades = sqlx::query(
        r#"SELECT pnl::float8 as pnl FROM trades WHERE user_id = $1 AND status = 'closed' AND pnl IS NOT NULL"#,
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let total_trades = trades.len() as f64;
    let wins = trades.iter().filter(|t| t.get::<f64, _>("pnl") > 0.0).count() as f64;
    let win_rate = if total_trades > 0.0 { wins / total_trades * 100.0 } else { 0.0 };
    let total_pnl: f64 = trades.iter().map(|t| t.get::<f64, _>("pnl")).sum();
    let avg_pnl = if total_trades > 0.0 { total_pnl / total_trades } else { 0.0 };

    Ok(Json(success_response("Risk metrics calculated successfully", serde_json::json!({
        "win_rate": win_rate,
        "total_trades": total_trades as i32,
        "total_pnl": total_pnl,
        "avg_pnl": avg_pnl,
        "max_drawdown": 0.0,
        "sharpe_ratio": 0.0,
    }))))
}
