//! Signal & Decision Card Data Models
//! 概率信号与决策卡数据模型
//!
//! 依据《系统评估与演进规划》第 4.4 节"概率信号与决策卡设计"

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 概率预测
/// 模型输出的概率分布、分位数、模型版本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalPrediction {
    pub prediction_id: Uuid,
    pub symbol: String,
    pub prediction_time: DateTime<Utc>,
    /// 预测目标周期（秒）
    pub target_horizon_sec: i32,

    /// 上涨概率
    pub p_up: f64,
    /// 下跌概率
    pub p_down: f64,
    /// 震荡概率
    pub p_flat: f64,

    /// 10 分位数（悲观情景）
    pub q10: Option<f64>,
    /// 50 分位数（中位预期）
    pub q50: Option<f64>,
    /// 90 分位数（乐观情景）
    pub q90: Option<f64>,

    /// 预期波动率
    pub expected_volatility: Option<f64>,
    /// 平均绝对误差估计
    pub mae_estimate: Option<f64>,
    /// 不确定性度量
    pub uncertainty: Option<f64>,

    /// 模型版本
    pub model_version: String,
    /// 模型类型
    pub model_type: String,
    /// 特征版本
    pub feature_version: Option<String>,
    /// 使用的特征列表
    pub features_used: Option<serde_json::Value>,
    /// 预测时的市场状态
    pub market_regime: Option<String>,

    /// 实际收益率（回填）
    pub realized_return: Option<f64>,
    /// 实际方向（up/down/flat）
    pub realized_direction: Option<String>,
    /// 评估时间
    pub evaluated_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
}

/// 建议动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    OpenLong,
    OpenShort,
    Close,
    Hold,
    Reduce,
}

impl SuggestedAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenLong => "open_long",
            Self::OpenShort => "open_short",
            Self::Close => "close",
            Self::Hold => "hold",
            Self::Reduce => "reduce",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open_long" => Some(Self::OpenLong),
            "open_short" => Some(Self::OpenShort),
            "close" => Some(Self::Close),
            "hold" => Some(Self::Hold),
            "reduce" => Some(Self::Reduce),
            _ => None,
        }
    }
}

/// 决策卡
/// 把每次交易变成"概率分布 + EV + CVaR + 失效条件 + 数据血缘"的可审计对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionCard {
    pub card_id: Uuid,
    pub user_id: Option<i64>,
    pub symbol: String,
    pub generated_at: DateTime<Utc>,

    /// 建议动作
    pub suggested_action: SuggestedAction,
    /// 预测周期（秒）
    pub target_horizon_sec: i32,

    /// 概率分布
    pub p_up: f64,
    pub p_down: f64,
    pub p_flat: f64,

    /// 收益区间
    pub q10: Option<f64>,
    pub q50: Option<f64>,
    pub q90: Option<f64>,

    /// 净期望 EV（扣除费用/滑点/资金费率后）
    pub expected_value: f64,
    /// 最坏情形（CVaR 口径）
    pub worst_case: Option<f64>,
    /// 仓位建议（0-1）
    pub position_suggestion: f64,
    /// 已用风险预算
    pub risk_budget_used: Option<f64>,

    /// 适用市场状态
    pub applicable_regime: Option<String>,
    /// 数据新鲜度（秒）
    pub data_freshness_sec: Option<f64>,

    /// 支持证据
    pub supporting_evidence: Option<serde_json::Value>,
    /// 反对证据
    pub opposing_evidence: Option<serde_json::Value>,
    /// 样本表现
    pub sample_performance: Option<serde_json::Value>,
    /// 数据血缘
    pub data_lineage: Option<serde_json::Value>,

    /// 失效条件列表
    pub invalidation_conditions: Option<serde_json::Value>,

    /// 决策原因（来自 DecisionEngine 的 reasons 列表）
    pub reasons: Option<Vec<String>>,
    /// 阻断原因（来自 DecisionEngine 的 blockers 列表，仅 action != Hold 时为空）
    pub blockers: Option<Vec<String>>,
    /// 回测可信等级（display_only / comparable / promotion_eligible）
    pub trust_level: Option<String>,

    /// 模型版本
    pub model_version: String,
    /// 关联的预测 ID
    pub prediction_id: Option<Uuid>,

    /// 用户实际采取的动作
    pub user_action: Option<String>,
    /// 用户反馈
    pub user_feedback: Option<String>,
    /// 用户行动时间
    pub acted_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
}

/// 决策卡构建器
/// 用于从概率预测和成本参数构建决策卡
#[derive(Debug, Clone)]
pub struct DecisionCardBuilder {
    pub symbol: String,
    pub user_id: Option<i64>,
    pub prediction: SignalPrediction,
    pub model_version: String,
}

impl DecisionCardBuilder {
    pub fn new(symbol: &str, prediction: SignalPrediction) -> Self {
        Self {
            symbol: symbol.to_string(),
            user_id: None,
            prediction,
            model_version: "unknown".to_string(),
        }
    }

    pub fn with_user_id(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_model_version(mut self, version: &str) -> Self {
        self.model_version = version.to_string();
        self
    }

    /// 计算净期望 EV
    /// EV = p_up * mu_up + p_down * mu_down + p_flat * mu_flat - fee - slippage - funding - impact
    pub fn compute_expected_value(
        &self,
        mu_up: f64,
        mu_down: f64,
        mu_flat: f64,
        fee_cost: f64,
        slippage_cost: f64,
        funding_cost: f64,
        impact_cost: f64,
    ) -> f64 {
        let gross_ev = self.prediction.p_up * mu_up
            + self.prediction.p_down * mu_down
            + self.prediction.p_flat * mu_flat;
        gross_ev - fee_cost - slippage_cost - funding_cost - impact_cost
    }

    /// 根据概率分布决定建议动作
    pub fn decide_action(&self, ev: f64, ev_min: f64) -> SuggestedAction {
        // 净期望不足，建议持有
        if ev < ev_min {
            return SuggestedAction::Hold;
        }
        // 根据概率分布决定方向
        if self.prediction.p_up > self.prediction.p_down
            && self.prediction.p_up > self.prediction.p_flat
        {
            SuggestedAction::OpenLong
        } else if self.prediction.p_down > self.prediction.p_up
            && self.prediction.p_down > self.prediction.p_flat
        {
            SuggestedAction::OpenShort
        } else {
            SuggestedAction::Hold
        }
    }

    /// 构建决策卡
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        self,
        ev: f64,
        action: SuggestedAction,
        position_suggestion: f64,
        worst_case: Option<f64>,
        risk_budget_used: Option<f64>,
        data_freshness_sec: Option<f64>,
        supporting_evidence: Option<serde_json::Value>,
        opposing_evidence: Option<serde_json::Value>,
        sample_performance: Option<serde_json::Value>,
        data_lineage: Option<serde_json::Value>,
        invalidation_conditions: Option<serde_json::Value>,
        reasons: Option<Vec<String>>,
        blockers: Option<Vec<String>>,
        trust_level: Option<String>,
    ) -> DecisionCard {
        let now = Utc::now();
        DecisionCard {
            card_id: Uuid::new_v4(),
            user_id: self.user_id,
            symbol: self.symbol,
            generated_at: now,
            suggested_action: action,
            target_horizon_sec: self.prediction.target_horizon_sec,
            p_up: self.prediction.p_up,
            p_down: self.prediction.p_down,
            p_flat: self.prediction.p_flat,
            q10: self.prediction.q10,
            q50: self.prediction.q50,
            q90: self.prediction.q90,
            expected_value: ev,
            worst_case,
            position_suggestion,
            risk_budget_used,
            applicable_regime: self.prediction.market_regime.clone(),
            data_freshness_sec,
            supporting_evidence,
            opposing_evidence,
            sample_performance,
            data_lineage,
            invalidation_conditions,
            reasons,
            blockers,
            trust_level,
            model_version: self.model_version,
            prediction_id: Some(self.prediction.prediction_id),
            user_action: None,
            user_feedback: None,
            acted_at: None,
            created_at: now,
        }
    }
}

/// 概率校准报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub report_id: Uuid,
    pub model_version: String,
    pub symbol: Option<String>,
    pub market_regime: Option<String>,

    /// 评估时间范围
    pub eval_start: DateTime<Utc>,
    pub eval_end: DateTime<Utc>,

    /// Brier 分数（越小越好）
    pub brier_score: f64,
    /// 对数损失（越小越好）
    pub log_loss: f64,
    /// 方向准确率
    pub accuracy: f64,
    /// 校准误差
    pub calibration_error: Option<f64>,

    /// 校准曲线 [{predicted, actual, count}]
    pub calibration_curve: serde_json::Value,

    /// 样本量
    pub sample_count: i32,
    pub up_count: i32,
    pub down_count: i32,
    pub flat_count: i32,

    /// 是否校准良好
    pub is_well_calibrated: bool,
    /// 是否检测到退化
    pub degradation_detected: bool,

    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_prediction(p_up: f64, p_down: f64, p_flat: f64) -> SignalPrediction {
        SignalPrediction {
            prediction_id: Uuid::new_v4(),
            symbol: "BTC-USDT".into(),
            prediction_time: Utc::now(),
            target_horizon_sec: 3600,
            p_up,
            p_down,
            p_flat,
            q10: Some(-0.02),
            q50: Some(0.0),
            q90: Some(0.02),
            expected_volatility: Some(0.015),
            mae_estimate: Some(0.01),
            uncertainty: Some(0.3),
            model_version: "test_v1".into(),
            model_type: "classifier".into(),
            feature_version: Some("v1".into()),
            features_used: None,
            market_regime: Some("trending_bull".into()),
            realized_return: None,
            realized_direction: None,
            evaluated_at: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_compute_expected_value_positive() {
        let pred = make_prediction(0.6, 0.3, 0.1);
        let builder = DecisionCardBuilder::new("BTC-USDT", pred);
        // mu_up=0.02, mu_down=-0.02, mu_flat=0.0, 成本=0.001
        let ev = builder.compute_expected_value(0.02, -0.02, 0.0, 0.001, 0.001, 0.0, 0.0);
        // EV = 0.6*0.02 + 0.3*(-0.02) + 0.1*0 - 0.001 - 0.001 = 0.012 - 0.006 - 0.002 = 0.004
        assert!((ev - 0.004).abs() < 1e-9, "EV 应为 0.004，实际: {}", ev);
    }

    #[test]
    fn test_compute_expected_value_negative_when_costs_high() {
        let pred = make_prediction(0.5, 0.3, 0.2);
        let builder = DecisionCardBuilder::new("BTC-USDT", pred);
        // 高成本场景
        let ev = builder.compute_expected_value(0.01, -0.01, 0.0, 0.005, 0.005, 0.003, 0.002);
        // EV = 0.5*0.01 + 0.3*(-0.01) + 0.2*0 - 0.005 - 0.005 - 0.003 - 0.002
        //    = 0.005 - 0.003 - 0.015 = -0.013
        assert!(ev < 0.0, "高成本时 EV 应为负，实际: {}", ev);
    }

    #[test]
    fn test_decide_action_hold_when_ev_low() {
        let pred = make_prediction(0.7, 0.2, 0.1);
        let builder = DecisionCardBuilder::new("BTC-USDT", pred);
        // EV 低于阈值，应建议持有
        let action = builder.decide_action(0.001, 0.005);
        assert_eq!(action, SuggestedAction::Hold);
    }

    #[test]
    fn test_decide_action_open_long_when_ev_high() {
        let pred = make_prediction(0.7, 0.2, 0.1);
        let builder = DecisionCardBuilder::new("BTC-USDT", pred);
        // EV 高于阈值且 p_up 最大，应建议开多
        let action = builder.decide_action(0.01, 0.005);
        assert_eq!(action, SuggestedAction::OpenLong);
    }

    #[test]
    fn test_decide_action_open_short_when_p_down_dominant() {
        let pred = make_prediction(0.2, 0.7, 0.1);
        let builder = DecisionCardBuilder::new("BTC-USDT", pred);
        // p_down 最大且 EV 高于阈值，应建议开空
        let action = builder.decide_action(0.01, 0.005);
        assert_eq!(action, SuggestedAction::OpenShort);
    }

    #[test]
    fn test_suggested_action_serialization() {
        assert_eq!(SuggestedAction::OpenLong.as_str(), "open_long");
        assert_eq!(SuggestedAction::OpenShort.as_str(), "open_short");
        assert_eq!(SuggestedAction::Hold.as_str(), "hold");
        assert_eq!(
            SuggestedAction::from_str("open_long"),
            Some(SuggestedAction::OpenLong)
        );
        assert_eq!(SuggestedAction::from_str("invalid"), None);
    }
}
