use axum::{
    extract::{State, Query, Path},
    routing::{get, post},
    Router,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;
use crate::utils::encryption::decrypt;

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

/// Helper: get the user's OKX client from their active API key
pub async fn get_okx_client(
    state: &AppState,
    user_id: i64,
) -> Result<crate::exchanges::okx::OkxClient> {
    let key = sqlx::query(
        r#"SELECT key, secret, passphrase, metadata FROM api_keys WHERE user_id = $1 AND is_active = true AND key_type = 'exchange' LIMIT 1"#,
    )
    .bind(user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    match key {
        Some(row) => {
            let encrypted_key = row.get::<String, _>("key");
            let encrypted_secret = row.get::<String, _>("secret");
            let encrypted_passphrase = row.get::<String, _>("passphrase");
            let api_key = decrypt(&encrypted_key).map_err(|e| AppError::Internal(format!("解密 API Key 失败: {}", e)))?;
            let secret = decrypt(&encrypted_secret).map_err(|e| AppError::Internal(format!("解密 Secret 失败: {}", e)))?;
            let passphrase = decrypt(&encrypted_passphrase).unwrap_or_default();
            let metadata: serde_json::Value = row.get::<serde_json::Value, _>("metadata");
            let is_demo = metadata.get("is_demo")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok(crate::exchanges::okx::OkxClient::new(
                api_key, secret, passphrase, is_demo,
            ))
        }
        None => Err(AppError::NotFound("未配置交易所 API 密钥，请先在系统设置中添加".to_string())),
    }
}

// ==================== Balance ====================

async fn get_balance(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    match get_okx_client(&state, user.user_id).await {
        Ok(client) => {
            let balance = client.get_account_balance().await?;
            Ok(Json(serde_json::to_value(balance).unwrap_or_default()))
        }
        Err(_) => Ok(Json(serde_json::json!({
            "total_equity": "0",
            "available_balance": "0",
            "margin_used": "0",
            "unrealized_pnl": "0",
        }))),
    }
}

// ==================== Positions ====================

async fn get_positions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let client = match get_okx_client(&state, user.user_id).await {
        Ok(c) => c,
        Err(_) => return Ok(Json(serde_json::json!({ "positions": [] }))),
    };

    let positions = match client.get_positions(None).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to fetch positions from OKX: {}", e);
            return Ok(Json(serde_json::json!({ "positions": [], "error": format!("无法连接交易所: {}", e) })));
        }
    };

    // Filter out positions with zero size
    let active_positions: Vec<&crate::exchanges::okx::OkxPosition> = positions
        .iter()
        .filter(|p| {
            p.pos.as_deref().unwrap_or("0") != "0"
        })
        .collect();

    Ok(Json(serde_json::json!({
        "positions": active_positions.iter().map(|p| {
            let pos_size: f64 = p.pos.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
            let side = if pos_size > 0.0 { "long" } else { "short" };
            let abs_size = pos_size.abs();
            serde_json::json!({
                "inst_id": p.inst_id.as_deref().unwrap_or(""),
                "inst_type": p.inst_type.as_deref().unwrap_or(""),
                "mgn_mode": p.mgn_mode.as_deref().unwrap_or(""),
                "side": side,
                "size": abs_size,
                "avg_px": p.avg_px.as_deref().unwrap_or("0"),
                "mark_px": p.mark_px.as_deref().unwrap_or("0"),
                "upl": p.upl.as_deref().unwrap_or("0"),
                "upl_ratio": p.upl_ratio.as_deref().unwrap_or("0"),
                "lever": p.lever.as_deref().unwrap_or("1"),
                "liq_px": p.liq_px.as_deref().unwrap_or(""),
                "margin": p.margin.as_deref().unwrap_or("0"),
                "notional_usd": p.notional_usd.as_deref().unwrap_or("0"),
            })
        }).collect::<Vec<_>>(),
    })))
}

// ==================== Orders ====================

#[derive(Debug, Deserialize)]
struct CreateOrderRequest {
    symbol: String,
    side: String,        // "long" or "short"
    #[serde(rename = "type")]
    order_type: String,  // "market" or "limit"
    quantity: f64,
    price: Option<f64>,
    leverage: Option<i32>,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
}

async fn create_order(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateOrderRequest>,
) -> Result<Json<serde_json::Value>> {
    let client = get_okx_client(&state, user.user_id).await?;

    // Set leverage first if specified
    if let Some(leverage) = req.leverage {
        let _ = client.set_leverage(&req.symbol, &leverage.to_string(), "cross").await;
    }

    // Map side: long -> buy, short -> sell
    let okx_side = match req.side.as_str() {
        "long" => "buy",
        "short" => "sell",
        other => other,
    };

    // Map order type: market -> market, limit -> limit
    let okx_ord_type = match req.order_type.as_str() {
        "market" => "market",
        "limit" => "limit",
        other => other,
    };

    let order_request = crate::exchanges::okx::OkxOrderRequest {
        inst_id: req.symbol.clone(),
        td_mode: "cross".to_string(),
        side: okx_side.to_string(),
        ord_type: okx_ord_type.to_string(),
        sz: req.quantity.to_string(),
        px: req.price.map(|p| p.to_string()),
        sl_trigger_px: req.stop_loss.map(|p| p.to_string()),
        sl_ord_px: req.stop_loss.map(|p| p.to_string()),
        tp_trigger_px: req.take_profit.map(|p| p.to_string()),
        tp_ord_px: req.take_profit.map(|p| p.to_string()),
    };

    let result = client.place_order(&order_request).await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "order_id": result.ord_id,
        "symbol": req.symbol,
        "side": req.side,
        "type": req.order_type,
        "quantity": req.quantity,
        "price": req.price,
        "status": "submitted",
        "s_code": result.s_code,
        "s_msg": result.s_msg,
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
    Query(_query): Query<OrderQuery>,
) -> Result<Json<serde_json::Value>> {
    let client = match get_okx_client(&state, user.user_id).await {
        Ok(c) => c,
        Err(_) => return Ok(Json(serde_json::json!({ "orders": [] }))),
    };

    let params = vec![("instType", "SWAP".to_string())];

    let resp = match client.get_raw("/api/v5/trade/orders-pending", Some(&params)).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to fetch orders from OKX: {}", e);
            return Ok(Json(serde_json::json!({ "orders": [], "error": format!("无法连接交易所: {}", e) })));
        }
    };

    Ok(Json(serde_json::json!({
        "orders": resp,
    })))
}

async fn cancel_order(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(order_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let client = get_okx_client(&state, user.user_id).await?;

    // order_id format: "inst_id:ord_id"
    let parts: Vec<&str> = order_id.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(AppError::Validation("订单 ID 格式错误，应为 inst_id:ord_id".to_string()));
    }

    let result = client.cancel_order(parts[0], parts[1]).await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "order_id": result.ord_id,
        "s_code": result.s_code,
        "s_msg": result.s_msg,
    })))
}

// ==================== Leverage ====================

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
    let client = get_okx_client(&state, user.user_id).await?;
    let mgn_mode = req.margin_mode.as_deref().unwrap_or("cross");
    client.set_leverage(&req.symbol, &req.leverage.to_string(), mgn_mode).await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("杠杆已设置为 {}x", req.leverage),
    })))
}

// ==================== Trade History ====================

async fn get_trade_history(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> Result<Json<serde_json::Value>> {
    let client = get_okx_client(&state, user.user_id).await?;

    let mut params = vec![("instType", "SWAP".to_string())];
    if let Some(ref symbol) = query.symbol {
        params.push(("instId", symbol.clone()));
    }
    let page = query.page.unwrap_or(1);
    params.push(("page", page.to_string()));

    let resp = client.get_raw("/api/v5/trade/orders-history-archive", Some(&params)).await?;

    Ok(Json(serde_json::json!({
        "trades": resp,
    })))
}

// ==================== Ticker & Candles ====================

async fn get_ticker(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let client = get_okx_client(&state, user.user_id).await?;
    let ticker = client.get_ticker(&symbol).await?;
    Ok(Json(serde_json::to_value(ticker).unwrap_or_default()))
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
    let client = get_okx_client(&state, user.user_id).await?;
    let bar = query.interval.unwrap_or_else(|| "1H".to_string());
    let candles = client.get_candles(&symbol, &bar, query.limit).await?;
    Ok(Json(serde_json::to_value(candles).unwrap_or_default()))
}
