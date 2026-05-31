use axum::{
    extract::{State, Query, Path},
    routing::{get, post, delete},
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
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{session_id}", get(get_session))
        .route("/sessions/{session_id}", delete(delete_session))
        .route("/sessions/{session_id}/messages", get(get_messages))
        .route("/sessions/{session_id}/messages", post(send_message))
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    title: Option<String>,
}

async fn create_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>> {
    let session = sqlx::query(
        r#"INSERT INTO ai_chat_sessions (user_id, title) VALUES ($1, $2) RETURNING id, title, created_at"#,
    )
    .bind(user.user_id as i32)
    .bind(req.title.unwrap_or_else(|| "New Chat".to_string()))
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": {
        "id": session.get::<i32, _>("id"),
        "title": session.get::<String, _>("title"),
        "created_at": session.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    }})))
}

async fn list_sessions(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let sessions = sqlx::query(
        r#"SELECT id, title, created_at, updated_at FROM ai_chat_sessions WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 50"#,
    )
    .bind(user.user_id as i32)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let sessions: Vec<_> = sessions.iter().map(|s| serde_json::json!({
        "id": s.get::<i32, _>("id"),
        "title": s.get::<String, _>("title"),
        "created_at": s.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updated_at": s.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })).collect();

    Ok(Json(serde_json::json!({"sessions": sessions})))
}

async fn get_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let session = sqlx::query(
        r#"SELECT id, title, created_at, updated_at FROM ai_chat_sessions WHERE id = $1 AND user_id = $2"#,
    )
    .bind(session_id)
    .bind(user.user_id as i32)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Session not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": {
        "id": session.get::<i32, _>("id"),
        "title": session.get::<String, _>("title"),
        "created_at": session.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updated_at": session.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    }})))
}

async fn delete_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"DELETE FROM ai_chat_sessions WHERE id = $1 AND user_id = $2"#,
    )
    .bind(session_id)
    .bind(user.user_id as i32)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"message": "Session deleted"})))
}

async fn get_messages(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let messages = sqlx::query(
        r#"SELECT id, role, content, created_at FROM ai_chat_messages WHERE session_id = $1 ORDER BY created_at ASC LIMIT 200"#,
    )
    .bind(session_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let messages: Vec<_> = messages.iter().map(|m| serde_json::json!({
        "id": m.get::<i32, _>("id"),
        "role": m.get::<String, _>("role"),
        "content": m.get::<String, _>("content"),
        "created_at": m.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect();

    Ok(Json(serde_json::json!({"messages": messages})))
}

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    content: String,
}

async fn send_message(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(session_id): Path<i32>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>> {
    let user_msg = sqlx::query(
        r#"INSERT INTO ai_chat_messages (session_id, role, content) VALUES ($1, 'user', $2) RETURNING id"#,
    )
    .bind(session_id)
    .bind(&req.content)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let ai_response = "I'm analyzing your request. Based on current market conditions, I recommend monitoring the situation closely.";

    let ai_msg = sqlx::query(
        r#"INSERT INTO ai_chat_messages (session_id, role, content) VALUES ($1, 'assistant', $2) RETURNING id"#,
    )
    .bind(session_id)
    .bind(ai_response)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    sqlx::query(
        r#"UPDATE ai_chat_sessions SET updated_at = NOW() WHERE id = $1"#,
    )
    .bind(session_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "user_message_id": user_msg.get::<i32, _>("id"),
        "ai_message_id": ai_msg.get::<i32, _>("id"),
        "response": ai_response,
    })))
}
