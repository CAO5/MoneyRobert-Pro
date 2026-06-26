use axum::{
    extract::{State, Query},
    routing::get,
    Router,
    Json,
};
use chrono::Timelike; // hour() 方法来自 Timelike trait（Datelike 提供 year/month/day）
use serde::Deserialize;
use sqlx::Row;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::schemas::success_response;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(get_metrics))
        .route("/asset-distribution", get(get_asset_distribution))
        .route("/profit-trend", get(get_profit_trend))
        .route("/strategy-summary", get(get_strategy_summary))
        .route("/market-tickers", get(get_market_tickers))
        .route("/positions", get(get_positions_summary))
        // Mobile BFF：工作台首屏聚合接口，一次返回问候/指标/未读/快捷入口
        .route("/workbench", get(get_workbench))
}

async fn get_metrics(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let equity = sqlx::query(
        r#"SELECT total_equity, available_balance, unrealized_pnl, realized_pnl
        FROM equity_snapshots WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(user.user_id as i64)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let total_equity = equity.as_ref().and_then(|r| r.try_get::<f64, _>("total_equity").ok()).unwrap_or(0.0);
    let available_balance = equity.as_ref().and_then(|r| r.try_get::<f64, _>("available_balance").ok()).unwrap_or(0.0);
    let unrealized_pnl = equity.as_ref().and_then(|r| r.try_get::<f64, _>("unrealized_pnl").ok()).unwrap_or(0.0);
    let realized_pnl = equity.as_ref().and_then(|r| r.try_get::<f64, _>("realized_pnl").ok()).unwrap_or(0.0);

    let active_strategies = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM strategies WHERE user_id = $1 AND status::text = 'active'"#,
    )
    .bind(user.user_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let total_strategies = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM strategies WHERE user_id = $1"#,
    )
    .bind(user.user_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let today_pnl = sqlx::query_scalar::<_, f64>(
        r#"SELECT COALESCE(SUM(realized_pnl), 0) FROM equity_snapshots WHERE user_id = $1 AND created_at >= CURRENT_DATE"#,
    )
    .bind(user.user_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let open_positions = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM positions WHERE user_id = $1 AND status::text = 'open'"#,
    )
    .bind(user.user_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(success_response("Dashboard metrics retrieved successfully", serde_json::json!({
        "total_equity": total_equity,
        "available_balance": available_balance,
        "margin_used": total_equity - available_balance,
        "unrealized_pnl": unrealized_pnl,
        "realized_pnl": realized_pnl,
        "today_pnl": today_pnl,
        "today_pnl_percent": if total_equity > 0.0 { today_pnl / total_equity * 100.0 } else { 0.0 },
        "active_strategies": active_strategies,
        "total_strategies": total_strategies,
        "open_positions": open_positions,
        "avg_win_rate": 0.0,
    }))))
}

async fn get_asset_distribution(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let positions = sqlx::query(
        r#"SELECT symbol, LOWER(side::text) as side, size::float8, entry_price::float8, unrealized_pnl::float8, leverage FROM positions WHERE user_id = $1 AND status::text = 'open'"#,
    )
    .bind(user.user_id as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let total_value: f64 = positions.iter().map(|p| {
        let qty: f64 = p.get("size");
        let price: f64 = p.get("entry_price");
        qty * price
    }).sum();

    let distribution: Vec<serde_json::Value> = positions.iter().map(|p| {
        let qty: f64 = p.get("size");
        let price: f64 = p.get("entry_price");
        let value = qty * price;
        let percent = if total_value > 0.0 { value / total_value * 100.0 } else { 0.0 };
        serde_json::json!({
            "currency": p.get::<String, _>("symbol"),
            "balance": value,
            "available": value,
            "equity": value,
            "equity_usdt": value,
            "percent": percent,
            "price_usdt": price,
        })
    }).collect();

    Ok(Json(success_response("Asset distribution retrieved successfully", serde_json::json!(distribution))))
}

#[derive(Debug, Deserialize)]
struct ProfitTrendQuery {
    period: Option<String>,
}

async fn get_profit_trend(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ProfitTrendQuery>,
) -> Result<Json<serde_json::Value>> {
    let interval = match query.period.as_deref() {
        Some("week") => "7 days",
        Some("month") => "30 days",
        _ => "1 day",
    };

    let query_str = format!(
        r#"SELECT created_at::text as date, COALESCE(realized_pnl, 0)::float8 as pnl, COALESCE(total_equity, 0)::float8 as equity
        FROM equity_snapshots WHERE user_id = $1 AND created_at > CURRENT_DATE - INTERVAL '{}'
        ORDER BY created_at"#,
        interval
    );

    let rows = sqlx::query(&query_str)
        .bind(user.user_id as i64)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    let mut cumulative_pnl = 0.0_f64;
    let trend: Vec<serde_json::Value> = rows.iter().map(|row| {
        let pnl: f64 = row.get::<Option<f64>, _>("pnl").unwrap_or(0.0);
        cumulative_pnl += pnl;
        serde_json::json!({
            "date": row.get::<Option<String>,_>("date"),
            "pnl": pnl,
            "equity": row.get::<Option<f64>,_>("equity").unwrap_or(0.0),
            "cumulative_pnl": cumulative_pnl,
        })
    }).collect();

    Ok(Json(success_response("Profit trend retrieved successfully", serde_json::json!(trend))))
}

async fn get_strategy_summary(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let strategies = sqlx::query(
        r#"SELECT id, symbol, LOWER(status::text) as status, created_at::text as created_at FROM strategies WHERE user_id = $1 ORDER BY updated_at DESC NULLS LAST, created_at DESC LIMIT 10"#,
    )
    .bind(user.user_id as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = strategies.iter().map(|row| {
        serde_json::json!({
            "id": row.get::<i32, _>("id"),
            "name": row.get::<String, _>("symbol"),
            "symbol": row.get::<String, _>("symbol"),
            "status": row.get::<String, _>("status"),
            "profit": 0.0,
            "profit_percent": 0.0,
            "win_rate": 0.0,
            "total_trades": 0,
            "max_drawdown": 0.0,
        })
    }).collect();

    Ok(Json(success_response("Strategy summary retrieved successfully", serde_json::json!(items))))
}

async fn get_market_tickers(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let tickers = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT symbol, close::float8 as last, high::float8 as high_24h, low::float8 as low_24h, volume::float8 as volume_24h
            FROM market_data WHERE interval = '1D' AND open_time = (SELECT MAX(open_time) FROM market_data) ORDER BY symbol LIMIT 20
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = tickers.into_iter().map(|t| {
        serde_json::json!({
            "symbol": t["symbol"].as_str().unwrap_or_default(),
            "name": t["symbol"].as_str().unwrap_or_default(),
            "last": t["last"].as_f64().unwrap_or(0.0),
            "change_24h": 0.0,
            "change_percent_24h": 0.0,
            "volume_24h": t["volume_24h"].as_f64().unwrap_or(0.0),
            "high_24h": t["high_24h"].as_f64().unwrap_or(0.0),
            "low_24h": t["low_24h"].as_f64().unwrap_or(0.0),
        })
    }).collect();

    Ok(Json(success_response("Market tickers retrieved successfully", serde_json::json!(items))))
}

async fn get_positions_summary(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let positions = sqlx::query(
        r#"SELECT symbol, LOWER(side::text) as side, size::float8, entry_price::float8, unrealized_pnl::float8, leverage FROM positions WHERE user_id = $1 AND status::text = 'open' ORDER BY unrealized_pnl DESC"#,
    )
    .bind(user.user_id as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let items: Vec<serde_json::Value> = positions.iter().map(|p| {
        let entry_price: f64 = p.get("entry_price");
        let unrealized_pnl: f64 = p.get::<Option<f64>, _>("unrealized_pnl").unwrap_or(0.0);
        let unrealized_pnl_percent = if entry_price > 0.0 { unrealized_pnl / entry_price * 100.0 } else { 0.0 };
        serde_json::json!({
            "symbol": p.get::<String, _>("symbol"),
            "side": p.get::<String, _>("side"),
            "size": p.get::<f64, _>("size"),
            "avg_price": entry_price,
            "current_price": entry_price,
            "unrealized_pnl": unrealized_pnl,
            "unrealized_pnl_percent": unrealized_pnl_percent,
            "leverage": p.get::<Option<i32>, _>("leverage").unwrap_or(1),
        })
    }).collect();

    Ok(Json(success_response("Positions summary retrieved successfully", serde_json::json!(items))))
}

/// Mobile BFF：工作台首屏聚合
/// 一次返回问候语、关键指标、未读消息数、快捷入口，避免首屏并发多个接口
/// 对接 mobile workbenchService.getWorkbench()，响应由 request.ts unwrapResponse 自动解包 data
async fn get_workbench(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    // 问候语（按当前小时分段）
    let hour = chrono::Utc::now().hour();
    let greeting = match hour {
        5..=11 => "早上好",
        12..=13 => "中午好",
        14..=18 => "下午好",
        _ => "晚上好",
    };

    // 未读消息数（从 notifications 表统计 is_read=false）
    let unread_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false"#,
    )
    .bind(user.user_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    // 总权益（最新一条 equity_snapshots）
    let equity = sqlx::query(
        r#"SELECT total_equity FROM equity_snapshots WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(user.user_id as i64)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;
    let total_equity = equity
        .as_ref()
        .and_then(|r| r.try_get::<f64, _>("total_equity").ok())
        .unwrap_or(0.0);

    // 今日已实现盈亏
    let today_pnl = sqlx::query_scalar::<_, f64>(
        r#"SELECT COALESCE(SUM(realized_pnl), 0) FROM equity_snapshots WHERE user_id = $1 AND created_at >= CURRENT_DATE"#,
    )
    .bind(user.user_id as i64)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let today_trend = if today_pnl >= 0.0 { "up" } else { "down" };
    let today_percent = if total_equity > 0.0 {
        today_pnl / total_equity * 100.0
    } else {
        0.0
    };

    // 聚合返回（字段对齐 mobile WorkbenchData 类型）
    Ok(Json(success_response(
        "Workbench aggregated",
        serde_json::json!({
            "greeting": greeting,
            "todo_count": 0,
            "risk_alert_count": 0,
            "unread_message_count": unread_count,
            "metrics": [
                {
                    "key": "equity",
                    "label": "总权益",
                    "value": format!("{:.2}", total_equity),
                    "unit": "USDT",
                    "trend": today_trend,
                    "change_percent": today_percent,
                },
                {
                    "key": "today_pnl",
                    "label": "今日盈亏",
                    "value": format!("{:.2}", today_pnl),
                    "unit": "USDT",
                    "trend": today_trend,
                },
            ],
            "risk_alerts": [],
            "quick_entries": [
                {"key": "business", "label": "业务", "route": "/pages/business/index"},
                {"key": "todo", "label": "待办", "route": "/pages/todo/index"},
                {"key": "message", "label": "消息", "route": "/pages/message/index", "badge": unread_count},
                {"key": "mine", "label": "我的", "route": "/pages/mine/index"},
            ],
            "recent_items": [],
        }),
    )))
}
