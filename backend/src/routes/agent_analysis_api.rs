// Agent Analysis API Routes
// Implements: AGENT_SYSTEM_DESIGN.md Chapter 5 - Agent Analysis API Endpoints

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::agents::debate::{AgentRegistry, DebateEngine};
use crate::agents::llm_client::LlmClient;
use crate::agents::models::*;
use crate::state::AppState;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AnalysisQuery {
    pub user_id: Option<i64>,
    pub config_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub symbol: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/analyze/{symbol}", post(analyze_symbol))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/debate", get(get_session_debate))
        .route("/decisions", get(list_decisions))
        .route("/decisions/{id}", get(get_decision))
        .route("/agents", get(list_agents))
        .route("/performance", get(get_performance))
}

/// POST /api/v1/agent/analyze/{symbol} - Trigger agent analysis for a symbol
async fn analyze_symbol(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(params): Query<AnalysisQuery>,
) -> impl IntoResponse {
    let llm_client = if LlmClient::is_configured() {
        match LlmClient::new_from_env().await {
            Ok(client) => Some(Arc::new(client)),
            Err(_) => None,
        }
    } else {
        None
    };

    let engine = DebateEngine::new(Arc::new(state.db_pool.clone()), llm_client);

    // Build a market snapshot from latest ticker data
    let snapshot = match build_market_snapshot(&state.db_pool, &symbol).await {
        Ok(s) => s,
        Err(e) => {
            return Json(AgentResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to build market snapshot: {}", e)),
            })
        }
    };

    match engine
        .run_debate(&symbol, snapshot, params.config_id, params.user_id)
        .await
    {
        Ok(session) => {
            // Save session to DB
            let _ = engine.save_session_to_db(&session).await;
            Json(AgentResponse {
                success: true,
                data: Some(session),
                error: None,
            })
        }
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/sessions - List analysis sessions
async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<AnalysisQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let result = if let Some(sym) = params.symbol {
        sqlx::query(
            r#"SELECT id, config_id, user_id, symbol, status, created_at, updated_at
               FROM debate_sessions
               WHERE symbol = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(&sym)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db_pool)
        .await
    } else {
        sqlx::query(
            r#"SELECT id, config_id, user_id, symbol, status, created_at, updated_at
               FROM debate_sessions
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db_pool)
        .await
    };

    match result {
        Ok(rows) => {
            let sessions: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.get::<Uuid, _>("id"),
                        "config_id": r.get::<Option<Uuid>, _>("config_id"),
                        "user_id": r.get::<Option<i64>, _>("user_id"),
                        "symbol": r.get::<String, _>("symbol"),
                        "status": r.get::<String, _>("status"),
                        "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                        "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                    })
                })
                .collect();
            Json(AgentResponse {
                success: true,
                data: Some(sessions),
                error: None,
            })
        }
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/sessions/{id} - Get a specific session
async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let llm_client = if LlmClient::is_configured() {
        match LlmClient::new_from_env().await {
            Ok(client) => Some(Arc::new(client)),
            Err(_) => None,
        }
    } else {
        None
    };

    let engine = DebateEngine::new(Arc::new(state.db_pool.clone()), llm_client);

    match engine.get_session_from_db(id).await {
        Ok(Some(session)) => Json(AgentResponse {
            success: true,
            data: Some(session),
            error: None,
        }),
        Ok(None) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some("Session not found".to_string()),
        }),
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/sessions/{id}/debate - Get debate messages for a session
async fn get_session_debate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query(
        r#"SELECT id, session_id, agent_name, agent_department, role, content,
                  analysis_data, confidence, sentiment, message_order, created_at
           FROM debate_messages
           WHERE session_id = $1
           ORDER BY message_order"#,
    )
    .bind(id)
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(rows) => {
            let messages: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.get::<Uuid, _>("id"),
                        "session_id": r.get::<Uuid, _>("session_id"),
                        "agent_name": r.get::<String, _>("agent_name"),
                        "agent_department": r.get::<String, _>("agent_department"),
                        "role": r.get::<String, _>("role"),
                        "content": r.get::<String, _>("content"),
                        "analysis_data": r.get::<serde_json::Value, _>("analysis_data"),
                        "confidence": r.get::<f64, _>("confidence"),
                        "sentiment": r.get::<Option<String>, _>("sentiment"),
                        "message_order": r.get::<i32, _>("message_order"),
                        "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    })
                })
                .collect();
            Json(AgentResponse {
                success: true,
                data: Some(messages),
                error: None,
            })
        }
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/decisions - List fund manager decisions
async fn list_decisions(
    State(state): State<AppState>,
    Query(params): Query<AnalysisQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let result = if let Some(sym) = params.symbol {
        sqlx::query(
            r#"SELECT id, session_id, symbol, action, confidence, position_size_percent,
                      leverage, stop_loss_percent, take_profit_percent, reasoning, timestamp
               FROM fund_manager_decisions
               WHERE symbol = $1
               ORDER BY timestamp DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(&sym)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db_pool)
        .await
    } else {
        sqlx::query(
            r#"SELECT id, session_id, symbol, action, confidence, position_size_percent,
                      leverage, stop_loss_percent, take_profit_percent, reasoning, timestamp
               FROM fund_manager_decisions
               ORDER BY timestamp DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db_pool)
        .await
    };

    match result {
        Ok(rows) => {
            let decisions: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.get::<Uuid, _>("id"),
                        "session_id": r.get::<Uuid, _>("session_id"),
                        "symbol": r.get::<String, _>("symbol"),
                        "action": r.get::<String, _>("action"),
                        "confidence": r.get::<f64, _>("confidence"),
                        "position_size_percent": r.get::<f64, _>("position_size_percent"),
                        "leverage": r.get::<i32, _>("leverage"),
                        "stop_loss_percent": r.get::<Option<f64>, _>("stop_loss_percent"),
                        "take_profit_percent": r.get::<Option<f64>, _>("take_profit_percent"),
                        "reasoning": r.get::<String, _>("reasoning"),
                        "timestamp": r.get::<chrono::DateTime<chrono::Utc>, _>("timestamp"),
                    })
                })
                .collect();
            Json(AgentResponse {
                success: true,
                data: Some(decisions),
                error: None,
            })
        }
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/decisions/{id} - Get a specific decision
async fn get_decision(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query(
        r#"SELECT id, session_id, symbol, action, confidence, position_size_percent,
                  leverage, stop_loss_percent, take_profit_percent, reasoning,
                  agent_contributions, risk_assessment, timestamp
           FROM fund_manager_decisions
           WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(row)) => {
            let decision = serde_json::json!({
                "id": row.get::<Uuid, _>("id"),
                "session_id": row.get::<Uuid, _>("session_id"),
                "symbol": row.get::<String, _>("symbol"),
                "action": row.get::<String, _>("action"),
                "confidence": row.get::<f64, _>("confidence"),
                "position_size_percent": row.get::<f64, _>("position_size_percent"),
                "leverage": row.get::<i32, _>("leverage"),
                "stop_loss_percent": row.get::<Option<f64>, _>("stop_loss_percent"),
                "take_profit_percent": row.get::<Option<f64>, _>("take_profit_percent"),
                "reasoning": row.get::<String, _>("reasoning"),
                "agent_contributions": row.get::<serde_json::Value, _>("agent_contributions"),
                "risk_assessment": row.get::<serde_json::Value, _>("risk_assessment"),
                "timestamp": row.get::<chrono::DateTime<chrono::Utc>, _>("timestamp"),
            });
            Json(AgentResponse {
                success: true,
                data: Some(decision),
                error: None,
            })
        }
        Ok(None) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some("Decision not found".to_string()),
        }),
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/v1/agent/agents - List all registered agents
async fn list_agents(State(_state): State<AppState>) -> impl IntoResponse {
    let registry = AgentRegistry::new();
    let profiles = registry.get_agent_profiles();

    let agents: Vec<serde_json::Value> = profiles
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "department": format!("{:?}", p.department),
                "role": p.role,
                "reference_institution": p.reference_institution,
                "credibility_score": p.credibility_score,
                "calibration_factor": p.calibration_factor,
            })
        })
        .collect();

    Json(AgentResponse {
        success: true,
        data: Some(agents),
        error: None,
    })
}

/// GET /api/v1/agent/performance - Get agent performance metrics
async fn get_performance(
    State(state): State<AppState>,
    Query(params): Query<AnalysisQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);

    let result = if let Some(agent_id) = params.symbol {
        sqlx::query(
            r#"SELECT agent_id, total_predictions, correct_predictions, accuracy_score,
                      credibility_score, weighted_accuracy, trend_accuracy, volatility_accuracy,
                      volume_accuracy, timing_accuracy, multi_timeframe_accuracy,
                      prediction_decay_rate, updated_at
               FROM agent_performance
               WHERE agent_id = $1
               ORDER BY updated_at DESC
               LIMIT $2"#,
        )
        .bind(&agent_id)
        .bind(limit)
        .fetch_all(&state.db_pool)
        .await
    } else {
        sqlx::query(
            r#"SELECT agent_id, total_predictions, correct_predictions, accuracy_score,
                      credibility_score, weighted_accuracy, trend_accuracy, volatility_accuracy,
                      volume_accuracy, timing_accuracy, multi_timeframe_accuracy,
                      prediction_decay_rate, updated_at
               FROM agent_performance
               ORDER BY updated_at DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&state.db_pool)
        .await
    };

    match result {
        Ok(rows) => {
            let performances: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "agent_id": r.get::<String, _>("agent_id"),
                        "total_predictions": r.get::<i64, _>("total_predictions"),
                        "correct_predictions": r.get::<i64, _>("correct_predictions"),
                        "accuracy_score": r.get::<f64, _>("accuracy_score"),
                        "credibility_score": r.get::<f64, _>("credibility_score"),
                        "weighted_accuracy": r.get::<Option<f64>, _>("weighted_accuracy"),
                        "trend_accuracy": r.get::<Option<f64>, _>("trend_accuracy"),
                        "volatility_accuracy": r.get::<Option<f64>, _>("volatility_accuracy"),
                        "volume_accuracy": r.get::<Option<f64>, _>("volume_accuracy"),
                        "timing_accuracy": r.get::<Option<f64>, _>("timing_accuracy"),
                        "multi_timeframe_accuracy": r.get::<Option<f64>, _>("multi_timeframe_accuracy"),
                        "prediction_decay_rate": r.get::<Option<f64>, _>("prediction_decay_rate"),
                        "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                    })
                })
                .collect();
            Json(AgentResponse {
                success: true,
                data: Some(performances),
                error: None,
            })
        }
        Err(e) => Json(AgentResponse {
            success: false,
            data: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Build a market snapshot from the latest ticker data in DB.
async fn build_market_snapshot(
    db_pool: &sqlx::PgPool,
    symbol: &str,
) -> Result<MarketSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    let row = sqlx::query(
        r#"SELECT symbol, last_price, open_24h, high_24h, low_24h, volume_24h,
                  price_change_percent_24h, funding_rate, timestamp
           FROM ticker_history
           WHERE symbol = $1
           ORDER BY timestamp DESC
           LIMIT 1"#,
    )
    .bind(symbol)
    .fetch_optional(db_pool)
    .await?;

    match row {
        Some(r) => Ok(MarketSnapshot {
            symbol: r.get("symbol"),
            current_price: r.get::<Option<f64>, _>("last_price").unwrap_or(0.0),
            open_24h: r.get::<Option<f64>, _>("open_24h").unwrap_or(0.0),
            high_24h: r.get::<Option<f64>, _>("high_24h").unwrap_or(0.0),
            low_24h: r.get::<Option<f64>, _>("low_24h").unwrap_or(0.0),
            close_24h: r.get::<Option<f64>, _>("last_price").unwrap_or(0.0),
            volume_24h: r.get::<Option<f64>, _>("volume_24h").unwrap_or(0.0),
            price_change_percent_24h: r
                .get::<Option<f64>, _>("price_change_percent_24h")
                .unwrap_or(0.0),
            funding_rate: r.get("funding_rate"),
            open_interest: None,
            long_short_ratio: None,
            rsi_14: None,
            macd_signal: None,
            timestamp: r.get("timestamp"),
        }),
        None => {
            // Return a default snapshot if no data
            Ok(MarketSnapshot {
                symbol: symbol.to_string(),
                current_price: 0.0,
                open_24h: 0.0,
                high_24h: 0.0,
                low_24h: 0.0,
                close_24h: 0.0,
                volume_24h: 0.0,
                price_change_percent_24h: 0.0,
                funding_rate: None,
                open_interest: None,
                long_short_ratio: None,
                rsi_14: None,
                macd_signal: None,
                timestamp: chrono::Utc::now(),
            })
        }
    }
}
