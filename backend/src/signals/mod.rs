//! Signal & Decision Card Module
//! 概率信号与决策卡模块
//!
//! 依据《系统评估与演进规划》第四节"概率信号与决策卡设计"：
//! - 预测目标：给定周期的条件分布 r(t,h) = ln(P(t+h) / P(t))
//! - 模型输出：p_up/p_down/p_flat、q10/q50/q90、expected_volatility、uncertainty
//! - 净期望：EV = p_up * mu_up + p_down * mu_down + p_flat * mu_flat - fee - slippage - funding - impact
//! - 交易门禁：EV > EV_min、P(EV > 0) > p_min、CVaR_95 < risk_budget、data_freshness == pass
//!
//! 模块结构：
//! - `models`: 数据结构（SignalPrediction、DecisionCard、CalibrationReport）
//! - `store`: 存储层（读写到 PostgreSQL）
//! - `calibration`: 概率校准（Brier、LogLoss、校准曲线）

pub mod calibration;
pub mod models;
pub mod store;

pub use calibration::{
    calibrate_three_class, compute_brier_score, compute_calibration_curve, compute_log_loss,
};
pub use models::{
    CalibrationReport, DecisionCard, DecisionCardBuilder, SuggestedAction,
};
pub use store::SignalStore;
