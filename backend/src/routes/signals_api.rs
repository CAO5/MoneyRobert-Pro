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
use crate::signals::calibration::CalibrationModel;
use crate::signals::{
    calibrate_three_class, compute_brier_score, compute_calibration_curve, compute_log_loss,
    models::SuggestedAction, CalibrationReport, DecisionCardBuilder, SignalStore,
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
        .route("/trade-recommendation", post(generate_trade_recommendation))
        .route("/calibration", get(get_calibration))
        .route("/calibration/compute", post(compute_calibration))
        // 校准模型管理
        .route("/calibration/models", get(list_calibration_models))
        .route("/calibration/models/{model_id}", get(get_calibration_model))
        .route("/calibration/models/train", post(train_calibration_model))
        .route("/calibration/models/{model_id}/default", post(set_default_calibration_model))
        .route("/calibration/models/{model_id}/deprecate", post(deprecate_calibration_model))
        .route("/calibration/apply", post(apply_calibration))
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

// =========================================================
// 校准模型管理 API
// =========================================================

/// 校准模型响应
#[derive(Debug, Serialize)]
struct CalibrationModelResponse {
    model_id: String,
    model_name: String,
    model_type: String,
    symbol: Option<String>,
    market_regime: Option<String>,
    target_horizon_sec: Option<i32>,
    source_model_version: Option<String>,
    platt_a: Option<f64>,
    platt_b: Option<f64>,
    linear_factor: Option<f64>,
    linear_bias: Option<f64>,
    isotonic_points: Option<serde_json::Value>,
    sample_count: i32,
    train_brier_score: Option<f64>,
    train_calibration_error: Option<f64>,
    status: String,
    is_default: bool,
    created_at: String,
    updated_at: String,
}

impl From<crate::signals::calibration::store::CalibrationModelEntity>
    for CalibrationModelResponse
{
    fn from(m: crate::signals::calibration::store::CalibrationModelEntity) -> Self {
        Self {
            model_id: m.model_id.to_string(),
            model_name: m.model_name,
            model_type: m.model_type,
            symbol: m.symbol,
            market_regime: m.market_regime,
            target_horizon_sec: m.target_horizon_sec,
            source_model_version: m.source_model_version,
            platt_a: m.platt_a,
            platt_b: m.platt_b,
            linear_factor: m.linear_factor,
            linear_bias: m.linear_bias,
            isotonic_points: m.isotonic_points,
            sample_count: m.sample_count,
            train_brier_score: m.train_brier_score,
            train_calibration_error: m.train_calibration_error,
            status: m.status,
            is_default: m.is_default,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

/// 查询校准模型列表
#[derive(Debug, Deserialize)]
struct ListCalibrationModelsParams {
    symbol: Option<String>,
    market_regime: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

async fn list_calibration_models(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListCalibrationModelsParams>,
) -> Result<Json<Vec<CalibrationModelResponse>>> {
    let limit = params.limit.unwrap_or(20).min(100);
    use crate::signals::calibration::store::CalibrationStore;

    let models = CalibrationStore::list_models(
        &state.db_pool,
        params.symbol.as_deref(),
        params.market_regime.as_deref(),
        params.status.as_deref(),
        limit,
    )
    .await
    .map_err(|e| AppError::Internal(format!("list calibration models failed: {}", e)))?;

    Ok(Json(models.into_iter().map(Into::into).collect()))
}

/// 查询单个校准模型
async fn get_calibration_model(
    _user: CurrentUser,
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Result<Json<CalibrationModelResponse>> {
    let uuid = Uuid::parse_str(&model_id)
        .map_err(|e| AppError::Validation(format!("invalid model_id: {}", e)))?;

    use crate::signals::calibration::store::CalibrationStore;
    let model = CalibrationStore::get_model_by_id(&state.db_pool, uuid)
        .await
        .map_err(|e| AppError::Internal(format!("get calibration model failed: {}", e)))?
        .ok_or_else(|| AppError::NotFound("校准模型不存在".to_string()))?;

    Ok(Json(model.into()))
}

/// 训练校准模型请求
#[derive(Debug, Deserialize)]
struct TrainCalibrationModelRequest {
    model_name: String,
    model_type: String,
    source_model_version: String,
    symbol: Option<String>,
    market_regime: Option<String>,
    target_horizon_sec: Option<i32>,
}

async fn train_calibration_model(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<TrainCalibrationModelRequest>,
) -> Result<Json<CalibrationModelResponse>> {
    let model_type = match req.model_type.as_str() {
        "platt" => CalibrationModel::Platt,
        "isotonic" => CalibrationModel::Isotonic,
        "linear" => CalibrationModel::Linear,
        other => {
            return Err(AppError::Validation(format!(
                "不支持的校准模型类型: {}，可选: platt, isotonic, linear",
                other
            )))
        }
    };

    use crate::signals::calibration::store::CalibrationStore;
    let model_id = CalibrationStore::train_from_predictions(
        &state.db_pool,
        &req.source_model_version,
        req.symbol.as_deref(),
        req.market_regime.as_deref(),
        req.target_horizon_sec,
        model_type,
        &req.model_name,
    )
    .await
    .map_err(|e| AppError::Validation(format!("训练校准模型失败: {}", e)))?;

    let model = CalibrationStore::get_model_by_id(&state.db_pool, model_id)
        .await
        .map_err(|e| AppError::Internal(format!("get calibration model failed: {}", e)))?
        .ok_or_else(|| AppError::Internal("校准模型保存后未找到".to_string()))?;

    Ok(Json(model.into()))
}

/// 设置为默认校准模型
async fn set_default_calibration_model(
    _user: CurrentUser,
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>> {
    let uuid = Uuid::parse_str(&model_id)
        .map_err(|e| AppError::Validation(format!("invalid model_id: {}", e)))?;

    use crate::signals::calibration::store::CalibrationStore;
    CalibrationStore::set_default(&state.db_pool, uuid)
        .await
        .map_err(|e| AppError::Internal(format!("set default failed: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "已设为默认校准模型"
    })))
}

/// 停用校准模型
async fn deprecate_calibration_model(
    _user: CurrentUser,
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>> {
    let uuid = Uuid::parse_str(&model_id)
        .map_err(|e| AppError::Validation(format!("invalid model_id: {}", e)))?;

    use crate::signals::calibration::store::CalibrationStore;
    CalibrationStore::deprecate_model(&state.db_pool, uuid)
        .await
        .map_err(|e| AppError::Internal(format!("deprecate failed: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "校准模型已停用"
    })))
}

/// 应用校准请求
#[derive(Debug, Deserialize)]
struct ApplyCalibrationRequest {
    p_up: f64,
    p_down: f64,
    p_flat: f64,
    model_id_up: Option<String>,
    model_id_down: Option<String>,
    symbol: Option<String>,
    market_regime: Option<String>,
    model_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApplyCalibrationResponse {
    original_p_up: f64,
    original_p_down: f64,
    original_p_flat: f64,
    calibrated_p_up: f64,
    calibrated_p_down: f64,
    calibrated_p_flat: f64,
    delta_up: f64,
    delta_down: f64,
    model_id_up: Option<String>,
    model_id_down: Option<String>,
}

async fn apply_calibration(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<ApplyCalibrationRequest>,
) -> Result<Json<ApplyCalibrationResponse>> {
    use crate::signals::calibration::store::CalibrationStore;
    use crate::signals::calibration::FittedCalibration;

    let identity = FittedCalibration::Linear(crate::signals::calibration::LinearParams {
        factor: 1.0,
        bias: 0.0,
    });

    let (model_up, model_id_up) = if let Some(id) = &req.model_id_up {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| AppError::Validation(format!("invalid model_id_up: {}", e)))?;
        let m = CalibrationStore::get_model_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| AppError::Internal(format!("get model_up failed: {}", e)))?
            .ok_or_else(|| AppError::NotFound("上涨校准模型不存在".to_string()))?;
        let fitted = m
            .to_fitted()
            .ok_or_else(|| AppError::Validation("上涨校准模型参数不完整".to_string()))?;
        (fitted, Some(m.model_id.to_string()))
    } else {
        let m = CalibrationStore::get_default_model(
            &state.db_pool,
            req.symbol.as_deref(),
            req.market_regime.as_deref(),
            req.model_type.as_deref(),
        )
        .await
        .map_err(|e| AppError::Internal(format!("get default model failed: {}", e)))?;

        if let Some(m) = m {
            let fitted = m
                .to_fitted()
                .ok_or_else(|| AppError::Validation("默认校准模型参数不完整".to_string()))?;
            (fitted, Some(m.model_id.to_string()))
        } else {
            (identity.clone(), None)
        }
    };

    let (model_down, model_id_down) = if let Some(id) = &req.model_id_down {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| AppError::Validation(format!("invalid model_id_down: {}", e)))?;
        let m = CalibrationStore::get_model_by_id(&state.db_pool, uuid)
            .await
            .map_err(|e| AppError::Internal(format!("get model_down failed: {}", e)))?
            .ok_or_else(|| AppError::NotFound("下跌校准模型不存在".to_string()))?;
        let fitted = m
            .to_fitted()
            .ok_or_else(|| AppError::Validation("下跌校准模型参数不完整".to_string()))?;
        (fitted, Some(m.model_id.to_string()))
    } else {
        (identity.clone(), None)
    };

    let (cal_up, cal_down, cal_flat) =
        calibrate_three_class(&model_up, &model_down, req.p_up, req.p_down, req.p_flat);

    Ok(Json(ApplyCalibrationResponse {
        original_p_up: req.p_up,
        original_p_down: req.p_down,
        original_p_flat: req.p_flat,
        calibrated_p_up: cal_up,
        calibrated_p_down: cal_down,
        calibrated_p_flat: cal_flat,
        delta_up: cal_up - req.p_up,
        delta_down: cal_down - req.p_down,
        model_id_up,
        model_id_down,
    }))
}

// ============================================================
// P2-2: 交易建议卡片 API
// 依据《系统 2.0 待补全问题清单》9.2.1
// ============================================================

/// 交易建议请求
#[derive(Debug, Deserialize)]
struct TradeRecommendationRequest {
    symbol: String,
    /// 信号方向: long / short / hold
    direction: String,
    /// 信号置信度 (0-1)
    confidence: f64,
    /// 信号强度 (0-1)
    signal_strength: f64,
    /// 预期收益 (bps)
    expected_return_bps: Option<f64>,
    /// 当前价格
    current_price: f64,
    /// 资产年化波动率 (可选，默认 0.6)
    asset_volatility: Option<f64>,
    /// 账户总权益
    total_equity: f64,
    /// 已用保证金
    margin_used: Option<f64>,
    /// 已有持仓 notional
    existing_position_notional: Option<f64>,
    /// 市场状态 (trending_bull/trending_bear/ranging/high_volatility/crisis)
    market_regime: Option<String>,
}

/// 交易建议响应
#[derive(Debug, Serialize)]
struct TradeRecommendationResponse {
    /// 追踪 ID
    trace_id: String,
    /// 建议动作: open_long / open_short / hold
    action: String,
    /// 是否可执行
    executable: bool,
    /// 置信度 (0-1)
    confidence: f64,
    /// 期望价值 (净额，扣除成本)
    expected_value: f64,
    /// 条件风险价值 (CVaR，占权益比例)
    cvar: f64,
    /// 信任等级 A/B/C/D
    trust_level: String,
    /// 建议仓位占比 (0-1)
    position_pct: f64,
    /// 建议 notional
    suggested_notional: f64,
    /// 止损价格
    stop_loss_price: Option<f64>,
    /// 止盈价格
    take_profit_price: Option<f64>,
    /// 主要理由
    reasons: Vec<String>,
    /// 主要风险
    risks: Vec<String>,
    /// 不能交易的原因
    blockers: Vec<String>,
    /// 决策流水线步骤
    pipeline_steps: Vec<String>,
    /// 生成时间
    generated_at: String,
}

/// 生成交易建议卡片
///
/// 调用决策编排器，综合 EV/CVaR/仓位/风控/信任等级，
/// 返回完整的交易建议（含止损止盈/理由/风险/阻断原因）
async fn generate_trade_recommendation(
    _user: CurrentUser,
    State(_state): State<AppState>,
    Json(req): Json<TradeRecommendationRequest>,
) -> Result<Json<TradeRecommendationResponse>> {
    use crate::backtest::models::{AccountState, AlphaSignal};
    use crate::backtest::risk_engine::{RiskConfig, RiskEngine};
    use crate::backtest::position_sizing::PositionSizingEngine;
    use crate::features::MarketRegime;
    use crate::orchestration::decision_orchestrator::DecisionOrchestrator;
    use crate::signals::decision_engine::DecisionAction;
    use chrono::Utc;

    // 构造 AlphaSignal
    let signal = AlphaSignal {
        signal_id: Uuid::new_v4(),
        job_id: None,
        strategy_id: None,
        agent_id: None,
        asset: req.symbol.clone(),
        exchange: None,
        timeframe: Some("1H".into()),
        event_time: Utc::now(),
        valid_until: Some(Utc::now() + chrono::Duration::hours(1)),
        direction: req.direction.clone(),
        signal_strength: Some(req.signal_strength),
        confidence: Some(req.confidence),
        expected_return_bps: req.expected_return_bps,
        expected_holding_period_sec: Some(3600),
        market_regime: req.market_regime.clone(),
        features_used: None,
        risk_flags: None,
        explanation: None,
    };

    // 构造 AccountState
    let account = AccountState {
        job_id: Uuid::nil(),
        timestamp: Utc::now(),
        initial_equity: req.total_equity,
        cash: req.total_equity - req.margin_used.unwrap_or(0.0),
        margin_used: req.margin_used.unwrap_or(0.0),
        unrealized_pnl: 0.0,
        realized_pnl: 0.0,
        total_equity: req.total_equity,
        total_notional: req.existing_position_notional.unwrap_or(0.0),
        leverage: 1.0,
        drawdown_pct: 0.0,
        peak_equity: req.total_equity,
    };

    // 解析市场状态
    let regime = req.market_regime.as_deref().and_then(|s| match s {
        "trending_bull" => Some(MarketRegime::TrendingBull),
        "trending_bear" => Some(MarketRegime::TrendingBear),
        "ranging" => Some(MarketRegime::Ranging),
        "high_volatility" => Some(MarketRegime::HighVolatility),
        "crisis" => Some(MarketRegime::Crisis),
        _ => None,
    });

    // 创建编排器
    let risk_engine = RiskEngine::new(RiskConfig::default());
    let orchestrator = DecisionOrchestrator::with_defaults(risk_engine);

    let asset_vol = req.asset_volatility.unwrap_or(0.6);

    // 执行决策流水线
    let result = orchestrator.orchestrate(
        &signal,
        regime,
        &account,
        req.current_price,
        asset_vol,
        req.existing_position_notional.unwrap_or(0.0),
    );

    // 计算止损止盈
    let (stop_loss_price, take_profit_price) = {
        let sl_pct = 0.02; // 2% 止损
        let tp_pct = req
            .expected_return_bps
            .map(|bps| (bps / 10000.0).abs())
            .unwrap_or(0.03);
        match result.final_action {
            DecisionAction::OpenLong => (
                Some(req.current_price * (1.0 - sl_pct)),
                Some(req.current_price * (1.0 + tp_pct)),
            ),
            DecisionAction::OpenShort => (
                Some(req.current_price * (1.0 + sl_pct)),
                Some(req.current_price * (1.0 - tp_pct)),
            ),
            DecisionAction::Hold => (None, None),
        }
    };

    // 信任等级（基于 EV/CVaR/置信度）
    let trust_level = if result.decision.expected_value > 0.01
        && result.decision.cvar < 0.03
        && req.confidence > 0.7
    {
        "A"
    } else if result.decision.expected_value > 0.005
        && result.decision.cvar < 0.05
        && req.confidence > 0.5
    {
        "B"
    } else if result.decision.expected_value > 0.0 {
        "C"
    } else {
        "D"
    };

    // 主要风险
    let mut risks = Vec::new();
    if result.decision.cvar > 0.05 {
        risks.push(format!("CVaR 占权益 {:.2}%，极端损失风险较高", result.decision.cvar * 100.0));
    }
    if asset_vol > 0.8 {
        risks.push(format!("资产年化波动率 {:.0}%，波动性高", asset_vol * 100.0));
    }
    if let Some(ref regime_str) = req.market_regime {
        if regime_str == "high_volatility" || regime_str == "crisis" {
            risks.push(format!("市场状态为 {}，不适合大仓位", regime_str));
        }
    }
    if req.confidence < 0.5 {
        risks.push(format!("信号置信度 {:.0}% 低于阈值", req.confidence * 100.0));
    }
    if risks.is_empty() {
        risks.push("市场风险正常".into());
    }

    let response = TradeRecommendationResponse {
        trace_id: result.trace_id.to_string(),
        action: result.final_action.as_str().to_string(),
        executable: result.can_execute(),
        confidence: result.decision.confidence,
        expected_value: result.decision.expected_value,
        cvar: result.decision.cvar,
        trust_level: trust_level.to_string(),
        position_pct: result.final_position_pct,
        suggested_notional: result.final_notional,
        stop_loss_price,
        take_profit_price,
        reasons: result.decision.reasons,
        risks,
        blockers: result.blockers,
        pipeline_steps: result.pipeline_steps,
        generated_at: Utc::now().to_rfc3339(),
    };

    Ok(Json(response))
}
