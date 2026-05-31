use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::state::AppState;

pub struct RateLimitEntry {
    pub count: u32,
    pub reset_at: Instant,
}

pub fn get_client_ip(headers: &axum::http::HeaderMap) -> String {
    if let Some(val) = headers.get("X-Forwarded-For") {
        if let Ok(s) = val.to_str() {
            if let Some(ip) = s.split(',').next() {
                let trimmed = ip.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }

    if let Some(val) = headers.get("X-Real-IP") {
        if let Ok(s) = val.to_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    "127.0.0.1".to_string()
}

pub async fn rate_limit(
    state: axum::extract::State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    if !state.config.rate_limit.enabled {
        return next.run(req).await;
    }

    let ip = get_client_ip(req.headers());
    let rate_limit_map: &Arc<DashMap<String, RateLimitEntry>> = &state.rate_limit_map;
    let requests_per_minute = state.config.rate_limit.requests_per_minute;

    let should_reject = {
        let mut entry = rate_limit_map.entry(ip.clone()).or_insert_with(|| RateLimitEntry {
            count: 0,
            reset_at: Instant::now() + Duration::from_secs(60),
        });

        let now = Instant::now();
        if now >= entry.reset_at {
            entry.count = 0;
            entry.reset_at = now + Duration::from_secs(60);
        }

        entry.count += 1;
        entry.count > requests_per_minute
    };

    if should_reject {
        return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    next.run(req).await
}

pub async fn request_logging(
    state: axum::extract::State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let duration = start.elapsed();

    let status = response.status();

    if state.config.is_development() {
        tracing::info!(
            method = %method,
            uri = %uri,
            status = status.as_u16(),
            duration_ms = duration.as_millis(),
            "Request completed"
        );
    } else if status.is_server_error() {
        tracing::error!(
            method = %method,
            uri = %uri,
            status = status.as_u16(),
            duration_ms = duration.as_millis(),
            "Server error"
        );
    }

    response
}
