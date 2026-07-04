use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 50))]
    pub username: String,
    /// 密码（sha256 hex 字符串，64 字符，客户端必须先做 SHA-256）
    #[validate(length(min = 64, max = 64))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 50))]
    pub username: String,
    #[validate(email)]
    pub email: String,
    /// 密码（sha256 hex 字符串，64 字符，客户端必须先做 SHA-256）
    #[validate(length(min = 64, max = 64))]
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// 密码加密策略响应
/// 客户端在登录/注册前通过 GET /auth/password-policy 拉取此配置
/// 根据返回的 method 决定如何处理用户输入的密码
#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordPolicyResponse {
    /// 加密方式：sha256（客户端必须发送 sha256(password) 的 hex 字符串）
    pub method: String,
}

/// 密码重置请求（通过邮箱验证）
/// 客户端流程：
/// 1. POST /auth/password-reset/request { email } → 后端生成 6 位验证码，发到邮箱
/// 2. POST /auth/password-reset/confirm { email, code, new_password } → 后端验证码校验通过后重置密码
#[derive(Debug, Deserialize, Validate)]
pub struct PasswordResetRequest {
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PasswordResetConfirm {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 6, max = 6))]
    pub code: String,
    /// 新密码（sha256 hex 字符串，64 字符）
    #[validate(length(min = 64, max = 64))]
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageResponse {
    #[serde(default = "default_true_val")]
    pub success: bool,
    pub message: String,
    pub timestamp: String,
}

impl MessageResponse {
    pub fn new(message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            timestamp: Utc::now().naive_utc().format("%Y-%m-%dT%H:%M:%S%.6f").to_string(),
        }
    }
}

fn default_true_val() -> bool {
    true
}

pub fn success_response(message: &str, data: serde_json::Value) -> serde_json::Value {
    let timestamp = Utc::now().naive_utc().format("%Y-%m-%dT%H:%M:%S%.6f").to_string();
    serde_json::json!({
        "success": true,
        "message": message,
        "data": data,
        "timestamp": timestamp
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

impl<T> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, total: i64, page: u32, page_size: u32) -> Self {
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
        Self {
            items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}
