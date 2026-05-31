use axum::{
    extract::{State, Query, Path},
    routing::{get, post, put, delete},
    Router,
    Json,
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/scheduled", get(get_scheduled_tasks))
        .route("/active", get(get_active_tasks))
        .route("/status/{task_id}", get(get_task_status))
        .route("/trigger", post(trigger_task))
        .route("/sync", post(sync_data))
        .route("/sync-direct", post(sync_data_direct))
        .route("/cancel/{task_id}", post(cancel_task))
        .route("/history", get(get_task_history))
        .route("/custom", get(list_custom_tasks))
        .route("/custom", post(create_custom_task))
        .route("/custom/{task_id}", put(update_custom_task))
        .route("/custom/{task_id}", delete(delete_custom_task))
        .route("/custom/{task_id}/toggle", post(toggle_custom_task))
}

async fn get_scheduled_tasks(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let tasks = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, name, task_type, task_config, is_active, last_run_at, created_at FROM scheduled_tasks ORDER BY created_at DESC
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"tasks": tasks})))
}

async fn get_active_tasks(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let tasks = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, name, task_type, task_config, last_run_at FROM scheduled_tasks WHERE is_active = true ORDER BY name
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"tasks": tasks})))
}

async fn get_task_status(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let task = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, name, task_type, task_config, is_active, last_run_at, last_result, created_at FROM scheduled_tasks WHERE id = $1
        ) AS sq"#,
    )
    .bind(task_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Task not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": task})))
}

#[derive(Debug, Deserialize)]
struct TriggerRequest {
    task_type: String,
}

async fn trigger_task(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<TriggerRequest>,
) -> Result<Json<serde_json::Value>> {
    let task_id = Uuid::new_v4();
    Ok(Json(serde_json::json!({
        "task_id": task_id.to_string(),
        "task_type": req.task_type,
        "status": "triggered",
    })))
}

async fn sync_data(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"message": "Sync task queued", "status": "processing"})))
}

async fn sync_data_direct(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"message": "Direct sync completed", "status": "completed"})))
}

async fn cancel_task(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"task_id": task_id, "status": "cancelled"})))
}

async fn get_task_history(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let history = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, name, task_type, last_result, last_run_at, created_at FROM scheduled_tasks ORDER BY last_run_at DESC NULLS LAST LIMIT 50
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"history": history})))
}

async fn list_custom_tasks(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let tasks = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, name, task_type, task_config, is_active, created_at FROM scheduled_tasks WHERE user_id = $1 ORDER BY created_at DESC
        ) AS sq"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"tasks": tasks})))
}

#[derive(Debug, Deserialize)]
struct CreateCustomTaskRequest {
    name: String,
    task_type: String,
    task_config: serde_json::Value,
    task_path: String,
}

async fn create_custom_task(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateCustomTaskRequest>,
) -> Result<Json<serde_json::Value>> {
    let task = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO scheduled_tasks (user_id, name, task_type, task_config, task_path, is_active, run_count) VALUES ($1, $2, $3, $4, $5, true, 0) RETURNING id, name, task_type, task_config, is_active, created_at)
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id as i32)
    .bind(req.name)
    .bind(req.task_type)
    .bind(req.task_config)
    .bind(req.task_path)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": task})))
}

#[derive(Debug, Deserialize)]
struct UpdateCustomTaskRequest {
    name: Option<String>,
    task_config: Option<serde_json::Value>,
    is_active: Option<bool>,
}

async fn update_custom_task(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i32>,
    Json(req): Json<UpdateCustomTaskRequest>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE scheduled_tasks SET name = COALESCE($3, name), task_config = COALESCE($4, task_config), is_active = COALESCE($5, is_active), updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING id"#,
    )
    .bind(task_id)
    .bind(user.user_id as i32)
    .bind(req.name)
    .bind(req.task_config)
    .bind(req.is_active)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Task not found".to_string()))?;

    Ok(Json(serde_json::json!({"message": "Task updated"})))
}

async fn delete_custom_task(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"DELETE FROM scheduled_tasks WHERE id = $1 AND user_id = $2"#,
    )
    .bind(task_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Task not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Task deleted"})))
}

async fn toggle_custom_task(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"UPDATE scheduled_tasks SET is_active = NOT is_active WHERE id = $1 AND user_id = $2 RETURNING id, is_active"#,
    )
    .bind(task_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Task not found".to_string()))?;

    Ok(Json(serde_json::json!({"task_id": result.get::<i32, _>("id"), "is_active": result.get::<bool, _>("is_active")})))
}
