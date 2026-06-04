use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{error, info};
use uuid::Uuid;

use crate::agents::models::*;
use crate::agents::promotion::PromotionSystem;
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/simulation/start", post(start_simulation))
        .route("/simulation/stop", post(stop_simulation))
        .route("/simulation/status", get(get_simulation_status))
        .route("/simulation/trades", get(get_trades))
        .route("/simulation/stats", get(get_stats))
        .route("/simulation/level", get(get_level))
        .route("/debate/start", post(start_debate))
        .route("/debate/{id}", get(get_debate_session))
        .route("/promotion/approve", post(approve_promotion))
        .route("/risk/confirmation/sign", post(sign_risk_confirmation))
        .route("/autonomous/start", post(start_autonomous))
        .route("/autonomous/stop", post(stop_autonomous))
        .route("/emergency/stop", post(emergency_stop))
}

#[derive(Debug, Deserialize)]
pub struct StartSimulationRequest {
    pub symbol: String,
    pub initial_balance: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StartSimulationResponse {
    pub config_id: Uuid,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SimulationStatusResponse {
    pub config: AiSimulationConfig,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct TradesResponse {
    pub trades: Vec<AiSimulationTrade>,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub config: AiSimulationConfig,
}

#[derive(Debug, Serialize)]
pub struct LevelResponse {
    pub current_level: i32,
    pub next_level: Option<i32>,
    pub eligibility: Option<PromotionEligibility>,
}

#[derive(Debug, Deserialize)]
pub struct StartDebateRequest {
    pub symbol: String,
    pub config_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct StartDebateResponse {
    pub session_id: Uuid,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ApprovePromotionRequest {
    pub audit_id: Uuid,
    pub review_comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApprovePromotionResponse {
    pub config: AiSimulationConfig,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SignRiskConfirmationRequest {
    pub config_id: Option<Uuid>,
    pub version: String,
    pub max_acceptable_loss: f64,
    pub accept_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SignRiskConfirmationResponse {
    pub confirmation_id: Uuid,
    pub signed: bool,
}

fn parse_agent_department(s: &str) -> AgentDepartment {
    match s {
        "Technical" => AgentDepartment::Technical,
        "Capital" => AgentDepartment::Capital,
        "News" => AgentDepartment::News,
        "FundManager" => AgentDepartment::FundManager,
        _ => AgentDepartment::Technical,
    }
}

fn parse_agent_sentiment(s: &str) -> Option<AgentSentiment> {
    match s {
        "Bullish" => Some(AgentSentiment::Bullish),
        "Bearish" => Some(AgentSentiment::Bearish),
        "Neutral" => Some(AgentSentiment::Neutral),
        "Cautious" => Some(AgentSentiment::Cautious),
        _ => None,
    }
}

async fn start_simulation(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<StartSimulationRequest>,
) -> Result<Json<StartSimulationResponse>> {
    let initial_balance = req.initial_balance.unwrap_or(10000.0);

    let config = sqlx::query_as::<_, AiSimulationConfig>(
        r#"
        INSERT INTO ai_simulation_configs (
            id, user_id, symbol, mode, level, status,
            initial_balance, current_balance,
            max_position_size_percent, max_leverage, max_daily_trades,
            max_daily_loss_percent, max_weekly_loss_percent, max_single_trade_loss_percent,
            ai_confidence_threshold, analysis_interval_minutes,
            allowed_symbols, autonomous_mode_enabled, requires_manual_confirm,
            total_trades, winning_trades, losing_trades,
            win_rate, avg_pnl_percent, profit_loss_ratio,
            max_drawdown_percent, sharpe_ratio,
            weekly_pnl, weekly_loss_percent, daily_pnl, daily_loss_percent,
            consecutive_stop_losses, running_days,
            promotion_eligible, risk_confirmation_signed,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, 'paper', 0, 'stopped', $4, $4,
                10.0, 3, 20, 5.0, 10.0, 2.0, 0.7, 60,
                ARRAY[$3]::VARCHAR[], false, true,
                0, 0, 0, 0.0, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 0.0, 0, 0,
                false, false, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(Uuid::new_v4())
    .bind(user.user_id)
    .bind(&req.symbol)
    .bind(initial_balance)
    .fetch_one(&state.db_pool)
    .await?;

    info!(
        "Simulation started for user {} with config {}",
        user.user_id, config.id
    );

    Ok(Json(StartSimulationResponse {
        config_id: config.id,
        status: "created".to_string(),
    }))
}

async fn stop_simulation(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<crate::schemas::MessageResponse>> {
    let config_id = req
        .get("config_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Validation("config_id is required".to_string()))?;

    let _config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE id = $1 AND user_id = $2"
    )
    .bind(config_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Simulation config not found".to_string()))?;

    sqlx::query(
        "UPDATE ai_simulation_configs SET status = 'stopped', updated_at = NOW() WHERE id = $1"
    )
    .bind(config_id)
    .execute(&state.db_pool)
    .await?;

    info!("Simulation stopped for config {}", config_id);

    Ok(Json(crate::schemas::MessageResponse::new(
        "Simulation stopped successfully",
    )))
}

async fn get_simulation_status(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<SimulationStatusResponse>> {
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No simulation config found".to_string()))?;

    Ok(Json(SimulationStatusResponse {
        status: config.status.clone(),
        config,
    }))
}

async fn get_trades(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<TradesResponse>> {
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No simulation config found".to_string()))?;

    let trades = sqlx::query_as::<_, AiSimulationTrade>(
        "SELECT * FROM ai_simulation_trades WHERE config_id = $1 ORDER BY opened_at DESC LIMIT 100"
    )
    .bind(config.id)
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(TradesResponse { trades }))
}

async fn get_stats(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<StatsResponse>> {
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No simulation config found".to_string()))?;

    Ok(Json(StatsResponse { config }))
}

async fn get_level(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<LevelResponse>> {
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No simulation config found".to_string()))?;

    let promotion_system = PromotionSystem::new(state.db_pool.clone());
    let eligibility = promotion_system.check_promotion_eligibility(&config).await.ok();
    let next_level = PromotionSystem::get_next_level(config.level);

    Ok(Json(LevelResponse {
        current_level: config.level,
        next_level,
        eligibility,
    }))
}

async fn start_debate(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<StartDebateRequest>,
) -> Result<Json<StartDebateResponse>> {
    let session_id = Uuid::new_v4();

    let _market_snapshot = MarketSnapshot {
        symbol: req.symbol.clone(),
        current_price: 42000.0,
        open_24h: 41000.0,
        high_24h: 42500.0,
        low_24h: 40500.0,
        close_24h: 42000.0,
        volume_24h: 1000000000.0,
        price_change_percent_24h: 2.44,
        funding_rate: Some(-0.0001),
        open_interest: Some(500000000.0),
        long_short_ratio: Some(1.2),
        rsi_14: Some(55.0),
        macd_signal: Some(0.001),
        timestamp: Utc::now(),
    };

    let _session = DebateSession {
        id: session_id,
        config_id: req.config_id,
        user_id: Some(user.user_id),
        symbol: req.symbol,
        status: DebateStatus::InProgress,
        messages: Vec::new(),
        final_decision: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    info!("Debate session started: {}", session_id);

    Ok(Json(StartDebateResponse {
        session_id,
        status: "in_progress".to_string(),
    }))
}

async fn get_debate_session(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DebateSession>> {
    let session_row = sqlx::query(
        "SELECT id, config_id, user_id, symbol, status, created_at, updated_at FROM debate_sessions WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Debate session not found".to_string()))?;

    let session_id: Uuid = session_row.get("id");
    let config_id: Option<Uuid> = session_row.get("config_id");
    let user_id: Option<i64> = session_row.get("user_id");
    let symbol: String = session_row.get("symbol");
    let status: String = session_row.get("status");
    let created_at: chrono::DateTime<Utc> = session_row.get("created_at");
    let updated_at: chrono::DateTime<Utc> = session_row.get("updated_at");

    let message_rows = sqlx::query(
        r#"SELECT
            id,
            session_id,
            agent_name,
            agent_department,
            role,
            content,
            analysis_data,
            confidence,
            sentiment,
            message_order,
            created_at
        FROM debate_messages WHERE session_id = $1 ORDER BY message_order"#
    )
    .bind(id)
    .fetch_all(&state.db_pool)
    .await?;

    let debate_messages: Vec<DebateMessage> = message_rows
        .iter()
        .map(|m| DebateMessage {
            id: m.get("id"),
            session_id: m.get("session_id"),
            agent_name: m.get("agent_name"),
            agent_department: parse_agent_department(m.get::<String, _>("agent_department").as_str()),
            role: m.get("role"),
            content: m.get("content"),
            analysis_data: m.get("analysis_data"),
            confidence: m.get("confidence"),
            sentiment: m.get::<Option<String>, _>("sentiment").as_deref().and_then(parse_agent_sentiment),
            message_order: m.get("message_order"),
            created_at: m.get("created_at"),
        })
        .collect();

    let session = DebateSession {
        id: session_id,
        config_id,
        user_id,
        symbol,
        status: if status == "completed" {
            DebateStatus::Completed
        } else if status == "failed" {
            DebateStatus::Failed
        } else {
            DebateStatus::InProgress
        },
        messages: debate_messages,
        final_decision: None,
        created_at,
        updated_at,
    };

    Ok(Json(session))
}

async fn approve_promotion(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ApprovePromotionRequest>,
) -> Result<Json<ApprovePromotionResponse>> {
    let promotion_system = PromotionSystem::new(state.db_pool.clone());

    let updated_config = promotion_system
        .approve_promotion(req.audit_id, Some(user.user_id.to_string()), req.review_comment)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ApprovePromotionResponse {
        config: updated_config,
        status: "approved".to_string(),
    }))
}

async fn sign_risk_confirmation(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<SignRiskConfirmationRequest>,
) -> Result<Json<SignRiskConfirmationResponse>> {
    let promotion_system = PromotionSystem::new(state.db_pool.clone());

    let confirmation = promotion_system
        .sign_risk_confirmation(
            user.user_id,
            req.config_id,
            req.version,
            req.max_acceptable_loss,
            req.accept_reason,
            None,
            None,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(SignRiskConfirmationResponse {
        confirmation_id: confirmation.id,
        signed: confirmation.accepted,
    }))
}

async fn start_autonomous(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<crate::schemas::MessageResponse>> {
    let config_id = req
        .get("config_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Validation("config_id is required".to_string()))?;

    let _config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE id = $1 AND user_id = $2"
    )
    .bind(config_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Simulation config not found".to_string()))?;

    sqlx::query(
        "UPDATE ai_simulation_configs SET autonomous_mode_enabled = true, status = 'running', updated_at = NOW() WHERE id = $1"
    )
    .bind(config_id)
    .execute(&state.db_pool)
    .await?;

    info!("Autonomous mode started for config {}", config_id);

    Ok(Json(crate::schemas::MessageResponse::new(
        "Autonomous mode started successfully",
    )))
}

async fn stop_autonomous(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<crate::schemas::MessageResponse>> {
    let config_id = req
        .get("config_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Validation("config_id is required".to_string()))?;

    let _config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE id = $1 AND user_id = $2"
    )
    .bind(config_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Simulation config not found".to_string()))?;

    sqlx::query(
        "UPDATE ai_simulation_configs SET autonomous_mode_enabled = false, updated_at = NOW() WHERE id = $1"
    )
    .bind(config_id)
    .execute(&state.db_pool)
    .await?;

    info!("Autonomous mode stopped for config {}", config_id);

    Ok(Json(crate::schemas::MessageResponse::new(
        "Autonomous mode stopped successfully",
    )))
}

async fn emergency_stop(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<crate::schemas::MessageResponse>> {
    let config_id = req
        .get("config_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Validation("config_id is required".to_string()))?;

    let _config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE id = $1 AND user_id = $2"
    )
    .bind(config_id)
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Simulation config not found".to_string()))?;

    sqlx::query(
        "UPDATE ai_simulation_configs SET status = 'emergency_stopped', autonomous_mode_enabled = false, updated_at = NOW() WHERE id = $1"
    )
    .bind(config_id)
    .execute(&state.db_pool)
    .await?;

    info!("EMERGENCY STOP activated for config {}", config_id);

    Ok(Json(crate::schemas::MessageResponse::new(
        "Emergency stop activated successfully",
    )))
}
