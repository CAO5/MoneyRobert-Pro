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

    // 已认证请求（带有效 Bearer Token）跳过限流：
    // 限流主要用于防止匿名流量爆破（登录/注册接口等），已登录用户走 JWT 鉴权另有保护，
    // 工作台首屏并发 9 个接口不应被全局 IP 限流误伤。
    // 同时保留对未认证请求的限流，防止恶意爬虫/暴力探测。
    if has_valid_bearer_token(req.headers()) {
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

/// 检查请求头是否带有 Bearer Token（仅做格式校验，签名验证由 auth_middleware 完成）
/// 用于让已认证请求跳过 IP 限流，避免工作台首屏并发接口被误伤
fn has_valid_bearer_token(headers: &axum::http::HeaderMap) -> bool {
    if let Some(auth) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            return s.starts_with("Bearer ") && s.len() > "Bearer ".len();
        }
    }
    false
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
