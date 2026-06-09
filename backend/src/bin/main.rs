use moneyrobert_rs::collector::MarketCollector;
use moneyrobert_rs::config::AppConfig;
use moneyrobert_rs::logging::init_logging;
use moneyrobert_rs::server::run_server;
use moneyrobert_rs::state::{AppState, get_proxy_config_from_db, initialize_database};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load configuration: {}", e);
        std::process::exit(1);
    });

    init_logging(config.server.debug);

    tracing::info!(
        app_name = "MoneyRobert",
        version = "1.0.0",
        environment = config.server.environment.as_str(),
        "Starting application"
    );

    let state = AppState::new(config.clone()).await?;

    initialize_database(&state.db_pool).await?;

    // Get proxy config from database for market collector
    let proxy_url = get_proxy_config_from_db(&state.db_pool).await;

    let collector = Arc::new(MarketCollector::new_with_proxy(
        state.db_pool.clone(),
        state.ws_manager.clone(),
        proxy_url,
    ));
    collector.start().await;

    run_server(state).await
}
