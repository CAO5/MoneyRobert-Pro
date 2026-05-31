use axum::{
    extract::State,
    routing::get,
    Router,
    Json,
};
use redis::AsyncCommands;
use serde::Serialize;

use crate::error::Result;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(health_check))
        .route("/ready", get(ready_check))
        .route("/live", get(live_check))
}

#[derive(Serialize)]
struct HealthStatus {
    status: String,
    version: String,
    environment: String,
    timestamp: String,
    redis: String,
    database: String,
}

#[derive(Serialize)]
struct SimpleStatus {
    status: String,
}

async fn health_check(State(state): State<AppState>) -> Result<Json<HealthStatus>> {
    let redis_status = match &state.redis {
        Some(redis) => {
            let conn_result: redis::RedisResult<()> = redis.clone().ping().await;
            if conn_result.is_ok() {
                "connected".to_string()
            } else {
                "disconnected".to_string()
            }
        }
        None => "not configured".to_string(),
    };

    let db_status = sqlx::query("SELECT 1")
        .fetch_optional(&state.db_pool)
        .await
        .map(|_| "connected".to_string())
        .unwrap_or_else(|_| "disconnected".to_string());

    Ok(Json(HealthStatus {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: state.config.server.environment.clone(),
        timestamp: chrono::Utc::now().naive_utc().format("%Y-%m-%dT%H:%M:%S%.6f").to_string(),
        redis: redis_status,
        database: db_status,
    }))
}

async fn ready_check(State(state): State<AppState>) -> Result<Json<SimpleStatus>> {
    let db_ok = sqlx::query("SELECT 1")
        .fetch_optional(&state.db_pool)
        .await
        .is_ok();

    if db_ok {
        Ok(Json(SimpleStatus { status: "ready".to_string() }))
    } else {
        Err(crate::error::AppError::Internal("Database not ready".to_string()))
    }
}

async fn live_check() -> Json<SimpleStatus> {
    Json(SimpleStatus { status: "alive".to_string() })
}
