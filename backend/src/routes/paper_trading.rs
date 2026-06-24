use axum::{
    extract::State,
    routing::{get, post},
    Router,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::backtest::execution::{
    ExecutionConfig, OrderSide, PaperTradingExecutor,
};
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
        .route("/fills", get(get_fill_history))
}

/// 获取执行配置（从系统设置中读取，或使用默认值）
fn get_execution_config() -> ExecutionConfig {
    ExecutionConfig::default()
}

async fn get_account(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let executor = PaperTradingExecutor::with_defaults(state.db_pool.clone(), user.user_id as i64);
    let summary = executor.get_account_summary().await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({"data": summary})))
}

async fn reset_account(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    // 关闭所有持仓
    let closed_positions = sqlx::query(
        r#"UPDATE positions SET status = 'CLOSED', updated_at = NOW()
           WHERE user_id = $1 AND status = 'OPEN'"#,
    )
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    // 重置账户
    sqlx::query(
        r#"INSERT INTO paper_trading_accounts
           (user_id, balance, initial_balance, total_pnl, total_fees, total_slippage_cost,
            margin_used, total_equity, peak_equity, drawdown_pct)
           VALUES ($1, 100000, 100000, 0, 0, 0, 0, 100000, 100000, 0)
           ON CONFLICT (user_id) DO UPDATE
           SET balance = 100000, initial_balance = 100000, total_pnl = 0,
               total_fees = 0, total_slippage_cost = 0, margin_used = 0,
               total_equity = 100000, peak_equity = 100000, drawdown_pct = 0,
               updated_at = NOW()"#,
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
            SELECT id, symbol, side::text as side, quantity::float8, entry_price::float8,
                   filled_price::float8, unrealized_pnl::float8, leverage,
                   fee::float8, slippage_bps::float8, margin::float8, notional::float8,
                   stop_loss, take_profit, opened_at
            FROM positions WHERE user_id = $1 AND status = 'OPEN'
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
    // 映射 side: "long" -> Buy, "short" -> Sell, "buy" -> Buy, "sell" -> Sell
    let order_side = match req.side.to_lowercase().as_str() {
        "long" | "buy" => OrderSide::Buy,
        "short" | "sell" => OrderSide::Sell,
        other => return Err(AppError::Validation(format!("Invalid side: {}", other))),
    };

    // 参考价格：使用请求中的价格，或从最新 K 线获取
    let ref_price = req.price.unwrap_or_else(|| {
        // 如果没有提供价格，使用 0 作为占位（前端应始终提供价格）
        tracing::warn!("Paper trading order without price, using 0 as placeholder");
        0.0
    });

    if ref_price <= 0.0 {
        return Err(AppError::Validation("Price must be provided and positive".into()));
    }

    let leverage = req.leverage.unwrap_or(1);
    let config = get_execution_config();
    let executor = PaperTradingExecutor::new(config, state.db_pool.clone(), user.user_id as i64);

    let result = executor
        .open_position(
            &req.symbol,
            order_side,
            req.size,
            ref_price,
            leverage,
            req.stop_loss,
            req.take_profit,
        )
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(serde_json::json!({
        "data": {
            "position_id": result.position_id.to_string(),
            "symbol": req.symbol,
            "side": order_side.as_str(),
            "quantity": req.size,
            "filled_price": result.fill.price,
            "fee": result.fee,
            "slippage_cost": result.slippage_cost,
            "margin": result.margin,
            "remaining_balance": result.remaining_balance,
        },
        "status": "filled"
    })))
}

#[derive(Debug, Deserialize)]
struct ClosePositionRequest {
    position_id: String,
    exit_price: f64,
    reason: Option<String>,
}

async fn close_position(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ClosePositionRequest>,
) -> Result<Json<serde_json::Value>> {
    let position_id: Uuid = req
        .position_id
        .parse()
        .map_err(|e| AppError::Validation(format!("Invalid position_id: {}", e)))?;

    let config = get_execution_config();
    let executor = PaperTradingExecutor::new(config, state.db_pool.clone(), user.user_id as i64);

    let result = executor
        .close_position(position_id, req.exit_price, req.reason.as_deref())
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(serde_json::json!({
        "data": {
            "position_id": position_id.to_string(),
            "exit_price": result.fill.price,
            "gross_pnl": result.gross_pnl,
            "fee": result.fee,
            "net_pnl": result.net_pnl,
            "margin_released": result.margin_released,
            "remaining_balance": result.remaining_balance,
        },
        "status": "closed"
    })))
}

async fn get_trade_history(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let trades = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, side::text as side, entry_price::float8, exit_price::float8,
                   size::float8, leverage, status::text as status,
                   pnl::float8, pnl_percent::float8,
                   entry_fee::float8, exit_fee::float8, slippage_bps::float8,
                   gross_pnl::float8, net_pnl::float8, margin::float8,
                   close_reason, created_at, updated_at
            FROM trades WHERE user_id = $1 AND status::text = 'CLOSED'
            ORDER BY created_at DESC LIMIT 50
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"trades": trades})))
}

/// 获取统一成交记录（paper_trading_fills 表）
async fn get_fill_history(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let fills = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT fill_id, symbol, side, quantity::float8, price::float8, notional::float8,
                   fee::float8, slippage_bps::float8, slippage_cost::float8,
                   fee_rate_bps::float8, is_maker, fill_time, position_id, close_reason
            FROM paper_trading_fills WHERE user_id = $1
            ORDER BY fill_time DESC LIMIT 100
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"fills": fills})))
}
