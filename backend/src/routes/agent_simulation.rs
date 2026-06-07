use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::agents::llm_client::LlmClient;
use crate::agents::models::*;
use crate::agents::promotion::PromotionSystem;
use crate::agents::simulation::SimulationEngine;
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::routes::ai_analysis::run_debate_for_simulation;
use crate::routes::trading::get_okx_client;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(get_dashboard))
        .route("/simulation/start", post(start_simulation))
        .route("/simulation/stop", post(stop_simulation))
        .route("/simulation/status", get(get_simulation_status))
        .route("/simulation/trades", get(get_trades))
        .route("/simulation/stats", get(get_stats))
        .route("/simulation/level", get(get_level))
        .route("/simulation/config", post(update_simulation_config))
        .route("/debate/start", post(start_debate))
        .route("/debate/{id}", get(get_debate_session))
        .route("/promotion/approve", post(approve_promotion))
        .route("/promotion/initiate", post(initiate_promotion))
        .route("/risk/confirmation/sign", post(sign_risk_confirmation))
        .route("/autonomous/start", post(start_autonomous))
        .route("/autonomous/stop", post(stop_autonomous))
        .route("/emergency/stop", post(emergency_stop))
}

#[derive(Debug, Deserialize)]
pub struct StartSimulationRequest {
    pub symbol: String,
    pub initial_balance: Option<f64>,
    pub interval: Option<String>,
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
    let interval = req.interval.unwrap_or_else(|| "1H".to_string());

    // Stop any existing running simulation for this user
    sqlx::query(
        "UPDATE ai_simulation_configs SET status = 'stopped', updated_at = NOW() WHERE user_id = $1 AND status = 'running'"
    )
    .bind(user.user_id)
    .execute(&state.db_pool)
    .await?;

    let config = sqlx::query_as::<_, AiSimulationConfig>(
        r#"
        INSERT INTO ai_simulation_configs (
            id, user_id, symbol, mode, level, status,
            initial_balance, current_balance,
            max_position_size_percent, max_leverage, max_daily_trades,
            max_daily_loss_percent, max_weekly_loss_percent, max_single_trade_loss_percent,
            ai_confidence_threshold, analysis_interval_minutes, analysis_interval,
            allowed_symbols, autonomous_mode_enabled, requires_manual_confirm,
            total_trades, winning_trades, losing_trades,
            win_rate, avg_pnl_percent, profit_loss_ratio,
            max_drawdown_percent, sharpe_ratio,
            weekly_pnl, weekly_loss_percent, daily_pnl, daily_loss_percent,
            consecutive_stop_losses, running_days,
            promotion_eligible, risk_confirmation_signed,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, 'paper', 0, 'running', $4, $4,
                10.0, 3, 20, 5.0, 10.0, 2.0, 0.7, 60, $5,
                ARRAY[$3]::VARCHAR[], false, false,
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
    .bind(&interval)
    .fetch_one(&state.db_pool)
    .await?;

    let config_id = config.id;

    info!(
        "Simulation started for user {} with config {} symbol {} interval {}, spawning auto-trade loop",
        user.user_id, config_id, req.symbol, interval
    );

    // Spawn background auto-trading loop
    spawn_simulation_loop(config_id, user.user_id, state);

    Ok(Json(StartSimulationResponse {
        config_id,
        status: "running".to_string(),
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

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub symbol: Option<String>,
    pub interval: Option<String>,
}

async fn update_simulation_config(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>> {
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No simulation config found".to_string()))?;

    let mut updates = Vec::new();
    let mut param_idx = 2u32; // $1 is config_id

    if let Some(ref symbol) = req.symbol {
        updates.push(format!("symbol = ${}", param_idx));
        param_idx += 1;
        updates.push(format!("allowed_symbols = ARRAY[${}]::VARCHAR[]", param_idx));
        param_idx += 1;
    }
    if let Some(ref interval) = req.interval {
        updates.push(format!("analysis_interval = ${}", param_idx));
        param_idx += 1;
    }

    if updates.is_empty() {
        return Ok(Json(serde_json::json!({"success": true, "message": "No changes"})));
    }

    updates.push("updated_at = NOW()".to_string());
    let sql = format!(
        "UPDATE ai_simulation_configs SET {} WHERE id = $1",
        updates.join(", ")
    );

    let mut query = sqlx::query(&sql).bind(config.id);

    if let Some(ref symbol) = req.symbol {
        query = query.bind(symbol).bind(symbol);
    }
    if let Some(ref interval) = req.interval {
        query = query.bind(interval);
    }

    query.execute(&state.db_pool).await?;

    info!(
        "Config {} updated: symbol={:?}, interval={:?}",
        config.id, req.symbol, req.interval
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "symbol": req.symbol,
        "interval": req.interval,
    })))
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

// ============ Simulation Auto-Trade Loop ============

pub fn spawn_simulation_loop(config_id: Uuid, user_id: i64, state: AppState) {
    let pool = state.db_pool.clone();
    let config_id_str = config_id.to_string();

    tokio::spawn(async move {
        info!("[SimLoop] Starting auto-trade loop for config {}", config_id_str);
        let engine = SimulationEngine::new(pool.clone());

        loop {
            // Check if simulation is still running
            let status: Option<String> = sqlx::query_scalar(
                "SELECT status FROM ai_simulation_configs WHERE id = $1"
            )
            .bind(config_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

            match status {
                Some(s) if s == "running" => {}
                Some(s) => {
                    info!("[SimLoop] Simulation {} status is '{}', stopping loop", config_id_str, s);
                    break;
                }
                None => {
                    info!("[SimLoop] Config {} no longer exists, stopping loop", config_id_str);
                    break;
                }
            }

            // Load config
            let config = match sqlx::query_as::<_, AiSimulationConfig>(
                "SELECT * FROM ai_simulation_configs WHERE id = $1"
            )
            .bind(config_id)
            .fetch_optional(&pool)
            .await
            {
                Ok(Some(c)) => c,
                _ => break,
            };

            // Step 1: Check and close open trades (stop-loss / take-profit / timeout)
            let mut closed_any = false;
            if let Ok(open_trades) = sqlx::query_as::<_, AiSimulationTrade>(
                "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open'"
            )
            .bind(config_id)
            .fetch_all(&pool)
            .await
            {
                for trade in &open_trades {
                    let current_price = match get_current_price(&state, user_id, &trade.symbol).await {
                        Some(p) => p,
                        None => continue,
                    };

                    let should_close = check_should_close(trade, current_price);
                    let holding_mins = Utc::now().signed_duration_since(trade.opened_at).num_minutes();
                    let timeout_close = holding_mins > 240;

                    if let Some(reason) = should_close.or_else(|| if timeout_close { Some("timeout".to_string()) } else { None }) {
                        match engine.close_trade(trade.id, current_price, &reason).await {
                            Ok(result) => {
                                info!("[SimLoop] Closed trade {} with PnL {:.2}% ({})", trade.id, result.pnl_percent, reason);
                                closed_any = true;
                            }
                            Err(e) => warn!("[SimLoop] Failed to close trade {}: {}", trade.id, e),
                        }
                    }
                }
            }

            // If we just closed a trade, immediately re-analyze for next trade (closed loop)
            if closed_any {
                // Reload config after close (balance updated)
                let config = match sqlx::query_as::<_, AiSimulationConfig>(
                    "SELECT * FROM ai_simulation_configs WHERE id = $1"
                )
                .bind(config_id)
                .fetch_optional(&pool)
                .await
                {
                    Ok(Some(c)) => c,
                    _ => break,
                };

                // Check daily limit after close
                let today_trades: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM ai_simulation_trades WHERE config_id = $1 AND opened_at > NOW() - INTERVAL '1 day'"
                )
                .bind(config_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

                if today_trades < config.max_daily_trades as i64 {
                    info!("[SimLoop] Trade closed, immediately re-analyzing for next opportunity");
                    if let Err(e) = try_open_new_trade(&engine, &state, &pool, &config, user_id).await {
                        warn!("[SimLoop] Re-analysis after close failed: {}", e);
                    }
                } else {
                    info!("[SimLoop] Daily trade limit reached after close, waiting");
                }

                // Continue loop (will check SL/TP for new trade on next iteration)
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                continue;
            }

            // Step 2: Check daily trade limit
            let today_trades: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM ai_simulation_trades WHERE config_id = $1 AND opened_at > NOW() - INTERVAL '1 day'"
            )
            .bind(config_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

            let open_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open'"
            )
            .bind(config_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

            // Only open new trade if under limits and no open positions
            if today_trades < config.max_daily_trades as i64 && open_count == 0 {
                if let Err(e) = try_open_new_trade(&engine, &state, &pool, &config, user_id).await {
                    warn!("[SimLoop] Open trade failed: {}", e);
                }
            }

            // Step 3: Update running stats
            let _ = update_running_stats(&pool, config_id).await;

            // Wait before next cycle (5 minutes)
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
        }

        info!("[SimLoop] Auto-trade loop stopped for config {}", config_id_str);
    });
}

async fn try_open_new_trade(
    engine: &SimulationEngine,
    state: &AppState,
    pool: &sqlx::PgPool,
    config: &AiSimulationConfig,
    user_id: i64,
) -> Result<()> {
    let current_price = match get_current_price(state, user_id, &config.symbol).await {
        Some(p) => p,
        None => return Ok(()), // Can't get price, skip
    };

    // Use AI debate flow instead of simple LLM call
    info!("[SimLoop] Starting AI debate for {} at price {:.2}", config.symbol, current_price);
    let debate = match run_debate_for_simulation(pool, user_id, &config.symbol, current_price, &config.analysis_interval).await {
        Ok(d) => d,
        Err(e) => {
            warn!("[SimLoop] AI debate failed: {}, skipping trade", e);
            return Ok(());
        }
    };

    info!("[SimLoop] Debate completed: action={}, confidence={:.0}%, session={}",
        debate.action, debate.confidence * 100.0, debate.session_id);

    // Skip if action is "hold"
    if debate.action == "hold" {
        info!("[SimLoop] Debate suggests hold, skipping trade");
        return Ok(());
    }

    let direction = debate.action.as_str(); // "long" or "short"

    // Use a lower threshold for paper trading to ensure we get trades for validation
    let effective_threshold = if config.mode == "paper" {
        (config.ai_confidence_threshold * 0.7).min(0.5)
    } else {
        config.ai_confidence_threshold
    };

    if debate.confidence < effective_threshold {
        info!("[SimLoop] Confidence too low ({:.0}% < {:.0}%), skipping",
            debate.confidence * 100.0, effective_threshold * 100.0);
        return Ok(());
    }

    // Calculate position size
    let position_value = config.current_balance * (config.max_position_size_percent / 100.0);
    let quantity = position_value / current_price;
    let leverage = debate.leverage.min(config.max_leverage).max(1);

    // Use debate's stop_loss and take_profit
    let stop_loss = Some(debate.stop_loss);
    let take_profit = debate.take_profit.first().copied(); // Use first TP level

    // Build reasoning with full debate context
    let reasoning = serde_json::json!({
        "debate_session_id": debate.session_id.to_string(),
        "action": debate.action,
        "confidence": debate.confidence,
        "reasoning": debate.reasoning,
        "leverage": debate.leverage,
        "agent_opinions": debate.agent_opinions,
        "department_reports": debate.department_reports,
    });

    match engine.execute_trade(
        config,
        direction,
        current_price,
        quantity,
        leverage,
        stop_loss,
        take_profit,
        Some(debate.confidence),
        Some(reasoning),
        Some(debate.session_id),
    ).await {
        Ok(_) => {
            info!("[SimLoop] Opened {} {} trade at {} via debate {} (confidence: {:.0}%)",
                direction, config.symbol, current_price, debate.session_id, debate.confidence * 100.0);
        }
        Err(e) => warn!("[SimLoop] Failed to execute trade: {}", e),
    }

    Ok(())
}

async fn get_current_price(state: &AppState, user_id: i64, symbol: &str) -> Option<f64> {
    let okx_client = get_okx_client(state, user_id).await.ok()?;
    let symbol_owned = symbol.to_string();
    let result = okx_client.get_raw("/api/v5/market/ticker", Some(&[("instId", symbol_owned)])).await.ok()?;
    let data = result.get("data")?.as_array()?.first()?;
    let last = data.get("last")?.as_str()?;
    last.parse::<f64>().ok()
}

fn check_should_close(trade: &AiSimulationTrade, current_price: f64) -> Option<String> {
    if let Some(sl) = trade.stop_loss {
        if trade.direction == "long" && current_price <= sl {
            return Some("stop_loss".to_string());
        }
        if trade.direction == "short" && current_price >= sl {
            return Some("stop_loss".to_string());
        }
    }
    if let Some(tp) = trade.take_profit {
        if trade.direction == "long" && current_price >= tp {
            return Some("take_profit".to_string());
        }
        if trade.direction == "short" && current_price <= tp {
            return Some("take_profit".to_string());
        }
    }
    None
}

async fn format_market_summary_with_history(config: &AiSimulationConfig, current_price: f64, pool: &sqlx::PgPool) -> String {
    let recent_trades: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed' AND closed_at > NOW() - INTERVAL '7 days'"
    )
    .bind(config.id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Get last 5 closed trades for feedback
    let last_trades = sqlx::query_as::<_, AiSimulationTrade>(
        "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed' ORDER BY closed_at DESC LIMIT 5"
    )
    .bind(config.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut history_str = String::new();
    for (i, t) in last_trades.iter().enumerate() {
        let pnl = t.pnl_percent.map(|p| format!("{:+.2}%", p)).unwrap_or_else(|| "N/A".to_string());
        let dir = if t.direction == "long" { "做多" } else { "做空" };
        let reason = t.close_reason.as_deref().unwrap_or("N/A");
        history_str.push_str(&format!(
            "  {}. {} {} → {} (平仓: {})\n",
            i + 1, dir, t.symbol, pnl, reason
        ));
    }

    format!(
        "交易对: {}\n当前价格: {:.2}\n账户余额: {:.2} (初始: {:.2})\n胜率: {:.1}%\n总交易: {}\n盈亏比: {:.2}\n最大回撤: {:.1}%\n近7天交易: {}\n\n最近交易历史:\n{}\n请基于以上数据和市场状况，给出你的交易建议。注意从历史交易中学习，如果之前的交易亏损，分析原因并调整策略。你必须给出明确的做多或做空方向。",
        config.symbol,
        current_price,
        config.current_balance,
        config.initial_balance,
        config.win_rate * 100.0,
        config.total_trades,
        config.profit_loss_ratio,
        config.max_drawdown_percent,
        recent_trades,
        if history_str.is_empty() { "  暂无交易记录".to_string() } else { history_str },
    )
}

#[derive(Debug)]
struct LlmAnalysisResult {
    sentiment: String,
    confidence: f64,
    analysis: String,
    key_factors: Vec<String>,
}

async fn get_llm_analysis(state: &AppState, user_id: i64, market_summary: &str) -> Result<LlmAnalysisResult> {
    // Get LLM config from database
    let row = sqlx::query(
        r#"SELECT provider, api_key_encrypted, base_url, model, max_tokens, temperature
           FROM ai_provider_configs
           WHERE user_id = $1 AND is_active = true
           ORDER BY is_default DESC, created_at DESC
           LIMIT 1"#
    )
    .bind(user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::Validation("请先在管理页面配置 AI 供应商".to_string()))?;

    use crate::utils::encryption::decrypt;
    let provider_str: String = row.try_get("provider").unwrap_or_else(|_| "openai".to_string());
    let encrypted_key: String = row.try_get("api_key_encrypted").unwrap_or_default();
    let api_key = decrypt(&encrypted_key)?;
    let base_url: String = row.try_get("base_url").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
    let model: String = row.try_get("model").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let max_tokens: i32 = row.try_get("max_tokens").unwrap_or(1024);
    let temperature: f64 = row.try_get("temperature").unwrap_or(0.7);

    let provider = match provider_str.to_lowercase().as_str() {
        "deepseek" => crate::agents::llm_client::LlmProvider::DeepSeek,
        "anthropic" => crate::agents::llm_client::LlmProvider::Anthropic,
        _ => crate::agents::llm_client::LlmProvider::OpenAI,
    };

    let llm_config = crate::agents::llm_client::LlmConfig {
        provider,
        api_key,
        base_url,
        model,
        max_tokens,
        temperature,
    };

    let client = LlmClient::new(llm_config);

    let system_prompt = r#"你是一个激进的加密货币交易分析师，专门负责模拟交易验证。你的任务是基于市场数据给出明确的交易方向建议。
重要规则：
1. 你必须给出明确的做多(bullish)或做空(bearish)建议，尽量避免返回neutral
2. 即使信号不强，只要偏向一方就给出该方向建议，用confidence反映确定性
3. confidence范围：0.5-0.6表示弱信号，0.6-0.75表示中等信号，0.75+表示强信号
4. 模拟交易目的是验证策略，需要积极开仓测试

你必须以JSON格式回复，格式如下：
{"sentiment": "bullish"或"bearish", "confidence": 0.5-1.0, "analysis": "你的详细分析(中文)", "key_factors": ["因素1", "因素2"]}
只输出JSON，不要输出其他内容。"#;

    let response = client.chat_with_system(system_prompt, market_summary).await
        .map_err(|e| AppError::Internal(format!("LLM 分析失败: {}", e)))?;

    // Parse JSON response
    let cleaned = response.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|e| AppError::Internal(format!("解析 LLM 响应失败: {}", e)))?;

    Ok(LlmAnalysisResult {
        sentiment: parsed.get("sentiment").and_then(|v| v.as_str()).unwrap_or("neutral").to_string(),
        confidence: parsed.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.3),
        analysis: parsed.get("analysis").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        key_factors: parsed.get("key_factors")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
    })
}

async fn update_running_stats(pool: &sqlx::PgPool, config_id: Uuid) -> Result<()> {
    // Calculate running days
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE id = $1"
    )
    .bind(config_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Config not found".to_string()))?;

    let running_days = (Utc::now() - config.created_at).num_days() as i32;

    // Calculate max drawdown
    let peak_balance: f64 = sqlx::query_scalar(
        "SELECT MAX(current_balance) FROM (
            SELECT current_balance, updated_at FROM ai_simulation_configs WHERE id = $1
            UNION ALL
            SELECT initial_balance as current_balance, created_at as updated_at FROM ai_simulation_configs WHERE id = $1
        ) sub"
    )
    .bind(config_id)
    .fetch_one(pool)
    .await
    .unwrap_or(config.initial_balance);

    let max_drawdown = if peak_balance > 0.0 {
        ((peak_balance - config.current_balance) / peak_balance) * 100.0
    } else {
        0.0
    };

    // Calculate avg PnL
    let avg_pnl: f64 = sqlx::query_scalar(
        "SELECT COALESCE(AVG(pnl_percent), 0) FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed'"
    )
    .bind(config_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0.0);

    // Calculate profit/loss ratio
    let avg_win: f64 = sqlx::query_scalar(
        "SELECT COALESCE(AVG(pnl_percent), 0) FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed' AND pnl > 0"
    )
    .bind(config_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0.0);

    let avg_loss: f64 = sqlx::query_scalar(
        "SELECT COALESCE(AVG(ABS(pnl_percent)), 0.01) FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed' AND pnl < 0"
    )
    .bind(config_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0.01);

    let profit_loss_ratio = if avg_loss != 0.0 { avg_win / avg_loss } else { 0.0 };

    // Calculate consecutive stop losses
    let recent_trades: Vec<AiSimulationTrade> = sqlx::query_as(
        "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed' ORDER BY closed_at DESC LIMIT 20"
    )
    .bind(config_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut consecutive_losses = 0i32;
    for t in &recent_trades {
        if t.pnl.map_or(false, |p| p < 0.0) {
            consecutive_losses += 1;
        } else {
            break;
        }
    }

    sqlx::query(
        r#"UPDATE ai_simulation_configs SET
            running_days = $1,
            max_drawdown_percent = GREATEST(max_drawdown_percent, $2),
            avg_pnl_percent = $3,
            profit_loss_ratio = $4,
            consecutive_stop_losses = $5,
            updated_at = NOW()
        WHERE id = $6"#
    )
    .bind(running_days)
    .bind(max_drawdown)
    .bind(avg_pnl)
    .bind(profit_loss_ratio)
    .bind(consecutive_losses)
    .bind(config_id)
    .execute(pool)
    .await?;

    Ok(())
}

// ============ Dashboard Aggregation API ============

#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    pub has_config: bool,
    pub config: Option<AiSimulationConfig>,
    pub level_info: Option<LevelInfo>,
    pub open_positions: Vec<PositionWithPnl>,
    pub closed_trades: Vec<AiSimulationTrade>,
    pub promotion_eligibility: Option<PromotionEligibility>,
    pub level_requirements: Option<LevelRequirementsInfo>,
    pub recent_debate_sessions: Vec<DebateSessionSummary>,
    pub risk_confirmation_signed: bool,
    pub reflections: Vec<ReflectionItem>,
    pub agent_credibility: Vec<AgentCredibilityItem>,
    pub total_unrealized_pnl: f64,
    pub equity: f64,
}

#[derive(Debug, Serialize)]
pub struct ReflectionItem {
    pub category: String,
    pub insight: String,
    pub recommendation: String,
}

#[derive(Debug, Serialize)]
pub struct AgentCredibilityItem {
    pub agent_name: String,
    pub department: String,
    pub accuracy: f64,
    pub credibility_score: f64,
    pub total_analyses: i32,
}

#[derive(Debug, Serialize)]
pub struct PositionWithPnl {
    pub trade: AiSimulationTrade,
    pub current_price: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub unrealized_pnl_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct LevelInfo {
    pub current_level: i32,
    pub current_level_name: String,
    pub current_mode: String,
    pub next_level: Option<i32>,
    pub next_level_name: Option<String>,
    pub next_mode: Option<String>,
    pub progress_percent: f64,
    pub experience_points: i32,
    pub next_level_points: i32,
}

#[derive(Debug, Serialize)]
pub struct LevelRequirementsInfo {
    pub next_level: i32,
    pub min_trades: i32,
    pub min_win_rate: f64,
    pub min_profit_loss_ratio: f64,
    pub min_running_days: i32,
    pub max_drawdown_percent: f64,
    pub max_consecutive_losses: i32,
    pub current_trades: i32,
    pub current_win_rate: f64,
    pub current_profit_loss_ratio: f64,
    pub current_running_days: i32,
    pub current_drawdown_percent: f64,
    pub current_consecutive_losses: i32,
}

#[derive(Debug, Serialize)]
pub struct DebateSessionSummary {
    pub id: Uuid,
    pub symbol: String,
    pub status: String,
    pub created_at: chrono::DateTime<Utc>,
}

fn get_level_name(level: i32) -> String {
    match level {
        0 => "实习交易员".to_string(),
        1 => "初级交易员".to_string(),
        2 => "资深交易员".to_string(),
        3 => "基金经理".to_string(),
        _ => "基金经理".to_string(),
    }
}

fn get_mode_name(mode: &str) -> String {
    match mode {
        "paper" => "模拟盘".to_string(),
        "demo" => "Demo盘".to_string(),
        "live" => "实盘".to_string(),
        _ => mode.to_string(),
    }
}

fn mode_for_level(level: i32) -> String {
    match level {
        0 => "paper".to_string(),
        1 => "demo".to_string(),
        2 => "autonomous".to_string(),
        _ => "live".to_string(),
    }
}

async fn generate_reflections(pool: &sqlx::PgPool, user_id: i64, config: &AiSimulationConfig) -> Vec<ReflectionItem> {
    let mut reflections = Vec::new();

    // Overall performance reflection
    if config.total_trades > 0 {
        let win_rate = config.win_rate;
        reflections.push(ReflectionItem {
            category: "overall_performance".to_string(),
            insight: format!(
                "总交易{}笔，胜率{:.1}%，盈亏比{:.2}",
                config.total_trades, win_rate * 100.0, config.profit_loss_ratio,
            ),
            recommendation: if win_rate < 0.5 {
                "胜率偏低，建议提高置信度阈值或缩小仓位".to_string()
            } else if win_rate > 0.65 {
                "表现优秀，可考虑维持当前策略".to_string()
            } else {
                "表现中等，建议关注风险管理参数".to_string()
            },
        });
    }

    // Direction-specific reflection from decision_memory
    let direction_stats = sqlx::query(
        r#"SELECT action, COUNT(*) as cnt,
            SUM(CASE WHEN success THEN 1 ELSE 0 END) as wins,
            AVG(actual_pnl_percent) as avg_pnl
        FROM decision_memory
        WHERE user_id = $1 AND success IS NOT NULL
        GROUP BY action"#
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for row in &direction_stats {
        let action: String = row.get("action");
        let cnt: i64 = row.get("cnt");
        let wins: i64 = row.get("wins");
        let avg_pnl: Option<f64> = row.get("avg_pnl");
        let rate = if cnt > 0 { wins as f64 / cnt as f64 } else { 0.0 };
        reflections.push(ReflectionItem {
            category: format!("{}_performance", action),
            insight: format!(
                "{}方向: {}笔交易，胜率{:.1}%，平均盈亏{:.2}%",
                action, cnt, rate * 100.0, avg_pnl.unwrap_or(0.0),
            ),
            recommendation: if rate < 0.4 {
                format!("{}方向表现不佳，建议更谨慎", action)
            } else {
                format!("{}方向表现尚可", action)
            },
        });
    }

    // Stop loss reflection
    let sl_stats = sqlx::query(
        r#"SELECT close_reason, COUNT(*) as cnt,
            AVG(actual_pnl_percent) as avg_pnl
        FROM decision_memory
        WHERE user_id = $1 AND close_reason IS NOT NULL
        GROUP BY close_reason"#
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for row in &sl_stats {
        let reason: String = row.get("close_reason");
        let cnt: i64 = row.get("cnt");
        let avg_pnl: Option<f64> = row.get("avg_pnl");
        reflections.push(ReflectionItem {
            category: format!("close_{}", reason),
            insight: format!("{}平仓{}笔，平均盈亏{:.2}%", reason, cnt, avg_pnl.unwrap_or(0.0)),
            recommendation: if reason == "stop_loss" && cnt > 3 {
                "止损频繁，建议调整止损策略或入场时机".to_string()
            } else {
                String::new()
            },
        });
    }

    reflections
}

async fn fetch_agent_credibility(pool: &sqlx::PgPool) -> Vec<AgentCredibilityItem> {
    sqlx::query_as::<_, (String, String, f64, f64, i32)>(
        r#"SELECT agent_name, agent_department, accuracy, credibility_score, total_analyses
        FROM agent_performance WHERE total_analyses > 0
        ORDER BY credibility_score DESC"#
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(name, dept, accuracy, credibility, total)| AgentCredibilityItem {
        agent_name: name,
        department: dept,
        accuracy,
        credibility_score: credibility,
        total_analyses: total,
    })
    .collect()
}

async fn get_dashboard(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<DashboardResponse>> {
    // Get or create config
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?;

    let has_config = config.is_some();
    let risk_signed = config.as_ref().map(|c| c.risk_confirmation_signed).unwrap_or(false);
    let cfg = match config {
        Some(c) => c,
        None => {
            return Ok(Json(DashboardResponse {
                has_config: false,
                config: None,
                level_info: None,
                open_positions: vec![],
                closed_trades: vec![],
                promotion_eligibility: None,
                level_requirements: None,
                recent_debate_sessions: vec![],
                risk_confirmation_signed: false,
                reflections: vec![],
                agent_credibility: vec![],
                total_unrealized_pnl: 0.0,
                equity: 0.0,
            }));
        }
    };

    // Level info
    let next_level = PromotionSystem::get_next_level(cfg.level);
    let level_requirements = match next_level {
        Some(nl) => {
            let req = PromotionSystem::get_level_requirements(nl);
            Some(LevelRequirementsInfo {
                next_level: nl,
                min_trades: req.min_trades,
                min_win_rate: req.min_win_rate,
                min_profit_loss_ratio: req.min_profit_loss_ratio,
                min_running_days: req.min_running_days,
                max_drawdown_percent: req.max_drawdown_percent,
                max_consecutive_losses: req.max_consecutive_losses,
                current_trades: cfg.total_trades,
                current_win_rate: cfg.win_rate,
                current_profit_loss_ratio: cfg.profit_loss_ratio,
                current_running_days: cfg.running_days,
                current_drawdown_percent: cfg.max_drawdown_percent,
                current_consecutive_losses: cfg.consecutive_stop_losses,
            })
        }
        None => None,
    };

    // Calculate progress percent toward next level
    let progress_percent = match next_level {
        Some(nl) => {
            let req = PromotionSystem::get_level_requirements(nl);
            let trade_progress = if req.min_trades > 0 { (cfg.total_trades as f64 / req.min_trades as f64).min(1.0) } else { 1.0 };
            let win_rate_progress = if req.min_win_rate > 0.0 { (cfg.win_rate / req.min_win_rate).min(1.0) } else { 1.0 };
            let days_progress = if req.min_running_days > 0 { (cfg.running_days as f64 / req.min_running_days as f64).min(1.0) } else { 1.0 };
            (trade_progress + win_rate_progress + days_progress) / 3.0 * 100.0
        }
        None => 100.0,
    };

    let experience_points = cfg.total_trades * 10 + (cfg.win_rate * 100.0) as i32 * 5 + cfg.running_days * 2;
    let next_level_points = match next_level {
        Some(nl) => {
            let req = PromotionSystem::get_level_requirements(nl);
            req.min_trades * 10 + (req.min_win_rate * 100.0) as i32 * 5 + req.min_running_days * 2
        }
        None => experience_points,
    };

    let level_info = LevelInfo {
        current_level: cfg.level,
        current_level_name: get_level_name(cfg.level),
        current_mode: get_mode_name(&cfg.mode),
        next_level,
        next_level_name: next_level.map(get_level_name),
        next_mode: next_level.map(|nl| get_mode_name(&mode_for_level(nl))),
        progress_percent: progress_percent.min(100.0),
        experience_points,
        next_level_points,
    };

    // Open positions with real-time PnL
    let open_trades = sqlx::query_as::<_, AiSimulationTrade>(
        "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'open' ORDER BY opened_at DESC"
    )
    .bind(cfg.id)
    .fetch_all(&state.db_pool)
    .await?;

    let mut open_positions: Vec<PositionWithPnl> = Vec::new();
    for trade in &open_trades {
        let current_price = get_current_price(&state, user.user_id, &trade.symbol).await;
        let (unrealized_pnl, unrealized_pnl_percent) = match (current_price, trade.quantity) {
            (Some(cp), qty) if qty > 0.0 => {
                let diff = if trade.direction == "long" {
                    cp - trade.entry_price
                } else {
                    trade.entry_price - cp
                };
                let pnl = diff * qty * trade.leverage as f64;
                let pnl_pct = (diff / trade.entry_price) * 100.0 * trade.leverage as f64;
                (Some(pnl), Some(pnl_pct))
            }
            _ => (None, None),
        };
        open_positions.push(PositionWithPnl {
            trade: trade.clone(),
            current_price,
            unrealized_pnl,
            unrealized_pnl_percent,
        });
    }

    // Closed trades
    let closed_trades = sqlx::query_as::<_, AiSimulationTrade>(
        "SELECT * FROM ai_simulation_trades WHERE config_id = $1 AND status = 'closed' ORDER BY closed_at DESC LIMIT 20"
    )
    .bind(cfg.id)
    .fetch_all(&state.db_pool)
    .await?;

    // Promotion eligibility
    let promotion_system = PromotionSystem::new(state.db_pool.clone());
    let promotion_eligibility = promotion_system.check_promotion_eligibility(&cfg).await.ok();

    // Recent debate sessions
    let session_rows = sqlx::query(
        "SELECT id, symbol, status, created_at FROM debate_sessions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 5"
    )
    .bind(user.user_id)
    .fetch_all(&state.db_pool)
    .await?;

    let recent_debate_sessions: Vec<DebateSessionSummary> = session_rows.iter().map(|row| {
        DebateSessionSummary {
            id: row.get("id"),
            symbol: row.get("symbol"),
            status: row.get("status"),
            created_at: row.get("created_at"),
        }
    }).collect();

    // Compute reflections and credibility before moving cfg
    let reflections = generate_reflections(&state.db_pool, user.user_id, &cfg).await;
    let agent_credibility = fetch_agent_credibility(&state.db_pool).await;

    // Calculate total unrealized PnL from open positions
    let total_unrealized_pnl: f64 = open_positions.iter()
        .filter_map(|p| p.unrealized_pnl)
        .sum();
    let equity = cfg.current_balance + total_unrealized_pnl;

    // Recalculate win_rate from closed trades for accuracy
    let closed_count = closed_trades.len() as i32;
    let closed_wins = closed_trades.iter().filter(|t| t.pnl.map_or(false, |p| p > 0.0)).count() as i32;
    let recalculated_win_rate = if closed_count > 0 {
        closed_wins as f64 / closed_count as f64
    } else {
        cfg.win_rate
    };

    // Update config stats if they seem stale (win_rate mismatch)
    if (recalculated_win_rate - cfg.win_rate).abs() > 0.01 && closed_count > 0 {
        let _ = sqlx::query(
            r#"UPDATE ai_simulation_configs SET
                win_rate = $1,
                winning_trades = $2,
                losing_trades = $3,
                total_trades = (SELECT COUNT(*) FROM ai_simulation_trades WHERE config_id = $4),
                updated_at = NOW()
            WHERE id = $4"#
        )
        .bind(recalculated_win_rate)
        .bind(closed_wins)
        .bind(closed_count - closed_wins)
        .bind(cfg.id)
        .execute(&state.db_pool)
        .await;
    }

    // Build a config with updated equity for display
    let mut display_cfg = cfg.clone();
    display_cfg.win_rate = recalculated_win_rate;

    Ok(Json(DashboardResponse {
        has_config: true,
        config: Some(display_cfg),
        level_info: Some(level_info),
        open_positions,
        closed_trades,
        promotion_eligibility,
        level_requirements,
        recent_debate_sessions,
        risk_confirmation_signed: risk_signed,
        reflections,
        agent_credibility,
        total_unrealized_pnl,
        equity,
    }))
}

async fn initiate_promotion(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let config = sqlx::query_as::<_, AiSimulationConfig>(
        "SELECT * FROM ai_simulation_configs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user.user_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No simulation config found".to_string()))?;

    let promotion_system = PromotionSystem::new(state.db_pool.clone());
    let audit = promotion_system.initiate_promotion(config.id).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "audit_id": audit.id,
        "from_level": audit.from_level,
        "to_level": audit.to_level,
        "status": audit.status,
    })))
}
