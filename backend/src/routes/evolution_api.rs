// Self-Evolution System API Routes
// Implements: AGENT_SYSTEM_DESIGN.md Chapter 13 - Self-evolving Fund Manager API

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agents::evolution::EvolutionEngine;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EvolutionQuery {
    pub agent_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePromptVersionRequest {
    pub agent_id: String,
    pub prompt_text: String,
    pub description: Option<String>,
    pub change_reason: Option<String>,
    pub parent_version_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CreateStrategyVersionRequest {
    pub name: String,
    pub strategy_type: String,
    pub parameters: serde_json::Value,
    pub risk_params: Option<serde_json::Value>,
    pub description: Option<String>,
    pub change_reason: Option<String>,
    pub parent_version_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct TriggerReflectionRequest {
    pub reflection_type: String, // daily_morning, weekly_review, monthly_architecture, triggered
    pub trigger: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EvolutionResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        // Prompt Versions
        .route("/prompts", get(list_prompts).post(create_prompt))
        .route("/prompts/{id}", get(get_prompt))
        .route("/prompts/{id}/approve", post(approve_prompt))
        .route("/prompts/{id}/activate", post(activate_prompt))
        .route("/prompts/{id}/rollback", post(rollback_prompt))
        .route("/prompts/agent/{agent_id}", get(list_agent_prompts))
        // Strategy Versions
        .route("/strategies", get(list_strategies).post(create_strategy))
        .route("/strategies/{id}", get(get_strategy))
        .route("/strategies/{id}/activate", post(activate_strategy))
        // Reflection Logs
        .route("/reflections", get(list_reflections).post(trigger_reflection))
        .route("/reflections/{id}/effectiveness", post(update_reflection_effectiveness))
        // Evolution Logs
        .route("/logs", get(list_evolution_logs))
        // Stats
        .route("/stats", get(get_evolution_stats))
        // Safety
        .route("/check", get(check_evolution_allowed))
}

/// GET /api/v1/agent/evolution/prompts - List all prompt versions
async fn list_prompts(
    State(state): State<AppState>,
    Query(params): Query<EvolutionQuery>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);

    // List across all agents - we'll get recent ones
    match engine.evolution_log_store().list(limit).await {
        Ok(logs) => Json(EvolutionResponse {
            success: true,
            data: Some(logs),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/prompts/agent/{agent_id} - List prompt versions for an agent
async fn list_agent_prompts(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<EvolutionQuery>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);

    match engine.prompt_store().list_by_agent(&agent_id, limit).await {
        Ok(prompts) => Json(EvolutionResponse {
            success: true,
            data: Some(prompts),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/prompts - Create a new prompt version
async fn create_prompt(
    State(state): State<AppState>,
    Json(req): Json<CreatePromptVersionRequest>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    match engine
        .prompt_store()
        .create(
            &req.agent_id,
            &req.prompt_text,
            req.description.as_deref(),
            req.change_reason.as_deref(),
            req.parent_version_id,
        )
        .await
    {
        Ok(prompt) => Json(EvolutionResponse {
            success: true,
            data: Some(prompt),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/prompts/{id} - Get a specific prompt version
async fn get_prompt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    match engine.prompt_store().get(id).await {
        Ok(Some(prompt)) => Json(EvolutionResponse {
            success: true,
            data: Some(prompt),
            error: None,
        }),
        Ok(None) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some("Prompt version not found".to_string()),
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/prompts/{id}/approve - Approve a prompt version
async fn approve_prompt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    match engine.prompt_store().approve(id, "admin").await {
        Ok(_) => Json(EvolutionResponse {
            success: true,
            data: Some(serde_json::json!({"approved": true})),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/prompts/{id}/activate - Activate a prompt version
async fn activate_prompt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());

    // Check evolution rate limit
    match engine.check_evolution_allowed().await {
        Ok(false) => {
            return Json(EvolutionResponse {
                success: false,
                data: None,
                error: Some("Evolution rate limit reached. Try again tomorrow.".to_string()),
            })
        }
        Ok(true) => {}
        Err(e) => {
            return Json(EvolutionResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            })
        }
    }

    match engine.prompt_store().activate(id).await {
        Ok(_) => Json(EvolutionResponse {
            success: true,
            data: Some(serde_json::json!({"activated": true})),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/prompts/{id}/rollback - Rollback a prompt version
async fn rollback_prompt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let reason = req
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("Manual rollback");

    match engine.prompt_store().rollback(id, reason).await {
        Ok(_) => Json(EvolutionResponse {
            success: true,
            data: Some(serde_json::json!({"rolled_back": true})),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/strategies - List strategy versions
async fn list_strategies(
    State(state): State<AppState>,
    Query(params): Query<EvolutionQuery>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    match engine.strategy_store().list(limit, offset).await {
        Ok(strategies) => Json(EvolutionResponse {
            success: true,
            data: Some(strategies),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/strategies - Create a new strategy version
async fn create_strategy(
    State(state): State<AppState>,
    Json(req): Json<CreateStrategyVersionRequest>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let risk_params = req.risk_params.unwrap_or(serde_json::json!({}));

    match engine
        .strategy_store()
        .create(
            &req.name,
            &req.strategy_type,
            req.parameters,
            risk_params,
            req.description.as_deref(),
            req.change_reason.as_deref(),
            req.parent_version_id,
        )
        .await
    {
        Ok(strategy) => Json(EvolutionResponse {
            success: true,
            data: Some(strategy),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/strategies/{id} - Get a specific strategy version
async fn get_strategy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    match engine.strategy_store().get(id).await {
        Ok(Some(strategy)) => Json(EvolutionResponse {
            success: true,
            data: Some(strategy),
            error: None,
        }),
        Ok(None) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some("Strategy version not found".to_string()),
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/strategies/{id}/activate - Activate a strategy version
async fn activate_strategy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());

    match engine.check_evolution_allowed().await {
        Ok(false) => {
            return Json(EvolutionResponse {
                success: false,
                data: None,
                error: Some("Evolution rate limit reached".to_string()),
            })
        }
        Ok(true) => {}
        Err(e) => {
            return Json(EvolutionResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            })
        }
    }

    match engine.strategy_store().activate(id).await {
        Ok(_) => Json(EvolutionResponse {
            success: true,
            data: Some(serde_json::json!({"activated": true})),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/reflections - List reflection logs
async fn list_reflections(
    State(state): State<AppState>,
    Query(params): Query<EvolutionQuery>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);

    match engine.reflection_store().list(limit).await {
        Ok(reflections) => Json(EvolutionResponse {
            success: true,
            data: Some(reflections),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/reflections - Trigger a reflection
async fn trigger_reflection(
    State(state): State<AppState>,
    Json(req): Json<TriggerReflectionRequest>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());

    let result = match req.reflection_type.as_str() {
        "daily_morning" => engine.run_daily_reflection().await,
        "weekly_review" => engine.run_weekly_review().await,
        "monthly_architecture" => engine.run_monthly_architecture_review().await,
        "triggered" => {
            engine
                .run_triggered_reflection(req.trigger.as_deref().unwrap_or("manual"))
                .await
        }
        _ => {
            return Json(EvolutionResponse {
                success: false,
                data: None,
                error: Some("Invalid reflection type. Use: daily_morning, weekly_review, monthly_architecture, triggered".to_string()),
            })
        }
    };

    match result {
        Ok(log) => Json(EvolutionResponse {
            success: true,
            data: Some(log),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/evolution/reflections/{id}/effectiveness - Update reflection effectiveness
async fn update_reflection_effectiveness(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let score = req
        .get("score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    match engine.reflection_store().update_effectiveness(id, score).await {
        Ok(_) => Json(EvolutionResponse {
            success: true,
            data: Some(serde_json::json!({"updated": true})),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/logs - List evolution logs
async fn list_evolution_logs(
    State(state): State<AppState>,
    Query(params): Query<EvolutionQuery>,
) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);

    match engine.evolution_log_store().list(limit).await {
        Ok(logs) => Json(EvolutionResponse {
            success: true,
            data: Some(logs),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/stats - Get evolution system statistics
async fn get_evolution_stats(State(state): State<AppState>) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    match engine.get_stats().await {
        Ok(stats) => Json(EvolutionResponse {
            success: true,
            data: Some(stats),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/evolution/check - Check if evolution is allowed (rate limit)
async fn check_evolution_allowed(State(state): State<AppState>) -> impl IntoResponse {
    let engine = EvolutionEngine::new(state.db_pool.clone());
    match engine.check_evolution_allowed().await {
        Ok(allowed) => Json(EvolutionResponse {
            success: true,
            data: Some(serde_json::json!({"allowed": allowed})),
            error: None,
        }),
        Err(e) => Json(EvolutionResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}
