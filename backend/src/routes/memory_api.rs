// Memory System API Routes
// Implements: AGENT_SYSTEM_DESIGN.md Chapter 12.11 - Memory System API Endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agents::memory::MemoryManager;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct MemoryQuery {
    pub agent_id: Option<String>,
    pub symbol: Option<String>,
    pub category: Option<String>,
    pub validated_only: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchRequest {
    pub query: String,
    pub symbol: Option<String>,
    pub category: Option<String>,
    pub limit: Option<i64>,
    // Note: embedding would be computed server-side in production
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Serialize)]
pub struct MemoryResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        // L2 Episodic Memory
        .route("/episodes", get(list_episodes))
        .route("/episodes/{id}", get(get_episode))
        // L3 Knowledge Memory
        .route("/knowledge", get(list_knowledge))
        .route("/knowledge/{id}", get(get_knowledge))
        .route("/knowledge/{id}/validate", post(validate_knowledge))
        .route("/knowledge/{id}/invalidate", post(invalidate_knowledge))
        // Semantic Search
        .route("/search", post(search_memory))
        // Calibration
        .route("/calibration", get(list_calibration))
        // Reflection
        .route("/reflect", post(trigger_reflection))
        // Stats
        .route("/stats", get(get_memory_stats))
}

/// GET /api/v1/agent/memory/episodes - List episodic memories
async fn list_episodes(
    State(state): State<AppState>,
    Query(params): Query<MemoryQuery>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    match memory
        .l2
        .list(params.agent_id.as_deref(), params.symbol.as_deref(), limit, offset)
        .await
    {
        Ok(episodes) => Json(MemoryResponse {
            success: true,
            data: Some(episodes),
            error: None,
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/memory/episodes/{id} - Get a specific episodic memory
async fn get_episode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    match memory.l2.get(id).await {
        Ok(Some(episode)) => Json(MemoryResponse {
            success: true,
            data: Some(episode),
            error: None,
        }),
        Ok(None) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some("Episodic memory not found".to_string()),
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/memory/knowledge - List knowledge memories
async fn list_knowledge(
    State(state): State<AppState>,
    Query(params): Query<MemoryQuery>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    let validated_only = params.validated_only.unwrap_or(false);

    match memory
        .l3
        .list(params.category.as_deref(), validated_only, limit, offset)
        .await
    {
        Ok(knowledge) => Json(MemoryResponse {
            success: true,
            data: Some(knowledge),
            error: None,
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/memory/knowledge/{id} - Get a specific knowledge memory
async fn get_knowledge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    match memory.l3.get(id).await {
        Ok(Some(knowledge)) => Json(MemoryResponse {
            success: true,
            data: Some(knowledge),
            error: None,
        }),
        Ok(None) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some("Knowledge memory not found".to_string()),
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/memory/knowledge/{id}/validate - Validate a knowledge item
async fn validate_knowledge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    match memory.l3.validate(id).await {
        Ok(_) => Json(MemoryResponse {
            success: true,
            data: Some(serde_json::json!({"validated": true})),
            error: None,
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/memory/knowledge/{id}/invalidate - Invalidate a knowledge item
async fn invalidate_knowledge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    match memory.l3.invalidate(id).await {
        Ok(_) => Json(MemoryResponse {
            success: true,
            data: Some(serde_json::json!({"invalidated": true})),
            error: None,
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/memory/search - Semantic search across memories
async fn search_memory(
    State(state): State<AppState>,
    Json(req): Json<MemorySearchRequest>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    let limit = req.limit.unwrap_or(10);

    // If embedding is provided, use vector search
    if let Some(embedding) = req.embedding {
        let results = memory
            .l2
            .search_by_embedding(&embedding, req.symbol.as_deref(), limit)
            .await;

        match results {
            Ok(episodes) => Json(MemoryResponse {
                success: true,
                data: Some(serde_json::json!({
                    "episodes": episodes,
                    "query": req.query,
                    "search_type": "semantic"
                })),
                error: None,
            }),
            Err(e) => Json(MemoryResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        }
    } else {
        // Fallback: text search (simplified - in production would use full-text search)
        Json(MemoryResponse {
            success: true,
            data: Some(serde_json::json!({
                "episodes": [],
                "query": req.query,
                "search_type": "text",
                "note": "Provide embedding for semantic search"
            })),
            error: None,
        })
    }
}

/// GET /api/v1/agent/memory/calibration - List agent calibration data
async fn list_calibration(
    State(state): State<AppState>,
    Query(params): Query<MemoryQuery>,
) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    let limit = params.limit.unwrap_or(50);

    match memory.calibration.list(limit).await {
        Ok(calibrations) => Json(MemoryResponse {
            success: true,
            data: Some(calibrations),
            error: None,
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/v1/agent/memory/reflect - Trigger memory reflection cycle
async fn trigger_reflection(State(state): State<AppState>) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    match memory.run_reflection_cycle().await {
        Ok(log) => Json(MemoryResponse {
            success: true,
            data: Some(log),
            error: None,
        }),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(MemoryResponse {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response(),
    }
}

/// GET /api/v1/agent/memory/stats - Get memory system statistics
async fn get_memory_stats(State(state): State<AppState>) -> impl IntoResponse {
    let memory = MemoryManager::new(state.db_pool.clone());
    match memory.get_stats().await {
        Ok(stats) => Json(MemoryResponse {
            success: true,
            data: Some(stats),
            error: None,
        }),
        Err(e) => Json(MemoryResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}
