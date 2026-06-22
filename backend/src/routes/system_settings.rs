use axum::{
    extract::{Path, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::agents::DecisionTuningConfig;
use crate::error::{AppError, Result};
use crate::extractors::{require_role, CurrentUser};
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
        .route(
            "/decision-tuning",
            get(get_decision_tuning).put(update_decision_tuning),
        )
        .route("/all", get(get_all_settings))
        .route("/{key}", get(get_setting).put(update_setting))
}

// ===== Handlers =====

async fn get_decision_tuning(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<DecisionTuningConfig>> {
    require_role(user, "admin").await?;
    Ok(Json(DecisionTuningConfig::load(&state.db_pool).await))
}

async fn update_decision_tuning(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(config): Json<DecisionTuningConfig>,
) -> Result<Json<DecisionTuningConfig>> {
    require_role(user, "admin").await?;
    config.validate().map_err(AppError::Validation)?;

    let serialized = serde_json::to_string(&config)
        .map_err(|error| AppError::Internal(format!("序列化决策调优配置失败: {}", error)))?;
    let parameters = serde_json::to_value(&config)
        .map_err(|error| AppError::Internal(format!("序列化策略版本失败: {}", error)))?;

    let mut tx = state.db_pool.begin().await.map_err(AppError::Database)?;
    sqlx::query(
        r#"INSERT INTO system_settings
           (key, value, value_type, category, description, updated_at, updated_by)
           VALUES ('decision_tuning', $1, 'json', 'ai', 'AI辩论决策手动调优参数', NOW(), 'admin')
           ON CONFLICT (key) DO UPDATE
           SET value = EXCLUDED.value, value_type = 'json', category = 'ai',
               description = EXCLUDED.description, updated_at = NOW(), updated_by = 'admin'"#,
    )
    .bind(&serialized)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let next_version: i32 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(version_number), 0) + 1
           FROM strategy_versions WHERE name = 'decision_tuning'"#,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        "UPDATE strategy_versions SET status = 'deprecated' WHERE name = 'decision_tuning' AND status = 'active'",
    )
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        r#"INSERT INTO strategy_versions
           (id, name, version_number, strategy_type, parameters, risk_params,
            description, change_reason, status, created_by, activated_at)
           VALUES (gen_random_uuid(), 'decision_tuning', $1, 'decision_policy', $2,
                   $3, '手动决策调优配置', '管理员在决策调优台保存', 'active', 'admin', NOW())"#,
    )
    .bind(next_version)
    .bind(&parameters)
    .bind(serde_json::json!({
        "minimum_data_quality": config.minimum_data_quality,
        "minimum_edge_floor": config.minimum_edge_floor,
        "minimum_edge_ceiling": config.minimum_edge_ceiling,
        "conflict_policy": config.conflict_policy,
    }))
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;
    Ok(Json(config))
}

/// GET /system/proxy - 获取代理配置
async fn get_proxy_config(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<ProxyConfig>> {
    require_role(_user, "admin").await?;
    let rows = sqlx::query("SELECT key, value FROM system_settings WHERE category = 'proxy'")
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
    user: CurrentUser,
    Json(input): Json<UpdateProxyConfig>,
) -> Result<Json<ProxyConfig>> {
    require_role(user.clone(), "admin").await?;
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
    get_proxy_config(State(state), user).await
}

/// PUT /system/proxy/test - 测试代理连接
async fn test_proxy(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<ProxyTestResult>> {
    // Get current proxy config from DB
    require_role(_user, "admin").await?;
    let proxy_config = {
        let rows = sqlx::query("SELECT key, value FROM system_settings WHERE category = 'proxy'")
            .fetch_all(&state.db_pool)
            .await
            .map_err(|e| AppError::Database(e))?;

        let mut enabled = false;
        let mut url = String::new();
        let mut proxy_type = "socks5".to_string();
        let mut test_url = "https://www.okx.com".to_string();

        for row in &rows {
            let key: String = row.get("key");
            let value: String = row.get("value");
            match key.as_str() {
                "proxy_enabled" => enabled = value == "true",
                "proxy_url" => url = value,
                "proxy_type" => proxy_type = value,
                "proxy_test_url" => test_url = value,
                _ => {}
            }
        }

        (enabled, url, proxy_type, test_url)
    };

    let (enabled, proxy_url, proxy_type, test_url) = proxy_config;

    // Build HTTP client with or without proxy
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10));

    if enabled && !proxy_url.is_empty() {
        // Normalize URL based on proxy_type
        let normalized = crate::state::normalize_proxy_url(&proxy_url, &proxy_type);
        match reqwest::Proxy::all(&normalized) {
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
    require_role(_user, "admin").await?;
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
    require_role(_user, "admin").await?;
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
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<SystemSetting>> {
    require_role(user.clone(), "admin").await?;
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

    get_setting(State(state), user, Path(key)).await
}
