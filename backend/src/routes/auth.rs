use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::Duration;
use redis::AsyncCommands;
use sqlx::Row;
use validator::Validate;

use crate::auth::{hash_password, verify_password, Claims};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::schemas::{
    AuthResponse, LoginRequest, MessageResponse, PasswordPolicyResponse, PasswordResetConfirm,
    PasswordResetRequest, RefreshRequest, RegisterRequest,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
        // 公开接口：下发当前密码加密策略，客户端登录前必须拉取
        .route("/password-policy", get(password_policy))
        // 公开接口：密码重置流程（无需登录态）
        .route("/password-reset/request", post(request_password_reset))
        .route("/password-reset/confirm", post(confirm_password_reset))
}

pub fn authenticated_router() -> Router<AppState> {
    Router::new().route("/me", get(get_current_user))
}

/// 校验客户端传来的密码字段
/// 安全策略：客户端必须先做 SHA-256，发送 64 字符的 hex 字符串
/// 后端将 hex 作为 bcrypt 输入存储/验证，绝不接受明文密码
/// 数据库存储的是 bcrypt(sha256_hex)
fn resolve_password(raw: &str) -> Result<String> {
    // 客户端必须发送 64 字符的 hex（sha256 输出）
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Validation(
            "Password must be sha256 hex string (64 hex chars). Plaintext password is not accepted.".to_string(),
        ));
    }
    Ok(raw.to_lowercase())
}

/// 公开接口：下发当前密码加密策略
/// 客户端登录/注册前必须先 GET /auth/password-policy 拉取此配置
async fn password_policy(
    State(state): State<AppState>,
) -> Result<Json<PasswordPolicyResponse>> {
    Ok(Json(PasswordPolicyResponse {
        // 强制 sha256，不再支持明文密码
        method: "sha256".to_string(),
    }))
}

/// 生成 6 位数字验证码
fn generate_reset_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1000000))
}

/// 请求密码重置：生成 6 位验证码，存 Redis（10 分钟过期）
/// 生产环境应通过邮件/短信发送；开发环境日志输出便于调试
async fn request_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetRequest>,
) -> Result<Json<MessageResponse>> {
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // 检查邮箱是否存在
    let user_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1 AND is_active = true)",
    )
    .bind(&req.email)
    .fetch_one(&state.db_pool)
    .await
    .map_err(AppError::Database)?;

    // 防枚举：无论邮箱是否存在，都返回相同成功消息（避免探测有效邮箱）
    // 只在用户存在时生成验证码并发送
    if user_exists {
        let code = generate_reset_code();
        let redis_key = format!("password_reset:{}", req.email);

        if let Some(redis) = state.redis.as_ref() {
            // 存 Redis，10 分钟过期
            let _: () = redis
                .clone()
                .set_ex(redis_key.clone(), code.clone(), 600u64)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to store reset code: {}", e)))?;
        } else {
            return Err(AppError::Internal(
                "Redis not configured. Password reset is unavailable.".to_string(),
            ));
        }

        // TODO: 生产环境对接邮件/短信服务发送验证码
        // 开发环境：通过日志输出验证码便于测试
        tracing::info!(email = %req.email, code = %code, "Password reset code generated");
    }

    Ok(Json(MessageResponse::new(
        "If the email exists, a reset code has been sent",
    )))
}

/// 确认密码重置：校验验证码 + 设置新密码
async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirm>,
) -> Result<Json<MessageResponse>> {
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // 强制 sha256：客户端必须发送 64 字符 hex
    let password = resolve_password(&req.new_password)?;

    let redis_key = format!("password_reset:{}", req.email);

    let redis = state.redis.as_ref().ok_or_else(|| {
        AppError::Internal("Redis not configured. Password reset is unavailable.".to_string())
    })?;

    // 从 Redis 读取并删除验证码（一次性使用）
    let stored_code: Option<String> = redis
        .clone()
        .get(redis_key.clone())
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read reset code: {}", e)))?;

    if let Some(code) = stored_code {
        let _: () = redis
            .clone()
            .del(&redis_key)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to delete reset code: {}", e)))?;

        if code != req.code {
            return Err(AppError::Authentication("Invalid or expired reset code".to_string()));
        }
    } else {
        return Err(AppError::Authentication("Invalid or expired reset code".to_string()));
    }

    // 重置密码：bcrypt(sha256_hex)
    let password_hash = hash_password(&password)?;

    let affected = sqlx::query(
        "UPDATE users SET hashed_password = $1, updated_at = NOW() WHERE email = $2 AND is_active = true",
    )
    .bind(&password_hash)
    .bind(&req.email)
    .execute(&state.db_pool)
    .await
    .map_err(AppError::Database)?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("User not found or inactive".to_string()));
    }

    tracing::info!(email = %req.email, "Password reset completed");

    Ok(Json(MessageResponse::new(
        "Password has been reset successfully",
    )))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<MessageResponse>> {
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // 强制 sha256：客户端必须发送 sha256(password) 的 hex 字符串
    let password = resolve_password(&req.password)?;
    let password_hash = hash_password(&password)?;

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

    // 强制 sha256：客户端必须发送 sha256(password) 的 hex 字符串
    let password = resolve_password(&req.password)?;
    let stored_hash: String = user.get::<String, _>("hashed_password");
    // 旧用户密码已清空（迁移脚本）：必须走重置流程
    if stored_hash.is_empty() {
        return Err(AppError::Authentication(
            "Password reset required. Please use forgot password to set a new password.".to_string(),
        ));
    }
    let valid = verify_password(&password, &stored_hash)?;
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
