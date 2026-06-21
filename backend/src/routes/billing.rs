use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscription", get(get_subscription))
        .route("/subscription", post(create_subscription))
        .route("/subscription/cancel", post(cancel_subscription))
        .route("/records", get(get_billing_records))
        .route("/payment", post(create_payment))
        .route("/pay-per-use", post(pay_per_use))
        .route("/usage-records", get(get_usage_records))
        .route("/pricing", get(get_pricing))
        .route("/check-subscription", get(check_subscription))
}

async fn get_subscription(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let subscription = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, subscription_type, is_active, start_date, end_date FROM subscriptions WHERE user_id = $1 AND is_active = true ORDER BY created_at DESC LIMIT 1) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(subscription.unwrap_or(serde_json::Value::Null)))
}

#[derive(Debug, Deserialize)]
struct CreateSubscriptionRequest {
    subscription_type: String,
}

async fn create_subscription(
    _user: CurrentUser,
    State(_state): State<AppState>,
    Json(_req): Json<CreateSubscriptionRequest>,
) -> Result<Json<serde_json::Value>> {
    Err(AppError::NotImplemented(
        "Subscription activation requires a configured payment provider".to_string(),
    ))
}

async fn cancel_subscription(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE subscriptions SET is_active = false, updated_at = NOW() WHERE user_id = $1 AND is_active = true"#,
    )
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(
        serde_json::json!({"message": "Subscription cancelled"}),
    ))
}

#[derive(Debug, Deserialize)]
struct BillingQuery {
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn get_billing_records(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<BillingQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let records = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, billing_type, amount::float8, currency, status, description, created_at FROM billing_records WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3) AS sq"#
    )
    .bind(user.user_id as i32)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(
        serde_json::json!({"items": records, "page": page, "page_size": page_size}),
    ))
}

#[derive(Debug, Deserialize)]
struct PaymentRequest {
    amount: f64,
    payment_method: String,
}

async fn create_payment(
    _user: CurrentUser,
    State(_state): State<AppState>,
    Json(_req): Json<PaymentRequest>,
) -> Result<Json<serde_json::Value>> {
    Err(AppError::NotImplemented(
        "Payment provider is not configured".to_string(),
    ))
}

#[derive(Debug, Deserialize)]
struct PayPerUseRequest {
    service_type: String,
    quantity: Option<i32>,
}

async fn pay_per_use(
    _user: CurrentUser,
    State(_state): State<AppState>,
    Json(_req): Json<PayPerUseRequest>,
) -> Result<Json<serde_json::Value>> {
    Err(AppError::NotImplemented(
        "Usage billing requires a configured payment provider".to_string(),
    ))
}

async fn get_usage_records(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let records = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, service_type, quantity, cost::float8, created_at FROM usage_records WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50) AS sq"#
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"records": records})))
}

async fn get_pricing(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "plans": [
            {"name": "free", "price": 0, "features": ["basic_analysis", "paper_trading"]},
            {"name": "pro", "price": 29.99, "features": ["ai_analysis", "auto_trading", "real_time_data"]},
            {"name": "enterprise", "price": 99.99, "features": ["all_features", "priority_support", "custom_strategies"]},
        ]
    })))
}

async fn check_subscription(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let active = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) as count FROM subscriptions WHERE user_id = $1 AND is_active = true"#,
    )
    .bind(user.user_id as i32)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(
        serde_json::json!({"has_active_subscription": active > 0}),
    ))
}
