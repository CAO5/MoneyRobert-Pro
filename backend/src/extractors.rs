use axum::extract::{FromRequestParts, Request};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::auth::Claims;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: i64,
    pub username: String,
    pub role: String,
}

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<Claims>()
            .ok_or_else(|| AppError::Authentication("Authentication required".to_string()))?;

        Ok(Self {
            user_id: claims.get_user_id(),
            username: claims.get_username(),
            role: claims.get_role().to_lowercase(),
        })
    }
}

pub async fn auth_middleware(
    state: axum::extract::State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(auth) if auth.starts_with("Bearer ") => {
            let token = &auth[7..];
            match Claims::from_token(token, &state.config.security.secret_key) {
                Ok(mut claims) if claims.is_access_token() => {
                    let user = sqlx::query_as::<_, (String, String)>(
                        "SELECT username, LOWER(role::text) FROM users WHERE id = $1 AND is_active = true",
                    )
                    .bind(claims.get_user_id())
                    .fetch_optional(&state.db_pool)
                    .await;

                    match user {
                        Ok(Some((username, role))) => {
                            claims.username = Some(username);
                            claims.role = Some(role);
                            request.extensions_mut().insert(claims);
                            next.run(request).await
                        }
                        Ok(None) => error_response(
                            StatusCode::UNAUTHORIZED,
                            "User is inactive or no longer exists",
                        ),
                        Err(_) => error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed to validate user session",
                        ),
                    }
                }
                Ok(_) => error_response(StatusCode::UNAUTHORIZED, "Access token required"),
                Err(e) => error_response(StatusCode::UNAUTHORIZED, &e.to_string()),
            }
        }
        _ => error_response(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid authorization header",
        ),
    }
}

pub async fn require_role(user: CurrentUser, required_role: &str) -> Result<(), AppError> {
    let roles = match user.role.as_str() {
        "admin" => vec!["admin", "trader", "viewer", "normal"],
        "trader" => vec!["trader", "viewer", "normal"],
        "viewer" => vec!["viewer", "normal"],
        "normal" => vec!["normal"],
        _ => vec![],
    };

    if roles.contains(&required_role) {
        Ok(())
    } else {
        Err(AppError::Authorization(
            "Insufficient permissions".to_string(),
        ))
    }
}

fn error_response(status: StatusCode, message: &str) -> Response {
    #[derive(Serialize)]
    struct ErrorBody {
        error: String,
        message: String,
        category: String,
        severity: String,
        recoverable: bool,
        timestamp: String,
    }

    let category = match status.as_u16() {
        401 => "authentication",
        403 => "authorization",
        404 => "not_found",
        429 => "rate_limit",
        _ => "internal",
    };

    let severity = match status.as_u16() {
        401 | 403 => "medium",
        500..=599 => "critical",
        _ => "low",
    };

    let error_code = match status.as_u16() {
        400 => "BAD_REQUEST",
        401 => "UNAUTHORIZED",
        403 => "FORBIDDEN",
        404 => "NOT_FOUND",
        409 => "CONFLICT",
        429 => "RATE_LIMIT_EXCEEDED",
        500 => "INTERNAL_ERROR",
        _ => "UNKNOWN_ERROR",
    };

    let body = ErrorBody {
        error: error_code.to_string(),
        message: message.to_string(),
        category: category.to_string(),
        severity: severity.to_string(),
        recoverable: !matches!(status.as_u16(), 403 | 500..=599),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    (status, Json(body)).into_response()
}
