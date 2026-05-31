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
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{user_id}", get(get_user))
        .route("/users", post(create_user))
        .route("/users/{user_id}", put(update_user))
        .route("/users/{user_id}", delete(delete_user))
        .route("/users/{user_id}/toggle-active", post(toggle_user_active))
        .route("/logs", get(get_system_logs))
        .route("/logs/stats", get(get_log_stats))
        .route("/stats", get(get_admin_stats))
}

async fn list_users(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let users = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, username, email, role::text, is_active, created_at, updated_at FROM users ORDER BY created_at DESC LIMIT 100) AS sq"#
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"users": users})))
}

async fn get_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let target = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, username, email, role::text, is_active, created_at, updated_at FROM users WHERE id = $1) AS sq"#,
    )
    .bind(user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": target})))
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    username: String,
    email: String,
    password: String,
    role: Option<String>,
}

async fn create_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let password_hash = crate::auth::hash_password(&req.password)?;
    let new_user = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO users (username, email, hashed_password, role, is_active) VALUES ($1, $2, $3, $4, true) RETURNING id, username, email, role::text) SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(req.username)
    .bind(req.email)
    .bind(password_hash)
    .bind(req.role.unwrap_or_else(|| "NORMAL".to_string()))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": new_user})))
}

#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    username: Option<String>,
    email: Option<String>,
    role: Option<String>,
    is_active: Option<bool>,
}

async fn update_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let result = sqlx::query(
        r#"UPDATE users SET username = COALESCE($3, username), email = COALESCE($4, email), role = COALESCE($5::user_role_enum, role), is_active = COALESCE($6, is_active), updated_at = NOW() WHERE id = $1 AND ($2::text IS NULL OR true) RETURNING id"#,
    )
    .bind(user_id)
    .bind(user.role)
    .bind(req.username)
    .bind(req.email)
    .bind(req.role)
    .bind(req.is_active)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(serde_json::json!({"message": "User updated"})))
}

async fn delete_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    sqlx::query(r#"DELETE FROM users WHERE id = $1"#, )
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"message": "User deleted"})))
}

async fn toggle_user_active(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let result = sqlx::query(
        r#"UPDATE users SET is_active = NOT is_active, updated_at = NOW() WHERE id = $1 RETURNING id, is_active"#,
    )
    .bind(user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(serde_json::json!({"user_id": result.get::<i32, _>("id"), "is_active": result.get::<bool, _>("is_active")})))
}

#[derive(Debug, Deserialize)]
struct LogQuery {
    level: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn get_system_logs(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(50).min(200);
    let offset = (page - 1) * page_size;

    let logs = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT id, action, resource, details, created_at FROM system_logs WHERE ($1::text IS NULL OR action = $1) ORDER BY created_at DESC LIMIT $2 OFFSET $3) AS sq"#
    )
    .bind(query.level)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": logs, "page": page, "page_size": page_size})))
}

async fn get_log_stats(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let stats = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (SELECT action, COUNT(*) as count FROM system_logs WHERE created_at > NOW() - INTERVAL '24 hours' GROUP BY action) AS sq"#
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"stats": stats})))
}

async fn get_admin_stats(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    if user.role != "admin" {
        return Err(AppError::Authorization("Admin access required".to_string()));
    }

    let total_users = sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) as count FROM users"#)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    let active_users = sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) as count FROM users WHERE is_active = true"#)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    let total_trades = sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) as count FROM trades"#)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "total_users": total_users,
        "active_users": active_users,
        "total_trades": total_trades,
    })))
}
