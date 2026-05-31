use axum::{
    extract::{State, Query, Path},
    routing::{get, put, delete, post},
    Router,
    Json,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notifications))
        .route("/stats", get(get_notification_stats))
        .route("/{notification_id}", get(get_notification))
        .route("/{notification_id}/read", put(mark_as_read))
        .route("/read-all", put(mark_all_read))
        .route("/{notification_id}", delete(delete_notification))
        .route("/read", delete(delete_read_notifications))
        .route("/settings", get(get_notification_settings))
        .route("/settings", put(update_notification_settings))
        .route("/test", post(test_notification))
}

#[derive(Debug, Deserialize)]
struct NotificationQuery {
    notification_type: Option<String>,
    is_read: Option<bool>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_notifications(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND ($2::text IS NULL OR type::text = $2) AND ($3::bool IS NULL OR is_read = $3)"#,
    )
    .bind(user.user_id as i32)
    .bind(query.notification_type.clone())
    .bind(query.is_read)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let unread_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let notifications = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, user_id, LOWER(type::text) as type, title, content, LOWER(channel::text) as channel, is_read, sent_at, created_at FROM notifications
            WHERE user_id = $1 AND ($2::text IS NULL OR type::text = $2) AND ($3::bool IS NULL OR is_read = $3)
            ORDER BY created_at DESC LIMIT $4 OFFSET $5
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .bind(query.notification_type)
    .bind(query.is_read)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": notifications, "total": total, "page": page, "page_size": page_size, "unread_count": unread_count})))
}

async fn get_notification_stats(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let unread = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM notifications WHERE user_id = $1"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let by_type: serde_json::Value = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT json_object_agg(LOWER(type::text), cnt) FROM (SELECT type::text, COUNT(*) as cnt FROM notifications WHERE user_id = $1 GROUP BY type::text) t"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .unwrap_or(serde_json::json!({}));

    let by_channel: serde_json::Value = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT json_object_agg(LOWER(channel::text), cnt) FROM (SELECT channel::text, COUNT(*) as cnt FROM notifications WHERE user_id = $1 GROUP BY channel::text) t"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .unwrap_or(serde_json::json!({}));

    Ok(Json(serde_json::json!({
        "total_count": total,
        "unread_count": unread,
        "by_type": by_type,
        "by_channel": by_channel,
    })))
}

async fn get_notification(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let notification = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, user_id, LOWER(type::text) as type, title, content, LOWER(channel::text) as channel, is_read, sent_at, created_at FROM notifications WHERE id = $1 AND user_id = $2
        ) AS sq"#,
    )
    .bind(notification_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

    Ok(Json(notification))
}

async fn mark_as_read(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE notifications SET is_read = true WHERE id = $1 AND user_id = $2"#,
    )
    .bind(notification_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let notification = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, user_id, LOWER(type::text) as type, title, content, LOWER(channel::text) as channel, is_read, sent_at, created_at FROM notifications WHERE id = $1 AND user_id = $2
        ) AS sq"#,
    )
    .bind(notification_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

    Ok(Json(notification))
}

async fn mark_all_read(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false"#,
    )
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let count = result.rows_affected();

    Ok(Json(serde_json::json!({"message": format!("Marked {} notifications as read", count)})))
}

async fn delete_notification(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(notification_id): Path<i32>,
) -> Result<StatusCode> {
    sqlx::query(
        r#"DELETE FROM notifications WHERE id = $1 AND user_id = $2"#,
    )
    .bind(notification_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_read_notifications(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<StatusCode> {
    sqlx::query(
        r#"DELETE FROM notifications WHERE user_id = $1 AND is_read = true"#,
    )
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_notification_settings(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let settings = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT notification_settings FROM users WHERE id = $1"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let settings_val = settings.unwrap_or(serde_json::json!({}));

    Ok(Json(serde_json::json!({
        "email_enabled": settings_val.get("email_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "sms_enabled": settings_val.get("sms_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
        "wechat_enabled": settings_val.get("wechat_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
        "stop_loss_enabled": settings_val.get("stop_loss_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "take_profit_enabled": settings_val.get("take_profit_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "circuit_breaker_enabled": settings_val.get("circuit_breaker_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "liquidation_risk_enabled": settings_val.get("liquidation_risk_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "consecutive_loss_enabled": settings_val.get("consecutive_loss_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "market_volatility_enabled": settings_val.get("market_volatility_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "phone": settings_val.get("phone").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })))
}

#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    settings: serde_json::Value,
}

async fn update_notification_settings(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE users SET notification_settings = $1 WHERE id = $2"#,
    )
    .bind(req.settings.clone())
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let settings_val = req.settings;

    Ok(Json(serde_json::json!({
        "email_enabled": settings_val.get("email_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "sms_enabled": settings_val.get("sms_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
        "wechat_enabled": settings_val.get("wechat_enabled").and_then(|v| v.as_bool()).unwrap_or(false),
        "stop_loss_enabled": settings_val.get("stop_loss_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "take_profit_enabled": settings_val.get("take_profit_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "circuit_breaker_enabled": settings_val.get("circuit_breaker_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "liquidation_risk_enabled": settings_val.get("liquidation_risk_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "consecutive_loss_enabled": settings_val.get("consecutive_loss_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "market_volatility_enabled": settings_val.get("market_volatility_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "phone": settings_val.get("phone").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })))
}

#[derive(Debug, Deserialize)]
struct TestNotificationRequest {
    channel: Option<String>,
    message: Option<String>,
}

async fn test_notification(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<TestNotificationRequest>,
) -> Result<Json<serde_json::Value>> {
    let channel = req.channel.unwrap_or_else(|| "in_app".to_string());

    sqlx::query(
        r#"INSERT INTO notifications (user_id, type, title, content, channel, is_read) VALUES ($1, 'system', 'Test Notification', $2, $3::notification_channel_enum, false)"#,
    )
    .bind(user.user_id as i32)
    .bind(req.message.unwrap_or_else(|| "This is a test notification".to_string()))
    .bind(&channel)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Test notification sent via {} channel", channel),
        "channel": channel,
    })))
}
