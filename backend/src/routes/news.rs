use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use feed_rs::parser;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::extractors::{require_role, CurrentUser};
use crate::state::{get_proxy_config_from_db, AppState};

const FEED_SOURCES: &[FeedSource] = &[
    FeedSource {
        name: "CryptoSlate",
        url: "https://cryptoslate.com/feed/",
    },
    FeedSource {
        name: "Cointelegraph",
        url: "https://cointelegraph.com/rss",
    },
    FeedSource {
        name: "Decrypt",
        url: "https://decrypt.co/feed",
    },
];

#[derive(Clone, Copy)]
struct FeedSource {
    name: &'static str,
    url: &'static str,
}

#[derive(Debug)]
struct NewsItem {
    title: String,
    content: Option<String>,
    source: String,
    url: String,
    published_at: DateTime<Utc>,
    sentiment: f64,
    related_symbols: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SourceResult {
    source: String,
    fetched: usize,
    error: Option<String>,
    #[serde(skip)]
    items: Vec<NewsItem>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_news))
        .route("/fetch", post(fetch_news))
        .route("/recent/{symbol}", get(get_recent_news))
        .route("/sentiment/analyze", post(analyze_sentiment))
        .route("/sentiment/{symbol}", get(get_sentiment_summary))
        .route(
            "/sentiment/{symbol}/aggregated",
            get(get_aggregated_sentiment),
        )
        .route("/sentiment/{symbol}/history", get(get_sentiment_history))
        .route("/{news_id}", get(get_news))
}

#[derive(Debug, Deserialize)]
struct NewsQuery {
    source: Option<String>,
    symbol: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

async fn list_news(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<NewsQuery>,
) -> Result<Json<serde_json::Value>> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * page_size;
    let symbol = query.symbol.map(|value| normalize_symbol(&value));

    let news = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, source, url, published_at,
                   sentiment::float8, related_symbols, created_at
            FROM news
            WHERE ($1::text IS NULL OR source = $1)
              AND ($2::text IS NULL OR $2 = ANY(COALESCE(related_symbols, ARRAY[]::text[])))
            ORDER BY published_at DESC LIMIT $3 OFFSET $4
        ) AS sq"#,
    )
    .bind(query.source)
    .bind(symbol)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(
        serde_json::json!({"items": news, "page": page, "page_size": page_size}),
    ))
}

async fn get_news(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(news_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let news = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, content, source, url, published_at,
                   sentiment::float8, related_symbols, created_at
            FROM news WHERE id = $1
        ) AS sq"#,
    )
    .bind(news_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("News not found".to_string()))?;

    Ok(Json(serde_json::json!({"data": news})))
}

async fn fetch_news(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    require_role(user, "admin").await?;
    Ok(Json(refresh_news(&state).await?))
}

pub async fn refresh_news(state: &AppState) -> Result<serde_json::Value> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(5))
        .user_agent("MoneyRobert/1.0 news aggregator");

    if let Some(proxy_url) = get_proxy_config_from_db(&state.db_pool).await {
        builder = builder.proxy(reqwest::Proxy::all(&proxy_url).map_err(|error| {
            AppError::Validation(format!("Invalid proxy configuration: {error}"))
        })?);
    }

    let client = builder.build()?;
    let mut transaction = state.db_pool.begin().await?;
    let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock($1)")
        .bind(7_347_791_101_i64)
        .fetch_one(&mut *transaction)
        .await?;
    if !acquired {
        return Ok(serde_json::json!({
            "message": "News refresh already running",
            "status": "skipped",
        }));
    }
    let results = join_all(
        FEED_SOURCES
            .iter()
            .map(|source| fetch_feed(&client, *source)),
    )
    .await;
    let successful_sources = results
        .iter()
        .filter(|result| result.error.is_none())
        .count();
    if successful_sources == 0 {
        let details = results
            .iter()
            .filter_map(|result| {
                result
                    .error
                    .as_ref()
                    .map(|error| format!("{}: {error}", result.source))
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::ExternalApi {
            service: "news feeds".to_string(),
            message: format!("All news sources failed: {details}"),
        });
    }

    let fetched = results.iter().map(|result| result.fetched).sum::<usize>();
    let mut inserted = 0_u64;
    for item in results.iter().flat_map(|result| result.items.iter()) {
        let outcome = sqlx::query(
            r#"INSERT INTO news
               (title, content, source, url, published_at, sentiment, related_symbols)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (url) DO NOTHING"#,
        )
        .bind(&item.title)
        .bind(&item.content)
        .bind(&item.source)
        .bind(&item.url)
        .bind(item.published_at)
        .bind(item.sentiment)
        .bind(&item.related_symbols)
        .execute(&mut *transaction)
        .await?;
        inserted += outcome.rows_affected();
    }
    transaction.commit().await?;

    Ok(serde_json::json!({
        "message": "News fetch completed",
        "status": if successful_sources == FEED_SOURCES.len() { "success" } else { "partial_success" },
        "fetched": fetched,
        "inserted": inserted,
        "duplicates": fetched.saturating_sub(inserted as usize),
        "sources": results,
    }))
}

async fn fetch_feed(client: &reqwest::Client, source: FeedSource) -> SourceResult {
    let response = match client.get(source.url).send().await {
        Ok(response) => match response.error_for_status() {
            Ok(response) => response,
            Err(error) => return source_error(source, format!("request failed: {error}")),
        },
        Err(error) => return source_error(source, format!("request failed: {error}")),
    };
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => return source_error(source, format!("response read failed: {error}")),
    };
    let feed = match parser::parse(&bytes[..]) {
        Ok(feed) => feed,
        Err(error) => return source_error(source, format!("feed parse failed: {error}")),
    };

    let items = feed
        .entries
        .into_iter()
        .take(50)
        .filter_map(|entry| {
            let title = entry.title?.content.trim().to_string();
            let url = entry.links.first()?.href.trim().to_string();
            if title.is_empty() || url.is_empty() {
                return None;
            }
            let content = entry
                .summary
                .map(|summary| truncate(&summary.content, 20_000))
                .or_else(|| {
                    entry
                        .content
                        .and_then(|content| content.body.map(|body| truncate(&body, 20_000)))
                });
            let searchable = format!("{} {}", title, content.as_deref().unwrap_or_default());
            Some(NewsItem {
                title: truncate(&title, 500),
                content,
                source: source.name.to_string(),
                url,
                published_at: entry.published.or(entry.updated).unwrap_or_else(Utc::now),
                sentiment: estimate_sentiment(&searchable),
                related_symbols: detect_symbols(&searchable),
            })
        })
        .collect::<Vec<_>>();

    SourceResult {
        source: source.name.to_string(),
        fetched: items.len(),
        error: None,
        items,
    }
}

fn source_error(source: FeedSource, error: String) -> SourceResult {
    tracing::warn!(source = source.name, %error, "News source fetch failed");
    SourceResult {
        source: source.name.to_string(),
        fetched: 0,
        error: Some(error),
        items: vec![],
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn normalize_symbol(symbol: &str) -> String {
    let upper = symbol
        .trim()
        .to_uppercase()
        .replace('/', "-")
        .replace('_', "-");
    if upper.contains('-') {
        upper
    } else {
        format!("{upper}-USDT")
    }
}

fn detect_symbols(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    [
        ("BTC-USDT", ["bitcoin", " btc", "$btc"]),
        ("ETH-USDT", ["ethereum", " ether", " eth"]),
        ("SOL-USDT", ["solana", " sol", "$sol"]),
        ("XRP-USDT", ["ripple", " xrp", "$xrp"]),
        ("DOGE-USDT", ["dogecoin", " doge", "$doge"]),
    ]
    .into_iter()
    .filter(|(_, keywords)| keywords.iter().any(|keyword| lower.contains(keyword)))
    .map(|(symbol, _)| symbol.to_string())
    .collect()
}

fn estimate_sentiment(text: &str) -> f64 {
    let lower = text.to_lowercase();
    let positive = [
        "surge",
        "rally",
        "gain",
        "bull",
        "record high",
        "approval",
        "adoption",
        "breakout",
    ];
    let negative = [
        "crash",
        "drop",
        "loss",
        "bear",
        "hack",
        "lawsuit",
        "ban",
        "liquidation",
        "fraud",
    ];
    let score = positive
        .iter()
        .filter(|word| lower.contains(**word))
        .count() as i32
        - negative
            .iter()
            .filter(|word| lower.contains(**word))
            .count() as i32;
    (0.5 + score.clamp(-4, 4) as f64 * 0.1).clamp(0.0, 1.0)
}

async fn get_recent_news(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let news = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
            SELECT id, title, source, url, published_at, sentiment::float8, related_symbols
            FROM news WHERE $1 = ANY(COALESCE(related_symbols, ARRAY[]::text[]))
            ORDER BY published_at DESC LIMIT 10
        ) AS sq"#,
    )
    .bind(normalize_symbol(&symbol))
    .fetch_all(&state.db_pool)
    .await?;
    Ok(Json(serde_json::json!({"items": news})))
}

#[derive(Debug, Deserialize)]
struct SentimentAnalyzeRequest {
    text: String,
    symbol: Option<String>,
}

async fn analyze_sentiment(
    _user: CurrentUser,
    State(_state): State<AppState>,
    Json(req): Json<SentimentAnalyzeRequest>,
) -> Result<Json<serde_json::Value>> {
    let score = estimate_sentiment(&req.text);
    Ok(Json(serde_json::json!({
        "text": req.text, "symbol": req.symbol, "sentiment_score": score,
        "sentiment_label": if score > 0.6 { "positive" } else if score < 0.4 { "negative" } else { "neutral" },
    })))
}

async fn get_sentiment_summary(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let data = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
        SELECT sentiment_score::float8, platform, created_at FROM sentiment_data
        WHERE symbol = $1 ORDER BY created_at DESC LIMIT 10) AS sq"#,
    )
    .bind(symbol)
    .fetch_all(&state.db_pool)
    .await?;
    Ok(Json(serde_json::json!({"data": data})))
}

async fn get_aggregated_sentiment(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let avg = sqlx::query_scalar::<_, Option<f64>>("SELECT AVG(sentiment_score)::float8 FROM sentiment_data WHERE symbol = $1 AND created_at > NOW() - INTERVAL '24 hours'")
        .bind(&symbol).fetch_one(&state.db_pool).await?;
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sentiment_data WHERE symbol = $1 AND created_at > NOW() - INTERVAL '24 hours'")
        .bind(&symbol).fetch_one(&state.db_pool).await?;
    Ok(Json(
        serde_json::json!({"symbol": symbol, "avg_sentiment": avg.unwrap_or(0.0), "sample_count": count, "period": "24h"}),
    ))
}

async fn get_sentiment_history(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let history = sqlx::query_scalar::<_, serde_json::Value>(
        r#"SELECT row_to_json(sq) FROM (
        SELECT sentiment_score::float8, platform, created_at FROM sentiment_data
        WHERE symbol = $1 ORDER BY created_at DESC LIMIT 100) AS sq"#,
    )
    .bind(symbol)
    .fetch_all(&state.db_pool)
    .await?;
    Ok(Json(serde_json::json!({"history": history})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_detects_symbols() {
        assert_eq!(normalize_symbol("btc/usdt"), "BTC-USDT");
        assert_eq!(normalize_symbol("eth"), "ETH-USDT");
        assert_eq!(
            detect_symbols("Bitcoin and Solana rally"),
            vec!["BTC-USDT", "SOL-USDT"]
        );
    }

    #[test]
    fn sentiment_stays_bounded() {
        assert!(estimate_sentiment("surge rally gain bull approval adoption breakout") <= 1.0);
        assert!(estimate_sentiment("crash hack fraud ban") >= 0.0);
    }
}
