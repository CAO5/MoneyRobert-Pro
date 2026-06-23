//! Signal & Decision Card REST API
//! 概率信号与决策卡 API
//!
//! 提供端点：
//! - POST /signals/decision-card          生成概率决策卡
//! - GET  /signals/decision-cards          查询用户决策卡列表
//! - GET  /signals/calibration             查询概率校准报告
//! - POST /signals/calibration/compute     触发校准计算

use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::signals::{
    compute_brier_score, compute_calibration_curve, compute_log_loss, models::SuggestedAction,
    CalibrationReport, DecisionCardBuilder, SignalStore,
};
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/decision-card", post(create_decision_card))
        .route("/decision-cards", get(list_decision_cards))
        .route("/calibration", get(get_calibration))
        .route("/calibration/compute", post(compute_calibration))
}

/// 创建决策卡请求
#[derive(Debug, Deserialize)]
struct CreateDecisionCardRequest {
    symbol: String,
    /// 预测周期（秒）
    target_horizon_sec: i32,
    /// 概率分布
    p_up: f64,
    p_down: f64,
    p_flat: f64,
    /// 收益分位数
    q10: Option<f64>,
    q50: Option<f64>,
    q90: Option<f64>,
    /// 预期波动率
    expected_volatility: Option<f64>,
    /// 模型版本
    model_version: String,
    /// 市场状态
    market_regime: Option<String>,
    /// 净期望 EV（扣除费用/滑点/资金费率后）
    expected_value: f64,
    /// 仓位建议（0-1）
    position_suggestion: f64,
    /// 最坏情形（CVaR 口径）
    worst_case: Option<f64>,
    /// 已用风险预算
    risk_budget_used: Option<f64>,
    /// 数据新鲜度（秒）
    data_freshness_sec: Option<f64>,
    /// 支持证据
    supporting_evidence: Option<serde_json::Value>,
    /// 反对证据
    opposing_evidence: Option<serde_json::Value>,
    /// 样本表现
    sample_performance: Option<serde_json::Value>,
    /// 数据血缘
    data_lineage: Option<serde_json::Value>,
    /// 失效条件
    invalidation_conditions: Option<serde_json::Value>,
}

/// 决策卡响应
#[derive(Debug, Serialize)]
struct DecisionCardResponse {
    card_id: String,
    symbol: String,
    generated_at: String,
    suggested_action: String,
    target_horizon_sec: i32,
    p_up: f64,
    p_down: f64,
    p_flat: f64,
    q10: Option<f64>,
    q50: Option<f64>,
    q90: Option<f64>,
    expected_value: f64,
    worst_case: Option<f64>,
    position_suggestion: f64,
    risk_budget_used: Option<f64>,
    applicable_regime: Option<String>,
    data_freshness_sec: Option<f64>,
    invalidation_conditions: Option<serde_json::Value>,
    model_version: String,
}

impl From<crate::signals::models::DecisionCard> for DecisionCardResponse {
    fn from(c: crate::signals::models::DecisionCard) -> Self {
        Self {
            card_id: c.card_id.to_string(),
            symbol: c.symbol,
            generated_at: c.generated_at.to_rfc3339(),
            suggested_action: c.suggested_action.as_str().to_string(),
            target_horizon_sec: c.target_horizon_sec,
            p_up: c.p_up,
            p_down: c.p_down,
            p_flat: c.p_flat,
            q10: c.q10,
            q50: c.q50,
            q90: c.q90,
            expected_value: c.expected_value,
            worst_case: c.worst_case,
            position_suggestion: c.position_suggestion,
            risk_budget_used: c.risk_budget_used,
            applicable_regime: c.applicable_regime,
            data_freshness_sec: c.data_freshness_sec,
            invalidation_conditions: c.invalidation_conditions,
            model_version: c.model_version,
        }
    }
}

/// 生成概率决策卡
/// 把每次交易变成"概率分布 + EV + CVaR + 失效条件 + 数据血缘"的可审计对象
async fn create_decision_card(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateDecisionCardRequest>,
) -> Result<Json<DecisionCardResponse>> {
    // 验证概率分布
    let prob_sum = req.p_up + req.p_down + req.p_flat;
    if (prob_sum - 1.0).abs() > 0.05 {
        return Err(AppError::Validation(format!(
            "概率之和应为 1.0，实际: {}",
            prob_sum
        )));
    }
    if req.p_up < 0.0 || req.p_down < 0.0 || req.p_flat < 0.0 {
        return Err(AppError::Validation(
            "概率不能为负数".to_string(),
        ));
    }
    if req.position_suggestion < 0.0 || req.position_suggestion > 1.0 {
        return Err(AppError::Validation(
            "仓位建议应在 0-1 之间".to_string(),
        ));
    }

    // 构建概率预测
    let prediction_id = Uuid::new_v4();
    let now = Utc::now();
    let prediction = crate::signals::models::SignalPrediction {
        prediction_id,
        symbol: req.symbol.clone(),
        prediction_time: now,
        target_horizon_sec: req.target_horizon_sec,
        p_up: req.p_up,
        p_down: req.p_down,
        p_flat: req.p_flat,
        q10: req.q10,
        q50: req.q50,
        q90: req.q90,
        expected_volatility: req.expected_volatility,
        mae_estimate: None,
        uncertainty: None,
        model_version: req.model_version.clone(),
        model_type: "unknown".into(),
        feature_version: None,
        features_used: None,
        market_regime: req.market_regime.clone(),
        realized_return: None,
        realized_direction: None,
        evaluated_at: None,
        created_at: now,
    };

    // 保存预测
    SignalStore::insert_prediction(&state.db_pool, &prediction)
        .await
        .map_err(|e| AppError::Internal(format!("insert prediction failed: {}", e)))?;

    // 根据概率分布和 EV 决定建议动作
    let ev_min = 0.0; // 净期望阈值
    let builder = DecisionCardBuilder::new(&req.symbol, prediction)
        .with_user_id(user.user_id)
        .with_model_version(&req.model_version);
    let action = builder.decide_action(req.expected_value, ev_min);

    // 构建决策卡
    let card = builder.build(
        req.expected_value,
        action,
        req.position_suggestion,
        req.worst_case,
        req.risk_budget_used,
        req.data_freshness_sec,
        req.supporting_evidence,
        req.opposing_evidence,
        req.sample_performance,
        req.data_lineage,
        req.invalidation_conditions,
    );

    // 保存决策卡
    SignalStore::insert_decision_card(&state.db_pool, &card)
        .await
        .map_err(|e| AppError::Internal(format!("insert decision card failed: {}", e)))?;

    Ok(Json(card.into()))
}

/// 查询用户决策卡列表
#[derive(Debug, Deserialize)]
struct ListCardsParams {
    limit: Option<i64>,
}

async fn list_decision_cards(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListCardsParams>,
) -> Result<Json<Vec<DecisionCardResponse>>> {
    let limit = params.limit.unwrap_or(20).min(100);
    let cards = SignalStore::list_decision_cards_by_user(&state.db_pool, user.user_id, limit)
        .await
        .map_err(|e| AppError::Internal(format!("list decision cards failed: {}", e)))?;

    Ok(Json(cards.into_iter().map(Into::into).collect()))
}

/// 查询概率校准报告
#[derive(Debug, Deserialize)]
struct GetCalibrationParams {
    model_version: String,
}

#[derive(Debug, Serialize)]
struct CalibrationResponse {
    report_id: String,
    model_version: String,
    symbol: Option<String>,
    market_regime: Option<String>,
    eval_start: String,
    eval_end: String,
    brier_score: f64,
    log_loss: f64,
    accuracy: f64,
    calibration_error: Option<f64>,
    calibration_curve: serde_json::Value,
    sample_count: i32,
    is_well_calibrated: bool,
    degradation_detected: bool,
}

async fn get_calibration(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<GetCalibrationParams>,
) -> Result<Json<CalibrationResponse>> {
    let report = SignalStore::get_latest_calibration(&state.db_pool, &params.model_version)
        .await
        .map_err(|e| AppError::Internal(format!("get calibration failed: {}", e)))?
        .ok_or_else(|| AppError::NotFound("校准报告不存在".to_string()))?;

    Ok(Json(CalibrationResponse {
        report_id: report.report_id.to_string(),
        model_version: report.model_version,
        symbol: report.symbol,
        market_regime: report.market_regime,
        eval_start: report.eval_start.to_rfc3339(),
        eval_end: report.eval_end.to_rfc3339(),
        brier_score: report.brier_score,
        log_loss: report.log_loss,
        accuracy: report.accuracy,
        calibration_error: report.calibration_error,
        calibration_curve: report.calibration_curve,
        sample_count: report.sample_count,
        is_well_calibrated: report.is_well_calibrated,
        degradation_detected: report.degradation_detected,
    }))
}

/// 触发校准计算
#[derive(Debug, Deserialize)]
struct ComputeCalibrationRequest {
    model_version: String,
    symbol: Option<String>,
    /// 评估时间范围
    start_time: String,
    end_time: String,
}

async fn compute_calibration(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ComputeCalibrationRequest>,
) -> Result<Json<CalibrationResponse>> {
    let start: DateTime<Utc> = req
        .start_time
        .parse()
        .map_err(|e| AppError::Validation(format!("invalid start_time: {}", e)))?;
    let end: DateTime<Utc> = req
        .end_time
        .parse()
        .map_err(|e| AppError::Validation(format!("invalid end_time: {}", e)))?;

    // 查询已评估的预测
    let preds = SignalStore::query_evaluated_predictions(
        &state.db_pool,
        &req.model_version,
        req.symbol.as_deref(),
        start,
        end,
    )
    .await
    .map_err(|e| AppError::Internal(format!("query predictions failed: {}", e)))?;

    if preds.is_empty() {
        return Err(AppError::NotFound("无已评估的预测数据".to_string()));
    }

    // 提取预测概率和实际结果
    // 使用 p_up 作为"上涨"的预测概率，实际方向为 up 时为 true
    let predictions: Vec<f64> = preds.iter().map(|p| p.p_up).collect();
    let outcomes: Vec<bool> = preds
        .iter()
        .map(|p| p.realized_direction.as_deref() == Some("up"))
        .collect();

    // 计算校准指标
    let brier = compute_brier_score(&predictions, &outcomes);
    let log_loss = compute_log_loss(&predictions, &outcomes);
    let accuracy = outcomes.iter().filter(|o| **o).count() as f64 / outcomes.len() as f64;
    let curve = compute_calibration_curve(&predictions, &outcomes, 10);
    let calibration_error = Some(
        crate::signals::calibration::compute_calibration_error(&curve),
    );

    // 统计样本
    let up_count = preds
        .iter()
        .filter(|p| p.realized_direction.as_deref() == Some("up"))
        .count() as i32;
    let down_count = preds
        .iter()
        .filter(|p| p.realized_direction.as_deref() == Some("down"))
        .count() as i32;
    let flat_count = preds
        .iter()
        .filter(|p| p.realized_direction.as_deref() == Some("flat"))
        .count() as i32;

    // 判断校准质量
    let is_well_calibrated = brier < 0.25 && calibration_error.unwrap_or(1.0) < 0.1;

    let report = CalibrationReport {
        report_id: Uuid::new_v4(),
        model_version: req.model_version.clone(),
        symbol: req.symbol.clone(),
        market_regime: None,
        eval_start: start,
        eval_end: end,
        brier_score: brier,
        log_loss,
        accuracy,
        calibration_error,
        calibration_curve: serde_json::to_value(&curve).unwrap_or(serde_json::json!([])),
        sample_count: preds.len() as i32,
        up_count,
        down_count,
        flat_count,
        is_well_calibrated,
        degradation_detected: !is_well_calibrated && brier > 0.33,
        metadata: None,
        created_at: Utc::now(),
    };

    // 保存校准报告
    SignalStore::insert_calibration_report(&state.db_pool, &report)
        .await
        .map_err(|e| AppError::Internal(format!("insert calibration report failed: {}", e)))?;

    Ok(Json(CalibrationResponse {
        report_id: report.report_id.to_string(),
        model_version: report.model_version,
        symbol: report.symbol,
        market_regime: report.market_regime,
        eval_start: report.eval_start.to_rfc3339(),
        eval_end: report.eval_end.to_rfc3339(),
        brier_score: report.brier_score,
        log_loss: report.log_loss,
        accuracy: report.accuracy,
        calibration_error: report.calibration_error,
        calibration_curve: report.calibration_curve,
        sample_count: report.sample_count,
        is_well_calibrated: report.is_well_calibrated,
        degradation_detected: report.degradation_detected,
    }))
}
