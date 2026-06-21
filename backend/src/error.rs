use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("External API error: {service} - {message}")]
    ExternalApi { service: String, message: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Websocket error: {0}")]
    WebSocket(String),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_code, message, category, severity, recoverable): (
            axum::http::StatusCode,
            &str,
            String,
            &str,
            &str,
            bool,
        ) = match &self {
            AppError::Authentication(msg) => (
                axum::http::StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                msg.clone(),
                "authentication",
                "medium",
                true,
            ),
            AppError::Authorization(msg) => (
                axum::http::StatusCode::FORBIDDEN,
                "FORBIDDEN",
                msg.clone(),
                "authorization",
                "medium",
                false,
            ),
            AppError::Validation(msg) => (
                axum::http::StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                msg.clone(),
                "validation",
                "low",
                true,
            ),
            AppError::NotFound(msg) => (
                axum::http::StatusCode::NOT_FOUND,
                "NOT_FOUND",
                msg.clone(),
                "not_found",
                "low",
                false,
            ),
            AppError::Conflict(msg) => (
                axum::http::StatusCode::CONFLICT,
                "CONFLICT",
                msg.clone(),
                "validation",
                "low",
                true,
            ),
            AppError::NotImplemented(msg) => (
                axum::http::StatusCode::NOT_IMPLEMENTED,
                "NOT_IMPLEMENTED",
                msg.clone(),
                "feature",
                "medium",
                false,
            ),

            AppError::RateLimitExceeded => (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMIT_EXCEEDED",
                "Rate limit exceeded".to_string(),
                "rate_limit",
                "low",
                true,
            ),
            AppError::Database(_) | AppError::Redis(_) | AppError::Internal(_) => {
                tracing::error!("Internal error: {:?}", self);
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An unexpected error occurred".to_string(),
                    "internal",
                    "critical",
                    false,
                )
            }
            AppError::ExternalApi {
                service: _,
                message,
            } => (
                axum::http::StatusCode::BAD_GATEWAY,
                "EXTERNAL_API_ERROR",
                message.clone(),
                "external_api",
                "high",
                true,
            ),
            _ => {
                tracing::error!("Unhandled error: {:?}", self);
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    self.to_string(),
                    "internal",
                    "critical",
                    false,
                )
            }
        };

        let body = serde_json::json!({
            "error": error_code,
            "message": message,
            "category": category,
            "severity": severity,
            "recoverable": recoverable,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        (status, axum::Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
