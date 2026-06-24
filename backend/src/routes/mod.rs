pub mod admin;
pub mod ai_analysis;
pub mod ai_chat;
pub mod ai_predictions;
pub mod ai_providers;
pub mod agent_simulation;
pub mod agent_analysis_api;
pub mod api_keys;
pub mod auth;
pub mod auto_trading;
pub mod backfill_api;
pub mod backtest_api;
pub mod billing;
pub mod dashboard;
pub mod data_quality_api;
pub mod evolution_api;
pub mod features_api;
pub mod health;
pub mod market_data;
pub mod memory_api;
pub mod microstructure_api;
pub mod news;
pub mod notifications;
pub mod paper_trading;
pub mod reports;
pub mod sentiment_data;
pub mod signals_api;
pub mod strategies;
pub mod system_settings;
pub mod tasks;
pub mod trading;
pub mod validation;

use axum::{routing::get, Router};

use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .nest("/market", market_data::router())
        .nest("/trading", trading::router())
        .nest("/strategies", strategies::router())
        .nest("/dashboard", dashboard::router())
        .nest("/news", news::router())
        .nest("/sentiment", sentiment_data::router())
        .nest("/notifications", notifications::router())
        .nest("/reports", reports::router())
        .nest("/admin", admin::router())
        .nest("/billing", billing::router())
        .nest("/ai/providers", ai_providers::router())
        .nest("/ai/prediction", ai_predictions::router())
        .nest("/ai", ai_analysis::router())
        .nest("/chat", ai_chat::router())
        .nest("/auto-trading", auto_trading::router())
        .nest("/api-keys", api_keys::router())
        .nest("/papers", paper_trading::router())
        .nest("/validation", validation::router())
        .nest("/tasks", tasks::router())
        // Agent system routes (Chapter 5, 12, 13, 14)
        .nest("/agent", agent_simulation::router())
        .nest("/agent/analyze", agent_analysis_api::router())
        .nest("/agent/memory", memory_api::router())
        .nest("/agent/evolution", evolution_api::router())
        .nest("/system", system_settings::router())
        .nest("/backtest", backtest_api::router())
        .nest("/features", features_api::router())
        .nest("/signals", signals_api::router())
        .nest("/microstructure", microstructure_api::router())
        .nest("/data-quality", data_quality_api::router())
        .nest("/backfill", backfill_api::router())
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "MoneyRobert API",
        "version": "1.0.0",
        "status": "running"
    }))
}
