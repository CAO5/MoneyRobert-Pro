mod factor;
mod market;
mod paper;

use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};

use crate::{collector::MarketCollector, error::AppError, state::AppState};

const FACTOR_VERSION: &str = "LLF v0.2";
const FORMULA_STATUS: &str = "confirmed";

fn collector_enabled() -> bool {
    std::env::var("LLF_COLLECTOR_ENABLED").map_or(true, |value| value.eq_ignore_ascii_case("true"))
}

pub fn start(state: &AppState, collector: Arc<MarketCollector>) {
    if collector_enabled() {
        tokio::spawn(market::run(
            collector,
            state.db_pool.clone(),
            state.redis.clone(),
        ));
        tokio::spawn(factor::run(state.db_pool.clone()));
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/status", post(status))
}

async fn status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let (assets, paper) = tokio::join!(
        factor::dashboard(&state.db_pool),
        paper::dashboard(&state.db_pool),
    );
    let paper =
        paper.map_err(|error| AppError::Internal(format!("LLF paper status: {error:#}")))?;
    Ok(Json(json!({
        "factor_version": FACTOR_VERSION,
        "formula_ready": true,
        "spec_id": "LLF-OKX-3ASSET-1M-001",
        "formula_status": FORMULA_STATUS,
        "collector_enabled": collector_enabled(),
        "as_of": chrono::Utc::now(),
        "assets": assets?,
        "paper": paper,
    })))
}
