use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::Duration;
use sqlx::Row;
use validator::Validate;

use crate::auth::{hash_password, verify_password, Claims};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::schemas::{
    AuthResponse, LoginRequest, MessageResponse, RefreshRequest, RegisterRequest,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
}

pub fn authenticated_router() -> Router<AppState> {
    Router::new().route("/me", get(get_current_user))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<MessageResponse>> {
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let password_hash = hash_password(&req.password)?;

    let user = sqlx::query(
        r#"
        INSERT INTO users (username, email, hashed_password, role, is_active, created_at, updated_at)
        VALUES ($1, $2, $3, 'NORMAL', true, NOW(), NOW())
        RETURNING id, username, email, role::text as role
        "#,
    )
    .bind(req.username)
    .bind(req.email)
    .bind(password_hash)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            AppError::Conflict("Username or email already exists".to_string())
        }
        e => AppError::Database(e),
    })?;

    tracing::info!(user_id = user.get::<i64, _>("id"), "New user registered");

    Ok(Json(MessageResponse::new("User registered successfully")))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>> {
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let user = sqlx::query(
        r#"
        SELECT id, username, hashed_password, LOWER(role::text) as role
        FROM users
        WHERE username = $1 AND is_active = true
        "#,
    )
    .bind(req.username)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let user =
        user.ok_or_else(|| AppError::Authentication("Invalid username or password".to_string()))?;

    let valid = verify_password(&req.password, &user.get::<String, _>("hashed_password"))?;
    if !valid {
        return Err(AppError::Authentication(
            "Invalid username or password".to_string(),
        ));
    }

    let access_claims = Claims::new(
        user.get::<i64, _>("id"),
        user.get::<String, _>("username"),
        user.get::<String, _>("role"),
        Duration::minutes(state.config.security.access_token_expire_minutes),
    );

    let refresh_claims = Claims::new_refresh(
        user.get::<i64, _>("id"),
        user.get::<String, _>("username"),
        user.get::<String, _>("role"),
        Duration::days(state.config.security.refresh_token_expire_days),
    );

    let access_token = access_claims.generate_token(&state.config.security.secret_key)?;
    let refresh_token = refresh_claims.generate_token(&state.config.security.secret_key)?;

    tracing::info!(user_id = user.get::<i64, _>("id"), "User logged in");

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        token_type: "bearer".to_string(),
        expires_in: state.config.security.access_token_expire_minutes * 60,
    }))
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>> {
    let claims = Claims::from_token(&req.refresh_token, &state.config.security.secret_key)?;
    if !claims.is_refresh_token() {
        return Err(AppError::Authentication(
            "Refresh token required".to_string(),
        ));
    }

    let user = sqlx::query(
        "SELECT id, username, LOWER(role::text) AS role FROM users WHERE id = $1 AND is_active = true",
    )
    .bind(claims.get_user_id())
    .fetch_optional(&state.db_pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::Authentication("User is inactive or no longer exists".to_string()))?;

    let new_claims = Claims::new(
        user.get::<i64, _>("id"),
        user.get::<String, _>("username"),
        user.get::<String, _>("role"),
        Duration::minutes(state.config.security.access_token_expire_minutes),
    );

    let access_token = new_claims.generate_token(&state.config.security.secret_key)?;
    let refresh_token = Claims::new_refresh(
        user.get::<i64, _>("id"),
        user.get::<String, _>("username"),
        user.get::<String, _>("role"),
        Duration::days(state.config.security.refresh_token_expire_days),
    )
    .generate_token(&state.config.security.secret_key)?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        token_type: "bearer".to_string(),
        expires_in: state.config.security.access_token_expire_minutes * 60,
    }))
}

async fn get_current_user(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let user_data = sqlx::query(
        r#"
        SELECT id, username, email, LOWER(role::text) as role, is_active,
               created_at::text as created_at, updated_at::text as updated_at
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(user.user_id as i64)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "id": user_data.get::<i64, _>("id"),
        "username": user_data.get::<String, _>("username"),
        "email": user_data.get::<String, _>("email"),
        "role": user_data.get::<String, _>("role"),
        "is_active": user_data.get::<bool, _>("is_active"),
        "created_at": user_data.try_get::<String, _>("created_at").unwrap_or_default(),
        "updated_at": user_data.try_get::<String, _>("updated_at").unwrap_or_default(),
    })))
}
