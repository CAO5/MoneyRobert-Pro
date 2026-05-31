use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::error::{AppError, Result};
use crate::middleware::RateLimitEntry;
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
