//! Model Cards & Counterfactual Explanations REST API
//! 模型卡与反事实解释 API
//!
//! 依据《系统评估与演进规划》第四阶段任务3：
//!   "模型卡、校准曲线、反事实解释"
//!
//! 提供端点：
//! - GET    /model-cards                       查询模型卡列表
//! - GET    /model-cards/{model_version}       查询单个模型卡详情
//! - POST   /model-cards                       创建/聚合生成模型卡
//! - POST   /model-cards/{model_version}/promote  发布门禁：变更状态
//! - POST   /model-cards/{model_version}/rollback 回滚到之前的版本
//! - POST   /counterfactuals/generate          为指定交易生成反事实解释
//! - GET    /counterfactuals/attribution/{id} 查询某笔交易的反事实场景
//! - GET    /counterfactuals/job/{job_id}      查询某 job 下所有反事实场景

use crate::backtest::counterfactual::{
    self, CounterfactualExplanation, CounterfactualInput, ScenarioType,
};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::signals::model_card::{
    self, ModelCard, ModelCardInput, ModelCardStatus,
};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        // 模型卡端点
        .route("/model-cards", get(list_model_cards).post(create_model_card))
        .route("/model-cards/{model_version}", get(get_model_card))
        .route(
            "/model-cards/{model_version}/promote",
            post(promote_model_card),
        )
        .route(
            "/model-cards/{model_version}/rollback",
            post(rollback_model_card),
        )
        // 反事实解释端点
        .route("/counterfactuals/generate", post(generate_counterfactuals))
        .route(
            "/counterfactuals/attribution/{attribution_id}",
            get(list_counterfactuals_by_attribution),
        )
        .route(
            "/counterfactuals/job/{job_id}",
            get(list_counterfactuals_by_job),
        )
}

// ============================================================================
// 模型卡 API
// ============================================================================

#[derive(Debug, Deserialize)]
struct ListModelCardsQuery {
    status: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// 查询模型卡列表
async fn list_model_cards(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(q): Query<ListModelCardsQuery>,
) -> Result<Json<serde_json::Value>> {
    let cards = model_card::list_cards(&state.db_pool, q.status.as_deref(), q.limit)
        .await
        .map_err(|e| AppError::Database(e))?;

    let cards_json: Vec<serde_json::Value> = cards
        .iter()
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();

    Ok(Json(serde_json::json!({
        "cards": cards_json,
        "count": cards_json.len()
    })))
}

/// 查询单个模型卡
async fn get_model_card(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(model_version): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let card = model_card::get_card(&state.db_pool, &model_version)
        .await
        .map_err(|e| AppError::Database(e))?
        .ok_or_else(|| AppError::NotFound(format!("model card not found: {}", model_version)))?;

    Ok(Json(serde_json::to_value(&card).unwrap_or_default()))
}

/// 创建模型卡请求
#[derive(Debug, Deserialize)]
struct CreateModelCardRequest {
    model_version: String,
    model_type: String,
    model_name: String,
    description: Option<String>,
    intended_use: Option<String>,
    out_of_scope: Option<String>,
    training_data_summary: Option<serde_json::Value>,
    feature_version: Option<String>,
    features_used: Option<serde_json::Value>,
    invalidation_conditions: Option<serde_json::Value>,
    known_limitations: Option<serde_json::Value>,
    ethical_considerations: Option<String>,
}

/// 创建/聚合生成模型卡
///
/// 从已有的校准报告、信任评估、预测统计聚合生成模型卡
async fn create_model_card(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateModelCardRequest>,
) -> Result<Json<serde_json::Value>> {
    let input = ModelCardInput {
        model_version: req.model_version.clone(),
        model_type: req.model_type,
        model_name: req.model_name,
        description: req.description,
        intended_use: req.intended_use,
        out_of_scope: req.out_of_scope,
        training_data_summary: req.training_data_summary,
        feature_version: req.feature_version,
        features_used: req.features_used,
        invalidation_conditions: req.invalidation_conditions,
        known_limitations: req.known_limitations,
        ethical_considerations: req.ethical_considerations,
        created_by: Some(user.user_id as i64),
    };

    // 聚合已有数据生成模型卡
    let card = model_card::aggregate_from_existing(&state.db_pool, &input)
        .await
        .map_err(|e| AppError::Database(e))?;

    // 保存到数据库
    model_card::save_card(&state.db_pool, &card)
        .await
        .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "card_id": card.card_id.to_string(),
        "model_version": card.model_version,
        "status": card.status.as_str(),
        "promotion_eligible": card.promotion_eligible,
        "brier_score": card.brier_score,
        "log_loss": card.log_loss,
        "accuracy": card.accuracy,
        "calibration_report_id": card.calibration_report_id.map(|u| u.to_string()),
        "trust_assessment_id": card.trust_assessment_id.map(|u| u.to_string()),
    })))
}

/// 发布门禁请求
#[derive(Debug, Deserialize)]
struct PromoteRequest {
    new_status: String,
}

/// 发布门禁：变更模型卡状态
///
/// 状态转换规则：
/// - draft -> shadow: 需要 promotion_eligible = true
/// - shadow -> active: 需要设置 shadow_period_end
/// - active -> deprecated: 直接允许
/// - active -> rolled_back: 需要 previous_version 存在
async fn promote_model_card(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(model_version): Path<String>,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<serde_json::Value>> {
    let new_status = ModelCardStatus::from_str(&req.new_status)
        .ok_or_else(|| AppError::Validation(format!("invalid status: {}", req.new_status)))?;

    let card = model_card::promote_card(
        &state.db_pool,
        &model_version,
        new_status,
        Some(user.user_id as i64),
    )
    .await
    .map_err(|e| AppError::Validation(e))?;

    Ok(Json(serde_json::json!({
        "card_id": card.card_id.to_string(),
        "model_version": card.model_version,
        "status": card.status.as_str(),
        "approved_by": card.approved_by,
        "approved_at": card.approved_at.map(|t| t.to_rfc3339()),
    })))
}

/// 回滚模型卡到之前的版本
async fn rollback_model_card(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(model_version): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let card = model_card::promote_card(
        &state.db_pool,
        &model_version,
        ModelCardStatus::RolledBack,
        Some(user.user_id as i64),
    )
    .await
    .map_err(|e| AppError::Validation(e))?;

    Ok(Json(serde_json::json!({
        "card_id": card.card_id.to_string(),
        "model_version": card.model_version,
        "status": card.status.as_str(),
        "previous_version": card.previous_version,
        "message": "model rolled back to previous version"
    })))
}

// ============================================================================
// 反事实解释 API
// ============================================================================

/// 生成反事实解释请求
#[derive(Debug, Deserialize)]
struct GenerateCounterfactualRequest {
    attribution_id: Option<Uuid>,
    decision_card_id: Option<Uuid>,
    job_id: Option<Uuid>,
    symbol: String,
    direction: String,
    actual_pnl: f64,
    gross_pnl: f64,
    entry_time: DateTime<Utc>,
    exit_time: Option<DateTime<Utc>>,
    holding_period_sec: Option<i32>,
    fee_cost: f64,
    slippage_cost: f64,
    funding_cost: f64,
    impact_cost: f64,
    benchmark_return: Option<f64>,
    market_regime: Option<String>,
    signal_confidence: Option<f64>,
    /// 是否保存到数据库
    #[serde(default = "default_save")]
    save: bool,
}

fn default_save() -> bool {
    true
}

/// 为指定交易生成反事实解释
///
/// 对每笔交易生成 5 种反事实场景：
/// - no_trade: 若不交易
/// - earlier_exit: 若提前退出
/// - later_exit: 若延后退出
/// - opposite_direction: 若反向操作
/// - reduced_size: 若减半仓位
async fn generate_counterfactuals(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<GenerateCounterfactualRequest>,
) -> Result<Json<serde_json::Value>> {
    let input = CounterfactualInput {
        attribution_id: req.attribution_id,
        decision_card_id: req.decision_card_id,
        job_id: req.job_id,
        user_id: Some(user.user_id as i64),
        symbol: req.symbol.clone(),
        direction: req.direction,
        actual_pnl: req.actual_pnl,
        gross_pnl: req.gross_pnl,
        entry_time: req.entry_time,
        exit_time: req.exit_time,
        holding_period_sec: req.holding_period_sec,
        fee_cost: req.fee_cost,
        slippage_cost: req.slippage_cost,
        funding_cost: req.funding_cost,
        impact_cost: req.impact_cost,
        benchmark_return: req.benchmark_return,
        market_regime: req.market_regime,
        signal_confidence: req.signal_confidence,
    };

    let explanations = counterfactual::generate_counterfactuals(&input);

    // 保存到数据库
    if req.save {
        counterfactual::save_explanations(&state.db_pool, &explanations)
            .await
            .map_err(|e| AppError::Database(e))?;
    }

    let explanations_json: Vec<serde_json::Value> = explanations
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or_default())
        .collect();

    Ok(Json(serde_json::json!({
        "explanations": explanations_json,
        "count": explanations_json.len(),
        "symbol": req.symbol,
        "actual_pnl": req.actual_pnl
    })))
}

/// 查询某笔交易的反事实场景
async fn list_counterfactuals_by_attribution(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(attribution_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let explanations = counterfactual::list_by_attribution(&state.db_pool, attribution_id)
        .await
        .map_err(|e| AppError::Database(e))?;

    let explanations_json: Vec<serde_json::Value> = explanations
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or_default())
        .collect();

    Ok(Json(serde_json::json!({
        "explanations": explanations_json,
        "count": explanations_json.len(),
        "attribution_id": attribution_id.to_string()
    })))
}

/// 查询某 job 下所有反事实场景
async fn list_counterfactuals_by_job(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let explanations = counterfactual::list_by_job(&state.db_pool, job_id)
        .await
        .map_err(|e| AppError::Database(e))?;

    let explanations_json: Vec<serde_json::Value> = explanations
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or_default())
        .collect();

    Ok(Json(serde_json::json!({
        "explanations": explanations_json,
        "count": explanations_json.len(),
        "job_id": job_id.to_string()
    })))
}
