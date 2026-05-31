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
        .route("/", get(list_sentiment))
        .route("/stats", get(get_sentiment_stats))
        .route("/{sentiment_id}", get(get_sentiment))
        .route("/", post(create_sentiment))
        .route("/batch", post(batch_create_sentiment))
        .route("/{sentiment_id}", put(update_sentiment))
        .route("/{sentiment_id}", delete(delete_sentiment))
        .route("/symbol/{symbol}/for-ai", get(get_sentiment_for_ai))
}

#[derive(Debug, Deserialize)]
struct SentimentQuery {
    symbol: Option<String>,
    source_type: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SentimentQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let data = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, sentiment_score::float8, source_type, created_at FROM sentiment_data
            WHERE ($1::text IS NULL OR symbol = $1) AND ($2::text IS NULL OR source_type = $2)
            ORDER BY created_at DESC LIMIT $3 OFFSET $4
        ) AS sq"#,
    )
    .bind(query.symbol)
    .bind(query.source_type)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": data, "page": page, "page_size": page_size})))
}

async fn get_sentiment_stats(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let by_source = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT source_type, COUNT(*) as count, AVG(sentiment_score)::float8 as avg_score FROM sentiment_data GROUP BY source_type
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let by_symbol = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT symbol, COUNT(*) as count, AVG(sentiment_score)::float8 as avg_score FROM sentiment_data GROUP BY symbol ORDER BY avg_score DESC LIMIT 20
        ) AS sq"#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"by_source": by_source, "by_symbol": by_symbol})))
}

async fn get_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(sentiment_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let data = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, symbol, sentiment_score::float8, source_type, extra_data, created_at FROM sentiment_data WHERE id = $1
        ) AS sq"#,
    )
    .bind(sentiment_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Sentiment data not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": data})))
}

#[derive(Debug, Deserialize)]
struct CreateSentimentRequest {
    symbol: String,
    platform: String,
    source_type: String,
    content: String,
    sentiment_type: String,
    sentiment_score: Option<f64>,
    title: Option<String>,
    author: Option<String>,
    extra_data: Option<serde_json::Value>,
}

async fn create_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateSentimentRequest>,
) -> Result<Json<serde_json::Value>> {
    let data = sqlx::query_scalar::<_, serde_json::Value>(
        r#"WITH ins AS (INSERT INTO sentiment_data (user_id, symbol, platform, source_type, content, sentiment_type, sentiment_score, title, author, extra_data, is_verified, is_kol, is_active, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, false, false, true, NOW(), NOW()) RETURNING id, symbol, sentiment_score::float8, source_type, created_at)
        SELECT row_to_json(ins) FROM ins"#,
    )
    .bind(user.user_id as i32)
    .bind(req.symbol)
    .bind(req.platform)
    .bind(req.source_type)
    .bind(req.content)
    .bind(req.sentiment_type)
    .bind(req.sentiment_score)
    .bind(req.title)
    .bind(req.author)
    .bind(req.extra_data)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": data})))
}

#[derive(Debug, Deserialize)]
struct BatchCreateRequest {
    items: Vec<CreateSentimentRequest>,
}

async fn batch_create_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<BatchCreateRequest>,
) -> Result<Json<serde_json::Value>> {
    let mut created = Vec::new();
    for item in req.items {
        let id: i32 = sqlx::query_scalar(
            r#"INSERT INTO sentiment_data (user_id, symbol, platform, source_type, content, sentiment_type, sentiment_score, title, author, extra_data, is_verified, is_kol, is_active, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, false, false, true, NOW(), NOW()) RETURNING id"#,
        )
        .bind(user.user_id as i32)
        .bind(item.symbol)
        .bind(item.platform)
        .bind(item.source_type)
        .bind(item.content)
        .bind(item.sentiment_type)
        .bind(item.sentiment_score)
        .bind(item.title)
        .bind(item.author)
        .bind(item.extra_data)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;
        created.push(id);
    }

    Ok(Json(serde_json::json!({"created_count": created.len(), "ids": created})))
}

#[derive(Debug, Deserialize)]
struct UpdateSentimentRequest {
    sentiment_score: Option<f64>,
    extra_data: Option<serde_json::Value>,
}

async fn update_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(sentiment_id): Path<i32>,
    Json(req): Json<UpdateSentimentRequest>,
) -> Result<Json<serde_json::Value>> {
    sqlx::query(
        r#"UPDATE sentiment_data SET sentiment_score = COALESCE($3, sentiment_score), extra_data = COALESCE($4, extra_data), updated_at = NOW() WHERE id = $1 AND user_id = $2 RETURNING id"#,
    )
    .bind(sentiment_id)
    .bind(user.user_id as i32)
    .bind(req.sentiment_score)
    .bind(req.extra_data)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("Sentiment data not found".to_string()))?;

    Ok(Json(serde_json::json!({"message": "Updated successfully"})))
}

async fn delete_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(sentiment_id): Path<i32>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query(
        r#"DELETE FROM sentiment_data WHERE id = $1"#,
    )
    .bind(sentiment_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Sentiment data not found".to_string()));
    }

    Ok(Json(serde_json::json!({"message": "Deleted successfully"})))
}

async fn get_sentiment_for_ai(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let data = sqlx::query(
        r#"SELECT sentiment_score::float8, source_type, created_at FROM sentiment_data WHERE symbol = $1 ORDER BY created_at DESC LIMIT 50"#,
    )
    .bind(&symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let avg = if data.is_empty() { 0.0 } else { data.iter().map(|d| d.get::<f64, _>("sentiment_score")).sum::<f64>() / data.len() as f64 };

    let recent_data: Vec<serde_json::Value> = data.iter().map(|d| {
        serde_json::json!({
            "sentiment_score": d.get::<f64, _>("sentiment_score"),
            "source_type": d.get::<String, _>("source_type"),
            "created_at": d.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "avg_sentiment": avg,
        "sample_count": recent_data.len(),
        "recent_data": recent_data,
    })))
}
