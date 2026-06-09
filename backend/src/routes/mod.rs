pub mod admin;
pub mod ai_analysis;
pub mod ai_chat;
pub mod ai_predictions;
pub mod ai_providers;
pub mod agent_simulation;
pub mod api_keys;
pub mod auth;
pub mod auto_trading;
pub mod billing;
pub mod dashboard;
pub mod health;
pub mod market_data;
pub mod news;
pub mod notifications;
pub mod paper_trading;
pub mod reports;
pub mod sentiment_data;
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
        .nest("/ai", ai_analysis::router())
        .nest("/chat", ai_chat::router())
        .nest("/ai/prediction", ai_predictions::router())
        .nest("/ai/providers", ai_providers::router())
        .nest("/auto-trading", auto_trading::router())
        .nest("/api-keys", api_keys::router())
        .nest("/papers", paper_trading::router())
        .nest("/validation", validation::router())
        .nest("/tasks", tasks::router())
        .nest("/agent", agent_simulation::router())
        .nest("/system", system_settings::router())
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "MoneyRobert API",
        "version": "1.0.0",
        "status": "running"
    }))
}
