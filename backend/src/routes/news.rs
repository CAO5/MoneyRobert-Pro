use axum::{
    extract::{State, Query, Path},
    routing::{get, post},
    Router,
    Json,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_news))
        .route("/{news_id}", get(get_news))
        .route("/fetch", post(fetch_news))
        .route("/recent/{symbol}", get(get_recent_news))
        .route("/sentiment/analyze", post(analyze_sentiment))
        .route("/sentiment/{symbol}", get(get_sentiment_summary))
        .route("/sentiment/{symbol}/aggregated", get(get_aggregated_sentiment))
        .route("/sentiment/{symbol}/history", get(get_sentiment_history))
}

#[derive(Debug, Deserialize)]
struct NewsQuery {
    source: Option<String>,
    symbol: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_news(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<NewsQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    let news = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, source, url, published_at, sentiment::float8, created_at
            FROM news_items WHERE ($1::text IS NULL OR source = $1) AND ($2::text IS NULL OR symbol = $2)
            ORDER BY published_at DESC LIMIT $3 OFFSET $4
        ) AS sq"#,
    )
    .bind(query.source)
    .bind(query.symbol)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": news, "page": page, "page_size": page_size})))
}

async fn get_news(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(news_id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    let news = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, source, url, published_at, sentiment::float8, symbol, created_at FROM news_items WHERE id = $1
        ) AS sq"#,
    )
    .bind(news_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("News not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": news})))
}

async fn fetch_news(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"message": "News fetch initiated", "status": "processing"})))
}

async fn get_recent_news(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let news = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, source, url, published_at, sentiment::float8 FROM news_items WHERE symbol = $1 ORDER BY published_at DESC LIMIT 10
        ) AS sq"#,
    )
    .bind(symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"items": news})))
}

#[derive(Debug, Deserialize)]
struct SentimentAnalyzeRequest {
    text: String,
    symbol: Option<String>,
}

async fn analyze_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<SentimentAnalyzeRequest>,
) -> Result<Json<serde_json::Value>> {
    let score: f64 = 0.5;
    Ok(Json(serde_json::json!({
        "text": req.text,
        "sentiment_score": score,
        "sentiment_label": if score > 0.6 { "positive" } else if score < 0.4 { "negative" } else { "neutral" },
    })))
}

async fn get_sentiment_summary(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let sentiment = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT sentiment_score::float8, platform, created_at FROM sentiment_data WHERE symbol = $1 ORDER BY created_at DESC LIMIT 10
        ) AS sq"#,
    )
    .bind(symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"data": sentiment})))
}

async fn get_aggregated_sentiment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let avg = sqlx::query_scalar::<_, Option<f64>>(
        r#"SELECT AVG(sentiment_score)::float8 FROM sentiment_data WHERE symbol = $1 AND created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) as count FROM sentiment_data WHERE symbol = $1 AND created_at > NOW() - INTERVAL '24 hours'"#,
    )
    .bind(&symbol)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "symbol": symbol,
        "avg_sentiment": avg.unwrap_or(0.0),
        "sample_count": count,
        "period": "24h",
    })))
}

async fn get_sentiment_history(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let history = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT sentiment_score::float8, platform, created_at FROM sentiment_data WHERE symbol = $1 ORDER BY created_at DESC LIMIT 100
        ) AS sq"#,
    )
    .bind(symbol)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({"history": history})))
}
