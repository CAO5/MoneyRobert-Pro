use redis::aio::ConnectionManager;
use sqlx::{PgPool, Row};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::error::{AppError, Result};
use crate::exchanges::okx::OkxClient;
use crate::middleware::RateLimitEntry;
use crate::utils::encryption::decrypt;
use crate::websocket::WebSocketManager;
use dashmap::DashMap;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub db_pool: PgPool,
    pub redis: Option<ConnectionManager>,
    pub ws_manager: Arc<WebSocketManager>,
    pub rate_limit_map: Arc<DashMap<String, RateLimitEntry>>,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self> {
        let db_pool = create_db_pool(&config.database.url, config.database.pool_size).await?;

        let redis = match create_redis_client(&config.redis.url).await {
            Ok(conn) => Some(conn),
            Err(e) => {
                tracing::warn!("Redis connection failed, will run without Redis: {}", e);
                None
            }
        };

        Ok(Self {
            config,
            db_pool,
            redis,
            ws_manager: Arc::new(WebSocketManager::new()),
            rate_limit_map: Arc::new(DashMap::new()),
        })
    }

    pub async fn get_user_okx_client(&self, user_id: i64) -> Result<Arc<OkxClient>> {
        let row = sqlx::query(
            r#"SELECT key, secret, passphrase, metadata 
               FROM api_keys 
               WHERE user_id = $1 AND key_type = 'exchange' AND is_active = true 
               ORDER BY created_at DESC 
               LIMIT 1"#,
        )
        .bind(user_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AppError::Database(e))?;

        let row = row.ok_or_else(|| {
            AppError::Validation("No active OKX API key found".to_string())
        })?;

        let encrypted_key = row.try_get::<String, _>("key").unwrap_or_default();
        let encrypted_secret = row.try_get::<String, _>("secret").unwrap_or_default();
        let encrypted_passphrase = row.try_get::<String, _>("passphrase").unwrap_or_default();

        let api_key = decrypt(&encrypted_key)?;
        let api_secret = decrypt(&encrypted_secret)?;
        let passphrase = decrypt(&encrypted_passphrase).unwrap_or_default();

        let metadata: serde_json::Value = row
            .try_get::<serde_json::Value, _>("metadata")
            .unwrap_or(serde_json::json!({}));
        let is_demo = metadata.get("is_demo").and_then(|v| v.as_bool()).unwrap_or(true);

        // Read proxy config from system_settings
        let proxy_url = get_proxy_config_from_db(&self.db_pool).await;

        Ok(Arc::new(OkxClient::new_with_proxy(api_key, api_secret, passphrase, is_demo, proxy_url)))
    }
}

pub async fn create_db_pool(database_url: &str, pool_size: u32) -> Result<PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(pool_size)
        .connect(database_url)
        .await
        .map_err(|e| AppError::Database(e))?;

    tracing::info!("Database pool created with max_connections={}", pool_size);

    Ok(pool)
}

pub async fn create_redis_client(redis_url: &str) -> Result<ConnectionManager> {
    let client = redis::Client::open(redis_url)
        .map_err(|e| AppError::Redis(e))?;

    let manager = ConnectionManager::new(client)
        .await
        .map_err(|e| AppError::Redis(e))?;

    tracing::info!("Redis connection established");

    Ok(manager)
}

pub async fn initialize_database(pool: &PgPool) -> Result<()> {
    tracing::info!("Running database migrations...");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        )"
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    tracing::info!("Database migrations completed successfully");

    Ok(())
}

pub async fn get_proxy_config_from_db(pool: &PgPool) -> Option<String> {
    let rows = sqlx::query(
        "SELECT key, value FROM system_settings WHERE category = 'proxy'"
    )
    .fetch_all(pool)
    .await
    .ok()?;

    let mut enabled = false;
    let mut url = String::new();
    let mut proxy_type = "socks5".to_string();

    for row in &rows {
        let key: String = row.try_get("key").unwrap_or_default();
        let value: String = row.try_get("value").unwrap_or_default();
        match key.as_str() {
            "proxy_enabled" => enabled = value == "true",
            "proxy_url" => url = value,
            "proxy_type" => proxy_type = value,
            _ => {}
        }
    }

    if enabled && !url.is_empty() {
        // Normalize proxy URL based on selected proxy_type
        let normalized = normalize_proxy_url(&url, &proxy_type);
        Some(normalized)
    } else {
        None
    }
}

/// Normalize proxy URL to match the selected proxy_type.
/// If the URL already contains a valid scheme, trust it; otherwise apply proxy_type.
pub fn normalize_proxy_url(url: &str, proxy_type: &str) -> String {
    let url = url.trim();

    // If URL already has a valid scheme, trust it but fix known issues
    if url.starts_with("socks5://") || url.starts_with("socks5h://") {
        return url.replace("socks5h://", "socks5://");
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        // https:// is not a valid proxy scheme for reqwest, convert to http://
        return url.replace("https://", "http://");
    }

    // No scheme in URL — apply based on proxy_type
    match proxy_type {
        "socks5" => format!("socks5://{}", url),
        _ => format!("http://{}", url), // http and default
    }
}
