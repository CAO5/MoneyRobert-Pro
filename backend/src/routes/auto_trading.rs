use axum::{
    extract::{State, Query, Path},
    routing::{get, post, put},
    Router,
    Json,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/configs", post(create_config))
        .route("/configs", get(list_configs))
        .route("/configs/{config_id}", get(get_config))
        .route("/configs/{config_id}", put(update_config))
        .route("/configs/{config_id}/enable", post(enable_config))
        .route("/configs/{config_id}/disable", post(disable_config))
        .route("/start", post(start_auto_trading))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{session_id}", get(get_session))
        .route("/sessions/{session_id}/close", post(close_session))
        .route("/monitor", get(monitor_positions))
}

#[derive(Debug, Deserialize)]
struct CreateConfigRequest {
    name: String,
    symbols: serde_json::Value,
    mode: Option<String>,
    max_position_size: Option<f64>,
    max_leverage: Option<i32>,
    risk_per_trade: Option<f64>,
    max_daily_trades: Option<i32>,
    max_daily_loss: Option<f64>,
    stop_loss_percent: Option<f64>,
    take_profit_percent: Option<f64>,
    ai_confidence_threshold: Option<f64>,
    auto_entry: Option<bool>,
    auto_exit: Option<bool>,
    enable_stop_loss: Option<bool>,
    enable_take_profit: Option<bool>,
    ai_analysis_version: Option<String>,
}

async fn create_config(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateConfigRequest>,
) -> Result<Json<serde_json::Value>> {
    let config = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO auto_trading_configs (user_id, name, mode, status, symbols, max_position_size, max_leverage, risk_per_trade, max_daily_trades, max_daily_loss, stop_loss_percent, take_profit_percent, ai_confidence_threshold, auto_entry, auto_exit, enable_stop_loss, enable_take_profit, ai_analysis_version, created_at, updated_at)
        VALUES ($1, $2, $3, 'inactive', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW(), NOW()) RETURNING id, name, mode, status, symbols, max_position_size::float8, max_leverage, risk_per_trade::float8, max_daily_trades, max_daily_loss::float8, stop_loss_percent::float8, take_profit_percent::float8, ai_confidence_threshold::float8, auto_entry, auto_exit, enable_stop_loss, enable_take_profit, ai_analysis_version, created_at, updated_at) SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id as i32)
    .bind(req.name)
    .bind(req.mode.unwrap_or_else(|| "paper".to_string()))
    .bind(req.symbols)
    .bind(req.max_position_size.unwrap_or(10000.0))
    .bind(req.max_leverage.unwrap_or(1))
    .bind(req.risk_per_trade.unwrap_or(2.0))
    .bind(req.max_daily_trades.unwrap_or(10))
    .bind(req.max_daily_loss.unwrap_or(5.0))
    .bind(req.stop_loss_percent.unwrap_or(2.0))
    .bind(req.take_profit_percent.unwrap_or(5.0))
    .bind(req.ai_confidence_threshold.unwrap_or(70.0))
    .bind(req.auto_entry.unwrap_or(false))
    .bind(req.auto_exit.unwrap_or(false))
    .bind(req.enable_stop_loss.unwrap_or(true))
    .bind(req.enable_take_profit.unwrap_or(true))
    .bind(req.ai_analysis_version.unwrap_or_else(|| "v1".to_string()))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(config))
}

async fn list_configs(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let configs = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, name, mode, status, symbols, max_position_size::float8, max_leverage, risk_per_trade::float8, max_daily_trades, max_daily_loss::float8, stop_loss_percent::float8, take_profit_percent::float8, ai_confidence_threshold::float8, auto_entry, auto_exit, enable_stop_loss, enable_take_profit, ai_analysis_version, created_at, updated_at FROM auto_trading_configs WHERE user_id = $1 ORDER BY created_at DESC) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"configs": configs})))
}

async fn get_config(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(config_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let config = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, name, mode, status, symbols, max_position_size::float8, max_leverage, risk_per_trade::float8, max_daily_trades, max_daily_loss::float8, stop_loss_percent::float8, take_profit_percent::float8, ai_confidence_threshold::float8, auto_entry, auto_exit, enable_stop_loss, enable_take_profit, ai_analysis_version, trading_hours, notification_settings, created_at, updated_at
        FROM auto_trading_configs WHERE id = $1 AND user_id = $2) AS sq"#,
    )
    .bind(config_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Config not found".to_string()))?;

    Ok(Json(config))
}

#[derive(Debug, Deserialize)]
struct UpdateConfigRequest {
    name: Option<String>,
    symbols: Option<serde_json::Value>,
    max_position_size: Option<f64>,
    max_leverage: Option<i32>,
    risk_per_trade: Option<f64>,
    max_daily_trades: Option<i32>,
    max_daily_loss: Option<f64>,
    stop_loss_percent: Option<f64>,
    take_profit_percent: Option<f64>,
    ai_confidence_threshold: Option<f64>,
    auto_entry: Option<bool>,
    auto_exit: Option<bool>,
    enable_stop_loss: Option<bool>,
    enable_take_profit: Option<bool>,
    ai_analysis_version: Option<String>,
    trading_hours: Option<serde_json::Value>,
    notification_settings: Option<serde_json::Value>,
}

async fn update_config(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(config_id): Path<i32>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"UPDATE auto_trading_configs SET name = COALESCE($3, name), symbols = COALESCE($4, symbols), max_position_size = COALESCE($5, max_position_size), max_leverage = COALESCE($6, max_leverage),
        risk_per_trade = COALESCE($7, risk_per_trade), max_daily_trades = COALESCE($8, max_daily_trades), max_daily_loss = COALESCE($9, max_daily_loss), stop_loss_percent = COALESCE($10, stop_loss_percent),
        take_profit_percent = COALESCE($11, take_profit_percent), ai_confidence_threshold = COALESCE($12, ai_confidence_threshold),
        auto_entry = COALESCE($13, auto_entry), auto_exit = COALESCE($14, auto_exit), enable_stop_loss = COALESCE($15, enable_stop_loss), enable_take_profit = COALESCE($16, enable_take_profit),
        ai_analysis_version = COALESCE($17, ai_analysis_version), trading_hours = COALESCE($18, trading_hours), notification_settings = COALESCE($19, notification_settings),
        updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING id"#,
    )
    .bind(config_id)
    .bind(user.user_id as i32)
    .bind(req.name)
    .bind(req.symbols)
    .bind(req.max_position_size)
    .bind(req.max_leverage)
    .bind(req.risk_per_trade)
    .bind(req.max_daily_trades)
    .bind(req.max_daily_loss)
    .bind(req.stop_loss_percent)
    .bind(req.take_profit_percent)
    .bind(req.ai_confidence_threshold)
    .bind(req.auto_entry)
    .bind(req.auto_exit)
    .bind(req.enable_stop_loss)
    .bind(req.enable_take_profit)
    .bind(req.ai_analysis_version)
    .bind(req.trading_hours)
    .bind(req.notification_settings)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Config not found".to_string()))?;

    Ok(Json(serde_json::json!({"message": "Config updated"})))
}

async fn enable_config(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(config_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE auto_trading_configs SET status = 'active', updated_at = NOW() WHERE id = $1 AND user_id = $2"#,
    )
    .bind(config_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"message": "Config enabled"})))
}

async fn disable_config(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(config_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE auto_trading_configs SET status = 'inactive', updated_at = NOW() WHERE id = $1 AND user_id = $2"#,
    )
    .bind(config_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"message": "Config disabled"})))
}

async fn start_auto_trading(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"message": "Auto trading started", "status": "running"})))
}

async fn list_sessions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let sessions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, config_id, symbol, mode, status, direction::text as direction, leverage, entry_price::float8, current_price::float8, position_size::float8, stop_loss::float8, take_profit::float8, pnl::float8, pnl_percent::float8, created_at, updated_at FROM auto_trading_sessions WHERE user_id = $1 ORDER BY created_at DESC) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"sessions": sessions})))
}

async fn get_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let session = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, config_id, symbol, mode, status, direction::text as direction, ai_prediction_id, ai_confidence::float8, ai_reasoning, entry_price::float8, current_price::float8, position_size::float8, leverage, stop_loss::float8, take_profit::float8, entry_order_id, entry_at, exit_price::float8, exit_order_id, exit_at, exit_reason, pnl::float8, pnl_percent::float8, triggered_type, triggered_price::float8, triggered_at, error_message, created_at, updated_at FROM auto_trading_sessions WHERE id = $1 AND user_id = $2) AS sq"#,
    )
    .bind(session_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Session not found".to_string()))?;

    Ok(Json(session))
}

async fn close_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE auto_trading_sessions SET status = 'closed', updated_at = NOW() WHERE id = $1 AND user_id = $2"#,
    )
    .bind(session_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"message": "Session closed"})))
}

async fn monitor_positions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let positions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, symbol, side, size::float8, entry_price::float8, unrealized_pnl::float8, leverage FROM positions WHERE user_id = $1 AND status = 'open') AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"positions": positions, "monitor_status": "active"})))
}
