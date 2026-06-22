use axum::{http::HeaderMap, Router};
use sqlx::Row;
use std::net::SocketAddr;
use tokio::signal;
use tokio::time::{Duration, MissedTickBehavior};
use uuid::Uuid;

use crate::agents::{EvolutionEngine, MemoryManager};
use crate::auth::Claims;
use crate::error::{AppError, Result};
use crate::extractors::auth_middleware;
use crate::middleware::{rate_limit, request_logging};
use crate::routes::agent_simulation::spawn_simulation_loop;
use crate::routes::api_router;
use crate::routes::news::refresh_news;
use crate::state::AppState;

async fn ws_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Result<axum::response::Response> {
    let protocols = headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Authentication("WebSocket token required".to_string()))?;
    let token = protocols
        .split(',')
        .map(str::trim)
        .find(|value| *value != "bearer")
        .ok_or_else(|| AppError::Authentication("WebSocket token required".to_string()))?;
    let claims = Claims::from_token(token, &state.config.security.secret_key)?;
    if !claims.is_access_token() {
        return Err(AppError::Authentication(
            "Access token required".to_string(),
        ));
    }
    let user_id =
        sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE id = $1 AND is_active = true")
            .bind(claims.get_user_id())
            .fetch_optional(&state.db_pool)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| {
                AppError::Authentication("User is inactive or no longer exists".to_string())
            })?;

    Ok(ws
        .protocols(["bearer"])
        .on_upgrade(move |socket| async move {
            state
                .ws_manager
                .handle_connection(socket, Some(user_id))
                .await;
        }))
}

pub fn create_app(state: AppState) -> axum::Router {
    let health_router = crate::routes::health::router();
    let auth_public_router = crate::routes::auth::router();
    let auth_authenticated_router = crate::routes::auth::authenticated_router().layer(
        axum::middleware::from_fn_with_state(state.clone(), auth_middleware),
    );

    let api = api_router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth_middleware,
    ));

    let ws_router = Router::new().route("/stream", axum::routing::get(ws_handler));

    let app = Router::new()
        .nest("/api/v1/health", health_router)
        .nest(
            "/api/v1/auth",
            auth_public_router.merge(auth_authenticated_router),
        )
        .nest("/api/v1", api)
        .nest("/api/v1/ws", ws_router)
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            request_logging,
        ))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(
                    state
                        .config
                        .cors
                        .origins
                        .iter()
                        .filter_map(|o| o.parse().ok())
                        .collect::<Vec<_>>(),
                )
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::PATCH,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::ACCEPT,
                    axum::http::header::HeaderName::from_static("x-requested-with"),
                ])
                .allow_credentials(true),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http());

    app
}

/// Resume any simulation loops that were running before server restart
async fn resume_simulation_loops(state: &AppState) {
    let rows =
        match sqlx::query("SELECT id, user_id FROM ai_simulation_configs WHERE status = 'running'")
            .fetch_all(&state.db_pool)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to query running simulations for resume: {}", e);
                return;
            }
        };

    if rows.is_empty() {
        tracing::info!("[Resume] No running simulations to resume");
        return;
    }

    tracing::info!(
        "[Resume] Found {} running simulation(s), resuming loops...",
        rows.len()
    );

    for row in &rows {
        let config_id: Uuid = row.get("id");
        let user_id: i64 = row.get("user_id");
        tracing::info!(
            "[Resume] Resuming simulation loop for config {} (user {})",
            config_id,
            user_id
        );
        spawn_simulation_loop(config_id, user_id, state.clone());
    }
}

fn spawn_news_refresh_loop(state: AppState) {
    let interval_minutes = std::env::var("NEWS_FETCH_INTERVAL_MINUTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(15);

    if interval_minutes == 0 {
        tracing::info!("Automatic news refresh is disabled");
        return;
    }

    let interval_minutes = interval_minutes.clamp(1, 1_440);
    tracing::info!(
        "Automatic news refresh scheduled every {} minute(s)",
        interval_minutes
    );

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_minutes * 60));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            match refresh_news(&state).await {
                Ok(summary) => tracing::info!(
                    status = summary
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown"),
                    fetched = summary
                        .get("fetched")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0),
                    inserted = summary
                        .get("inserted")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0),
                    "Automatic news refresh completed"
                ),
                Err(error) => tracing::warn!("Automatic news refresh failed: {}", error),
            }
        }
    });
}

fn spawn_agent_reflection_loop(state: AppState) {
    let interval_hours = std::env::var("AGENT_REFLECTION_INTERVAL_HOURS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(24);

    if interval_hours == 0 {
        tracing::info!("Automatic agent reflection is disabled");
        return;
    }

    let interval_hours = interval_hours.clamp(1, 168);
    tracing::info!(
        "Automatic agent reflection scheduled every {} hour(s)",
        interval_hours
    );

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_hours * 3_600));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Consume the immediate first tick. Reflections need completed outcomes,
        // so running on every process restart would create duplicate lessons.
        interval.tick().await;

        loop {
            interval.tick().await;

            let memory = MemoryManager::new(state.db_pool.clone());
            if let Err(error) = memory.run_reflection_cycle().await {
                tracing::warn!("Automatic memory reflection failed: {}", error);
            }

            let evolution = EvolutionEngine::new(state.db_pool.clone());
            match evolution.run_daily_reflection().await {
                Ok(log) => tracing::info!(
                    reflection_id = %log.id,
                    status = %log.status,
                    "Automatic evolution reflection created for human review"
                ),
                Err(error) => tracing::warn!("Automatic evolution reflection failed: {}", error),
            }
        }
    });
}
pub async fn run_server(state: AppState) -> anyhow::Result<()> {
    // Resume any simulation loops that were running before restart
    resume_simulation_loops(&state).await;
    spawn_news_refresh_loop(state.clone());
    spawn_agent_reflection_loop(state.clone());

    let app = create_app(state.clone());

    let addr: SocketAddr =
        format!("{}:{}", state.config.server.host, state.config.server.port).parse()?;

    tracing::info!(
        "Starting server on {} in {} mode",
        addr,
        state.config.server.environment
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutting down server...");
}
