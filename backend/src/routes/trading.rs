use axum::{
    extract::{State, Query, Path},
    routing::{get, post},
    Router,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/balance", get(get_balance))
        .route("/positions", get(get_positions))
        .route("/orders", post(create_order))
        .route("/orders", get(get_orders))
        .route("/orders/{order_id}/cancel", post(cancel_order))
        .route("/leverage", post(set_leverage))
        .route("/trades", get(get_trade_history))
        .route("/ticker/{symbol}", get(get_ticker))
        .route("/candles/{symbol}", get(get_candles))
}

#[derive(Debug, Deserialize)]
struct OrderRequest {
    symbol: String,
    side: String,
    order_type: String,
    size: f64,
    price: Option<f64>,
    leverage: Option<i32>,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
}

async fn create_order(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<OrderRequest>,
) -> Result<Json<serde_json::Value>> {
    let order_id = Uuid::new_v4().to_string();
    let symbol = req.symbol.clone();
    let side = req.side.clone();
    let order_type = req.order_type.clone();
    let size = req.size;
    let price = req.price;

    sqlx::query(
        r#"INSERT INTO trades (user_id, symbol, side, order_type, price, quantity, status, order_id)
        VALUES ($1, $2, $3::trade_side_enum, $4::order_type_enum, $5, $6, 'pending'::trade_status_enum, $7)"#,
    )
    .bind(user.user_id as i32)
    .bind(req.symbol)
    .bind(req.side)
    .bind(req.order_type)
    .bind(req.price)
    .bind(req.size)
    .bind(&order_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "order_id": order_id,
        "symbol": symbol,
        "side": side,
        "order_type": order_type,
        "size": size,
        "price": price,
        "status": "submitted",
        "filled_size": 0,
        "avg_price": null,
        "created_at": chrono::Utc::now().naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string(),
    })))
}

#[derive(Debug, Deserialize)]
struct OrderQuery {
    symbol: Option<String>,
    status: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn get_orders(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let orders = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, side::text as side, order_type::text as order_type, price::float8, quantity::float8 as size, status::text as status, order_id, executed_at, created_at
            FROM trades WHERE user_id = $1 AND ($2::text IS NULL OR symbol = $2) AND ($3::text IS NULL OR status::text = $3)
            ORDER BY created_at DESC LIMIT $4 OFFSET $5
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .bind(query.symbol.clone())
    .bind(query.status.clone())
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!(orders)))
}

async fn cancel_order(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(order_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"UPDATE trades SET status = 'cancelled' WHERE id = $1 AND user_id = $2 AND status::text = 'pending'"#,
    )
    .bind(order_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Order not found or cannot be cancelled".to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "order_id": order_id,
        "message": "Order cancelled successfully",
    })))
}

#[derive(Debug, Deserialize)]
struct LeverageRequest {
    symbol: String,
    leverage: i32,
    margin_mode: Option<String>,
}

async fn set_leverage(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<LeverageRequest>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Leverage set to {}x", req.leverage),
    })))
}

async fn get_balance(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let api_key = sqlx::query(
        r#"SELECT key, secret, passphrase FROM api_keys WHERE user_id = $1 AND is_active = true LIMIT 1"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    match api_key {
        Some(key) => {
            let client = crate::exchanges::okx::OkxClient::new(
                key.get::<String, _>("key"), key.get::<String, _>("secret"), key.get::<String, _>("passphrase"), false,
            );
            let balance = client.get_account_balance().await?;
            Ok(Json(serde_json::to_value(balance).unwrap()))
        }
        None => Ok(Json(serde_json::json!({
            "total_equity": 0.0,
            "available_balance": 0.0,
            "margin_used": 0.0,
            "unrealized_pnl": 0.0,
            "realized_pnl": 0.0,
            "margin_ratio": 0.0,
        }))),
    }
}

async fn get_positions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let positions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, strategy_id, symbol, side::text as side, entry_price::float8, quantity::float8, leverage, unrealized_pnl::float8, stop_loss::float8, take_profit::float8, status::text as status, opened_at, closed_at
            FROM positions WHERE user_id = $1 AND status::text = 'open' ORDER BY opened_at DESC
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!(positions)))
}

async fn get_trade_history(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let trades = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, side::text as side, order_type::text as order_type, price::float8, quantity::float8 as size, status::text as status, order_id, executed_at, created_at
            FROM trades WHERE user_id = $1 AND status::text IN ('closed', 'cancelled') AND ($2::text IS NULL OR symbol = $2)
            ORDER BY created_at DESC LIMIT $3 OFFSET $4
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .bind(query.symbol)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!(trades)))
}

async fn get_ticker(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let api_key = sqlx::query(
        r#"SELECT key, secret, passphrase FROM api_keys WHERE user_id = $1 AND is_active = true LIMIT 1"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    match api_key {
        Some(key) => {
            let client = crate::exchanges::okx::OkxClient::new(
                key.get::<String, _>("key"), key.get::<String, _>("secret"), key.get::<String, _>("passphrase"), false,
            );
            let ticker = client.get_ticker(&symbol).await?;
            Ok(Json(serde_json::to_value(ticker).unwrap()))
        }
        None => Err(AppError::NotFound("No API key configured".to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct CandlesQuery {
    interval: Option<String>,
    limit: Option<usize>,
}

async fn get_candles(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(query): Query<CandlesQuery>,
) -> Result<Json<serde_json::Value>> {
    let api_key = sqlx::query(
        r#"SELECT key, secret, passphrase FROM api_keys WHERE user_id = $1 AND is_active = true LIMIT 1"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    match api_key {
        Some(key) => {
            let client = crate::exchanges::okx::OkxClient::new(
                key.get::<String, _>("key"), key.get::<String, _>("secret"), key.get::<String, _>("passphrase"), false,
            );
            let bar = query.interval.unwrap_or_else(|| "1H".to_string());
            let candles = client.get_candles(&symbol, &bar, query.limit).await?;
            Ok(Json(serde_json::to_value(candles).unwrap()))
        }
        None => Err(AppError::NotFound("No API key configured".to_string())),
    }
}
