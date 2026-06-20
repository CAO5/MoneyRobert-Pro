use axum::{
    extract::State,
    routing::{get, post},
    Router,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/account", get(get_account))
        .route("/account/reset", post(reset_account))
        .route("/positions", get(get_positions))
        .route("/orders", post(create_order))
        .route("/positions/close", post(close_position))
        .route("/trades", get(get_trade_history))
}

async fn get_account(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let account = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, available_balance_usdt::float8 as balance, total_equity_usdt::float8 as initial_balance, realized_pnl_usdt::float8 as total_pnl, created_at FROM equity_snapshots WHERE user_id = $1 ORDER BY snapshot_date DESC LIMIT 1
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    match account {
        Some(acc) => Ok(Json(serde_json::json!({"data": acc}))),
        None => Ok(Json(serde_json::json!({"data": {"balance": 100000, "initial_balance": 100000, "total_pnl": 0}}))),
    }
}

async fn reset_account(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    // Close all open positions for the user
    let closed_positions = sqlx::query(
        r#"UPDATE positions SET status = 'CLOSED', updated_at = NOW()
           WHERE user_id = $1 AND status = 'OPEN'"#,
    )
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    // Reset paper trading account balance to initial value
    let reset = sqlx::query(
        r#"INSERT INTO paper_trading_accounts (user_id, balance, initial_balance, total_pnl, total_trades, winning_trades, losing_trades)
           VALUES ($1, 100000, 100000, 0, 0, 0, 0)
           ON CONFLICT (user_id) DO UPDATE
           SET balance = 100000, initial_balance = 100000, total_pnl = 0,
               total_trades = 0, winning_trades = 0, losing_trades = 0, updated_at = NOW()"#,
    )
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "message": "Account reset successfully",
        "balance": 100000,
        "closed_positions": closed_positions.rows_affected()
    })))
}

async fn get_positions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let positions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, side::text as side, quantity::float8, entry_price::float8, unrealized_pnl::float8, leverage, opened_at FROM positions WHERE user_id = $1 AND status::text = 'OPEN'
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"positions": positions})))
}

#[derive(Debug, Deserialize)]
struct PaperOrderRequest {
    symbol: String,
    side: String,
    size: f64,
    price: Option<f64>,
    leverage: Option<i32>,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
}

async fn create_order(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<PaperOrderRequest>,
) -> Result<Json<serde_json::Value>> {
    let entry_price = req.price.unwrap_or(0.0);

    let position = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO positions (user_id, symbol, side, quantity, entry_price, unrealized_pnl, leverage, stop_loss, take_profit, status)
        VALUES ($1, $2, $3::trade_side_enum, $4, $5, 0, $6, $7, $8, 'OPEN') RETURNING id, symbol, side::text as side, quantity::float8, entry_price::float8)
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id as i32)
    .bind(req.symbol)
    .bind(req.side)
    .bind(req.size)
    .bind(entry_price)
    .bind(req.leverage.unwrap_or(1) as i32)
    .bind(req.stop_loss)
    .bind(req.take_profit)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": position, "status": "filled"})))
}

#[derive(Debug, Deserialize)]
struct ClosePositionRequest {
    position_id: i32,
    exit_price: f64,
}

async fn close_position(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ClosePositionRequest>,
) -> Result<Json<serde_json::Value>> {
    let position = sqlx::query(
        r#"SELECT symbol, side::text as side, quantity::float8, entry_price::float8 FROM positions WHERE id = $1 AND user_id = $2 AND status::text = 'OPEN'"#,
    )
    .bind(req.position_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Position not found".to_string()))?;

    let side: String = position.get("side");
    let quantity: f64 = position.get("quantity");
    let entry_price: f64 = position.get("entry_price");
    let symbol: String = position.get("symbol");

    let pnl = match side.as_str() {
        "BUY" => (req.exit_price - entry_price) * quantity,
        "SELL" => (entry_price - req.exit_price) * quantity,
        _ => 0.0,
    };

    sqlx::query(
        r#"UPDATE positions SET status = 'CLOSED', closed_at = NOW() WHERE id = $1"#,
    )
    .bind(req.position_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    sqlx::query(
        r#"INSERT INTO trades (user_id, symbol, side, order_type, price, quantity, status, executed_at) VALUES ($1, $2, $3::trade_side_enum, 'MARKET', $4, $5, 'FILLED', NOW())"#,
    )
    .bind(user.user_id as i32)
    .bind(symbol)
    .bind(side)
    .bind(entry_price)
    .bind(quantity)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"pnl": pnl, "status": "closed"})))
}

async fn get_trade_history(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let trades = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, side::text as side, price::float8, quantity::float8, status::text as status, executed_at, created_at FROM trades WHERE user_id = $1 AND status::text = 'FILLED' ORDER BY created_at DESC LIMIT 50
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"trades": trades})))
}
