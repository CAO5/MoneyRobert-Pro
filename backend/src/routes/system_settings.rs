use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

// ===== Models =====

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct SystemSetting {
    pub key: String,
    pub value: String,
    pub value_type: String,
    pub category: String,
    pub description: Option<String>,
    pub updated_at: chrono::NaiveDateTime,
    pub updated_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub url: String,
    pub proxy_type: String,
    pub test_url: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProxyConfig {
    pub enabled: Option<bool>,
    pub url: Option<String>,
    pub proxy_type: Option<String>,
    pub test_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProxyTestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

// ===== Router =====

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proxy", get(get_proxy_config).put(update_proxy_config))
        .route("/proxy/test", put(test_proxy))
        .route("/all", get(get_all_settings))
        .route("/{key}", get(get_setting).put(update_setting))
}

// ===== Handlers =====

/// GET /system/proxy - 获取代理配置
async fn get_proxy_config(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<ProxyConfig>> {
    let rows = sqlx::query(
        "SELECT key, value FROM system_settings WHERE category = 'proxy'",
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let mut config = ProxyConfig {
        enabled: false,
        url: String::new(),
        proxy_type: "socks5".to_string(),
        test_url: "https://www.okx.com".to_string(),
    };

    for row in &rows {
        let key: String = row.get("key");
        let value: String = row.get("value");
        match key.as_str() {
            "proxy_enabled" => config.enabled = value == "true",
            "proxy_url" => config.url = value,
            "proxy_type" => config.proxy_type = value,
            "proxy_test_url" => config.test_url = value,
            _ => {}
        }
    }

    Ok(Json(config))
}

/// PUT /system/proxy - 更新代理配置
async fn update_proxy_config(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(input): Json<UpdateProxyConfig>,
) -> Result<Json<ProxyConfig>> {
    // Update each field if provided
    if let Some(enabled) = input.enabled {
        let _ = sqlx::query(
            "INSERT INTO system_settings (key, value, value_type, category, description, updated_at) \
             VALUES ('proxy_enabled', $1, 'boolean', 'proxy', '是否启用代理', NOW()) \
             ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
        )
        .bind(if enabled { "true" } else { "false" })
        .execute(&state.db_pool)
        .await;
    }

    if let Some(url) = &input.url {
        let _ = sqlx::query(
            "INSERT INTO system_settings (key, value, value_type, category, description, updated_at) \
             VALUES ('proxy_url', $1, 'string', 'proxy', '代理地址', NOW()) \
             ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
        )
        .bind(url)
        .execute(&state.db_pool)
        .await;
    }

    if let Some(proxy_type) = &input.proxy_type {
        let _ = sqlx::query(
            "INSERT INTO system_settings (key, value, value_type, category, description, updated_at) \
             VALUES ('proxy_type', $1, 'string', 'proxy', '代理类型', NOW()) \
             ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
        )
        .bind(proxy_type)
        .execute(&state.db_pool)
        .await;
    }

    if let Some(test_url) = &input.test_url {
        let _ = sqlx::query(
            "INSERT INTO system_settings (key, value, value_type, category, description, updated_at) \
             VALUES ('proxy_test_url', $1, 'string', 'proxy', '代理测试URL', NOW()) \
             ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
        )
        .bind(test_url)
        .execute(&state.db_pool)
        .await;
    }

    // Return updated config
    get_proxy_config(State(state), _user).await
}

/// PUT /system/proxy/test - 测试代理连接
async fn test_proxy(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<ProxyTestResult>> {
    // Get current proxy config from DB
    let proxy_config = {
        let rows = sqlx::query(
            "SELECT key, value FROM system_settings WHERE category = 'proxy'",
        )
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

        let mut enabled = false;
        let mut url = String::new();
        let mut test_url = "https://www.okx.com".to_string();

        for row in &rows {
            let key: String = row.get("key");
            let value: String = row.get("value");
            match key.as_str() {
                "proxy_enabled" => enabled = value == "true",
                "proxy_url" => url = value,
                "proxy_test_url" => test_url = value,
                _ => {}
            }
        }

        (enabled, url, test_url)
    };

    let (enabled, proxy_url, test_url) = proxy_config;

    // Build HTTP client with or without proxy
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10));

    if enabled && !proxy_url.is_empty() {
        let proxy_url = proxy_url.replace("socks5h://", "socks5://");
        match reqwest::Proxy::all(&proxy_url) {
            Ok(proxy) => {
                builder = builder.proxy(proxy);
            }
            Err(e) => {
                return Ok(Json(ProxyTestResult {
                    success: false,
                    message: format!("代理配置无效: {}", e),
                    latency_ms: None,
                }));
            }
        }
    }

    let client = builder
        .build()
        .map_err(|e| AppError::Internal(format!("创建HTTP客户端失败: {}", e)))?;

    let start = std::time::Instant::now();
    let result = client.get(&test_url).send().await;
    let latency = start.elapsed().as_millis() as u64;

    match result {
        Ok(resp) => {
            let status = resp.status();
            Ok(Json(ProxyTestResult {
                success: status.is_success(),
                message: if status.is_success() {
                    format!("连接成功 (HTTP {})", status)
                } else {
                    format!("连接失败 (HTTP {})", status)
                },
                latency_ms: Some(latency),
            }))
        }
        Err(e) => Ok(Json(ProxyTestResult {
            success: false,
            message: format!("连接失败: {}", e),
            latency_ms: Some(latency),
        })),
    }
}

/// GET /system/all - 获取所有系统设置
async fn get_all_settings(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<Vec<SystemSetting>>> {
    let settings = sqlx::query_as::<_, SystemSetting>(
        "SELECT key, value, value_type, category, description, updated_at, updated_by FROM system_settings ORDER BY category, key",
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(settings))
}

/// GET /system/{key} - 获取单个设置
async fn get_setting(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(key): Path<String>,
) -> Result<Json<SystemSetting>> {
    let setting = sqlx::query_as::<_, SystemSetting>(
        "SELECT key, value, value_type, category, description, updated_at, updated_by FROM system_settings WHERE key = $1",
    )
    .bind(&key)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound(format!("Setting '{}' not found", key)))?;

    Ok(Json(setting))
}

/// PUT /system/{key} - 更新单个设置
async fn update_setting(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<SystemSetting>> {
    let value = body
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("Missing 'value' field".to_string()))?;

    sqlx::query(
        "INSERT INTO system_settings (key, value, value_type, category, description, updated_at) \
         VALUES ($1, $2, 'string', 'general', '', NOW()) \
         ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()",
    )
    .bind(&key)
    .bind(value)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    get_setting(State(state), _user, Path(key)).await
}
