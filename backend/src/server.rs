use axum::Router;
use std::net::SocketAddr;
use tokio::signal;

use crate::extractors::auth_middleware;
use crate::middleware::{rate_limit, request_logging};
use crate::routes::api_router;
use crate::state::AppState;

async fn ws_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| async move {
        state.ws_manager.handle_connection(socket, None).await;
    })
}

pub fn create_app(state: AppState) -> axum::Router {
    let health_router = crate::routes::health::router();
    let auth_public_router = crate::routes::auth::router();
    let auth_authenticated_router = crate::routes::auth::authenticated_router()
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let api = api_router()
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let ws_router = Router::new()
        .route("/stream", axum::routing::get(ws_handler));

    let app = Router::new()
        .nest("/api/v1/health", health_router)
        .nest("/api/v1/auth", auth_public_router.merge(auth_authenticated_router))
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

pub async fn run_server(state: AppState) -> anyhow::Result<()> {
    let app = create_app(state.clone());

    let addr: SocketAddr = format!(
        "{}:{}",
        state.config.server.host, state.config.server.port
    )
    .parse()?;

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
