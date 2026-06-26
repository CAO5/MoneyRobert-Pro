//! Decision Engine: EV/CVaR-based trade decision
//! 决策引擎 — 基于期望价值和条件风险价值的交易决策
//!
//! 依据《系统 2.0 待补全问题清单》P1-1：
//! 替换 runner 中简单的 confidence/strength 阈值逻辑，
//! 采用 EV + CVaR + 市场状态的综合决策。

use crate::backtest::models::{AccountState, AlphaSignal};
use crate::features::MarketRegime;

/// 决策引擎配置
#[derive(Debug, Clone)]
pub struct DecisionConfig {
    /// 基础 EV 阈值（净期望低于此值不开仓）
    pub ev_min_base: f64,
    /// CVaR 占权益的最大比例（超过则拒单）
    pub max_cvar_pct: f64,
    /// 手续费率（bps）
    pub fee_bps: f64,
    /// 滑点率（bps）
    pub slippage_bps: f64,
    /// 默认资金费率（bps，8h 周期）
    pub default_funding_rate_bps: f64,
    /// 默认持仓周期（秒，用于资金费率累计）
    pub default_holding_period_sec: f64,
    /// CVaR 置信度（0.95 = 95%）
    pub cvar_confidence: f64,
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            ev_min_base: 0.001, // 0.1% 净期望
            max_cvar_pct: 0.05,  // CVaR 不超过权益的 5%（加密货币波动率高）
            fee_bps: 5.0,
            slippage_bps: 3.0,
            default_funding_rate_bps: 1.0, // 默认 1 bps / 8h
            default_holding_period_sec: 86400.0 * 3.0, // 默认持仓 3 天
            cvar_confidence: 0.95,
        }
    }
}

/// 概率校准数据 — 从历史预测结果拟合，用于将原始置信度校准为真实概率
#[derive(Debug, Clone, Default)]
pub struct CalibrationData {
    /// Brier Score（0=完美，1=最差）
    pub brier_score: Option<f64>,
    /// 校准曲线斜率（1.0 = 完美校准）
    pub calibration_slope: Option<f64>,
    /// 校准曲线截距（0.0 = 完美校准）
    pub calibration_intercept: Option<f64>,
    /// 样本量（用于判断校准数据是否足够）
    pub sample_size: Option<i64>,
}

impl CalibrationData {
    /// 将原始概率校准为真实概率
    /// 使用线性校准：p_calibrated = slope * p_raw + intercept
    pub fn calibrate(&self, p_raw: f64) -> f64 {
        match (self.calibration_slope, self.calibration_intercept) {
            (Some(slope), Some(intercept)) => {
                // 线性校准 + clamp 到 [0.01, 0.99]
                (slope * p_raw + intercept).clamp(0.01, 0.99)
            }
            _ => p_raw, // 无校准数据，返回原始概率
        }
    }

    /// 校准数据是否足够可信（样本量 >= 30 且 Brier Score < 0.33）
    pub fn is_reliable(&self) -> bool {
        self.sample_size.unwrap_or(0) >= 30
            && self.brier_score.map(|b| b < 0.33).unwrap_or(false)
    }
}

/// 决策输入
#[derive(Debug, Clone)]
pub struct DecisionInput<'a> {
    pub signal: &'a AlphaSignal,
    pub regime: Option<MarketRegime>,
    pub account: &'a AccountState,
    pub current_price: f64,
    /// 资产年化波动率（None 时使用默认 0.6）
    pub asset_volatility: Option<f64>,
    /// 资金费率（bps / 8h），用于计算持仓成本
    pub funding_rate_bps: Option<f64>,
    /// 概率校准数据（从 DB 读取历史校准结果）
    pub calibration: Option<CalibrationData>,
}

/// 决策输出
#[derive(Debug, Clone)]
pub struct DecisionOutput {
    /// 建议动作
    pub action: DecisionAction,
    /// 期望价值（净额，扣除成本）
    pub expected_value: f64,
    /// 条件风险价值（CVaR，占权益比例）
    pub cvar: f64,
    /// 综合置信度
    pub confidence: f64,
    /// 建议仓位占比
    pub position_pct: f64,
    /// 决策原因
    pub reasons: Vec<String>,
    /// 阻断原因（action=Hold 时非空）
    pub blockers: Vec<String>,
}

/// 决策动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionAction {
    OpenLong,
    OpenShort,
    Hold,
}

impl DecisionAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenLong => "open_long",
            Self::OpenShort => "open_short",
            Self::Hold => "hold",
        }
    }
}

/// 决策引擎
pub struct DecisionEngine {
    config: DecisionConfig,
}

impl DecisionEngine {
    pub fn new(config: DecisionConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(DecisionConfig::default())
    }

    /// 根据信号、市场状态和账户状态做出交易决策
    ///
    /// 决策流程（v1.8 增强版）：
    /// 1. 从信号推导概率分布，若有校准数据则校准概率
    /// 2. 计算成本（手续费 + 滑点 + 资金费率）
    /// 3. 计算净 EV
    /// 4. 计算 CVaR（基于波动率、仓位占比和持仓周期）
    /// 5. 根据 regime 调整 EV 阈值
    /// 6. 综合 EV、CVaR、regime 给出最终建议
    pub fn decide(&self, input: &DecisionInput) -> DecisionOutput {
        let signal = input.signal;
        let mut reasons = Vec::new();
        let mut blockers = Vec::new();

        // Hold 信号直接返回
        if signal.is_hold() {
            blockers.push("signal_direction_is_hold".into());
            return DecisionOutput {
                action: DecisionAction::Hold,
                expected_value: 0.0,
                cvar: 0.0,
                confidence: 0.0,
                position_pct: 0.0,
                reasons,
                blockers,
            };
        }

        // 从信号推导原始概率
        let raw_confidence = signal.confidence.unwrap_or(0.0).clamp(0.0, 1.0);
        let strength = signal.signal_strength.unwrap_or(0.0).clamp(0.0, 1.0);
        let expected_return_bps = signal.expected_return_bps.unwrap_or(0.0);

        // 1) 概率校准：若有校准数据，将原始置信度校准为真实概率
        let (p_win, calibration_note) = match &input.calibration {
            Some(cal) if cal.is_reliable() => {
                let calibrated = cal.calibrate(raw_confidence);
                (
                    calibrated,
                    format!(
                        "calibrated: {:.3} -> {:.3} (slope={:?} intercept={:?} brier={:?} n={:?})",
                        raw_confidence,
                        calibrated,
                        cal.calibration_slope,
                        cal.calibration_intercept,
                        cal.brier_score,
                        cal.sample_size
                    ),
                )
            }
            Some(cal) => (
                raw_confidence,
                format!(
                    "calibration_skipped: unreliable (brier={:?} n={:?})",
                    cal.brier_score, cal.sample_size
                ),
            ),
            None => (
                raw_confidence,
                "no_calibration_data: using raw confidence".into(),
            ),
        };
        reasons.push(calibration_note);

        // 概率分配：校准后的概率
        let p_loss = (1.0 - p_win) / 2.0;
        let p_flat = 1.0 - p_win - p_loss;

        // 预期收益（将 bps 转为比例）
        let mu_win = expected_return_bps.max(0.0) / 10000.0;
        let mu_loss = expected_return_bps.abs().min(500.0) / 10000.0; // 限制最大 5%
        let mu_flat = 0.0;

        // 2) 成本计算（手续费 + 滑点 + 资金费率）
        let fee_cost_pct = (self.config.fee_bps + self.config.slippage_bps) / 10000.0;

        // 资金费率成本：funding_rate_bps / 10000 * (holding_periods)
        // holding_periods = holding_period_sec / 28800 (8h = 28800s)
        let funding_rate_bps = input.funding_rate_bps.unwrap_or(self.config.default_funding_rate_bps);
        let holding_sec = signal
            .expected_holding_period_sec
            .map(|v| v as f64)
            .unwrap_or(self.config.default_holding_period_sec);
        let funding_periods = (holding_sec / 28800.0).max(1.0);
        let funding_cost_pct = (funding_rate_bps * funding_periods) / 10000.0;
        let total_cost_pct = fee_cost_pct + funding_cost_pct;

        reasons.push(format!(
            "costs: fee_slippage={:.4}% funding={:.4}% (rate={}bps periods={:.1}) total={:.4}%",
            fee_cost_pct * 100.0,
            funding_cost_pct * 100.0,
            funding_rate_bps,
            funding_periods,
            total_cost_pct * 100.0
        ));

        // 3) 净 EV = p_win * mu_win - p_loss * mu_loss + p_flat * mu_flat - cost
        let gross_ev = p_win * mu_win - p_loss * mu_loss + p_flat * mu_flat;
        let net_ev = gross_ev - total_cost_pct;
        reasons.push(format!(
            "ev: gross={:.6} net={:.6} (p_win={:.3} mu_win={:.4} p_loss={:.3} mu_loss={:.4})",
            gross_ev, net_ev, p_win, mu_win, p_loss, mu_loss
        ));

        // 4) CVaR 估算：基于波动率、仓位占比和持仓周期
        // CVaR_95 ≈ z * sigma_daily * sqrt(holding_days) * position_pct
        // 其中 z = 2.063 (95% CVaR for normal distribution)
        // sigma_daily = asset_vol / sqrt(365) (年化波动率转日波动率)
        let asset_vol = input.asset_volatility.unwrap_or(0.6);
        let equity = input.account.total_equity.max(1.0);
        let holding_days = (holding_sec / 86400.0).max(1.0).min(30.0); // 限制 1-30 天
        let sigma_daily = asset_vol / (365.0_f64).sqrt();
        let z_cvar = 2.063; // 95% CVaR 正态分布分位数
        let estimated_position_pct = 0.05; // 估算 5% 仓位用于 CVaR 计算
        let cvar = z_cvar * sigma_daily * holding_days.sqrt() * estimated_position_pct;
        reasons.push(format!(
            "cvar: asset_vol={:.3} sigma_daily={:.4} holding_days={:.1} est_cvar={:.4}% (limit={:.4}%)",
            asset_vol,
            sigma_daily,
            holding_days,
            cvar * 100.0,
            self.config.max_cvar_pct * 100.0
        ));

        // Regime 调整 EV 阈值和仓位系数
        let (ev_threshold, regime_multiplier, regime_reason) = match input.regime {
            Some(MarketRegime::TrendingBull) => {
                if signal.is_long() {
                    (self.config.ev_min_base * 0.5, 1.2, "trending_bull: long EV 阈值降低 50%".into())
                } else {
                    (self.config.ev_min_base * 1.5, 0.6, "trending_bull: short EV 阈值提高 50%".into())
                }
            }
            Some(MarketRegime::TrendingBear) => {
                if signal.is_short() {
                    (self.config.ev_min_base * 0.5, 1.2, "trending_bear: short EV 阈值降低 50%".into())
                } else {
                    (self.config.ev_min_base * 1.5, 0.6, "trending_bear: long EV 阈值提高 50%".into())
                }
            }
            Some(MarketRegime::Ranging) => {
                (self.config.ev_min_base * 1.5, 0.5, "ranging: EV 阈值提高 50%".into())
            }
            Some(MarketRegime::HighVolatility) => {
                (self.config.ev_min_base * 2.0, 0.3, "high_volatility: EV 阈值翻倍".into())
            }
            Some(MarketRegime::Crisis) => {
                (f64::MAX, 0.0, "crisis: 拒绝开新仓".into())
            }
            None => (self.config.ev_min_base, 1.0, "no_regime".into()),
        };
        reasons.push(regime_reason);

        // 决策逻辑
        let mut action = DecisionAction::Hold;
        let mut position_pct = 0.0;

        // Crisis 直接拒单
        if matches!(input.regime, Some(MarketRegime::Crisis)) {
            blockers.push("crisis_regime_no_new_positions".into());
        }
        // EV 检查
        else if net_ev < ev_threshold {
            blockers.push(format!(
                "ev_too_low: {:.6} < threshold {:.6}",
                net_ev, ev_threshold
            ));
        }
        // CVaR 检查
        else if cvar > self.config.max_cvar_pct {
            blockers.push(format!(
                "cvar_exceeded: {:.4}% > limit {:.4}%",
                cvar * 100.0,
                self.config.max_cvar_pct * 100.0
            ));
        }
        // 通过所有检查，确定方向
        else {
            action = if signal.is_long() {
                DecisionAction::OpenLong
            } else {
                DecisionAction::OpenShort
            };

            // 仓位计算：基于 strength 和 regime 调整
            let base_pct = 0.05 * strength; // 基础仓位 5% * strength
            position_pct = (base_pct * regime_multiplier).min(0.10); // 不超过 10%
            reasons.push(format!(
                "position: base={:.4}% regime_adj={:.4}% final={:.4}%",
                base_pct * 100.0,
                base_pct * regime_multiplier * 100.0,
                position_pct * 100.0
            ));
        }

        DecisionOutput {
            action,
            expected_value: net_ev,
            cvar,
            confidence: p_win, // 使用校准后的概率
            position_pct,
            reasons,
            blockers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::models::AlphaSignal;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_account() -> AccountState {
        AccountState::new(Uuid::new_v4(), 10000.0, Utc::now())
    }

    fn make_signal(direction: &str, confidence: f64, strength: f64, return_bps: f64) -> AlphaSignal {
        AlphaSignal {
            signal_id: Uuid::new_v4(),
            job_id: Some(Uuid::new_v4()),
            strategy_id: None,
            agent_id: None,
            asset: "BTC".into(),
            exchange: Some("OKX".into()),
            timeframe: Some("1H".into()),
            event_time: Utc::now(),
            valid_until: None,
            direction: direction.into(),
            signal_strength: Some(strength),
            confidence: Some(confidence),
            expected_return_bps: Some(return_bps),
            expected_holding_period_sec: None,
            market_regime: None,
            features_used: None,
            risk_flags: None,
            explanation: None,
        }
    }

    #[test]
    fn test_hold_signal_returns_hold() {
        let engine = DecisionEngine::with_defaults();
        let signal = make_signal("hold", 0.9, 0.8, 100.0);
        let account = make_account();
        let input = DecisionInput {
            signal: &signal,
            regime: None,
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration: None,
        };
        let out = engine.decide(&input);
        assert_eq!(out.action, DecisionAction::Hold);
        assert!(out.blockers.contains(&"signal_direction_is_hold".to_string()));
    }

    #[test]
    fn test_low_ev_rejected() {
        let engine = DecisionEngine::with_defaults();
        // 低置信度 + 低预期收益 → EV 不足
        let signal = make_signal("long", 0.4, 0.3, 10.0);
        let account = make_account();
        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::Ranging),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration: None,
        };
        let out = engine.decide(&input);
        assert_eq!(out.action, DecisionAction::Hold);
        assert!(out.blockers.iter().any(|b| b.contains("ev_too_low")));
    }

    #[test]
    fn test_high_ev_long_accepted() {
        let engine = DecisionEngine::with_defaults();
        // 高置信度 + 高预期收益 → EV 充足
        let signal = make_signal("long", 0.8, 0.9, 200.0);
        let account = make_account();
        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration: None,
        };
        let out = engine.decide(&input);
        assert_eq!(out.action, DecisionAction::OpenLong);
        assert!(out.expected_value > 0.0);
        assert!(out.position_pct > 0.0);
    }

    #[test]
    fn test_crisis_regime_blocks_all() {
        let engine = DecisionEngine::with_defaults();
        let signal = make_signal("long", 0.9, 0.9, 500.0);
        let account = make_account();
        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::Crisis),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration: None,
        };
        let out = engine.decide(&input);
        assert_eq!(out.action, DecisionAction::Hold);
        assert!(out.blockers.iter().any(|b| b.contains("crisis")));
    }

    #[test]
    fn test_high_volatility_reduces_position() {
        let engine = DecisionEngine::with_defaults();
        let signal = make_signal("long", 0.85, 0.9, 300.0);
        let account = make_account();

        // 低波动率场景
        let input_low_vol = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.3),
            funding_rate_bps: None,
            calibration: None,
        };
        let out_low_vol = engine.decide(&input_low_vol);

        // 高波动率场景
        let input_high_vol = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::HighVolatility),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.9),
            funding_rate_bps: None,
            calibration: None,
        };
        let out_high_vol = engine.decide(&input_high_vol);

        // 高波动率下仓位应更小
        if out_low_vol.action == DecisionAction::OpenLong && out_high_vol.action == DecisionAction::OpenLong {
            assert!(
                out_high_vol.position_pct < out_low_vol.position_pct,
                "高波动率下仓位应更小: {} < {}",
                out_high_vol.position_pct,
                out_low_vol.position_pct
            );
        }
    }

    #[test]
    fn test_short_signal_in_bear_market() {
        let engine = DecisionEngine::with_defaults();
        let signal = make_signal("short", 0.8, 0.9, 200.0);
        let account = make_account();
        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBear),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration: None,
        };
        let out = engine.decide(&input);
        assert_eq!(out.action, DecisionAction::OpenShort);
        assert!(out.position_pct > 0.0);
    }

    #[test]
    fn test_cvar_exceeds_limit_blocks() {
        let mut config = DecisionConfig::default();
        config.max_cvar_pct = 0.001; // 极低 CVaR 限制
        let engine = DecisionEngine::new(config);

        let signal = make_signal("long", 0.8, 0.9, 200.0);
        let account = make_account();
        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(2.0), // 极高波动率
            funding_rate_bps: None,
            calibration: None,
        };
        let out = engine.decide(&input);
        assert_eq!(out.action, DecisionAction::Hold);
        assert!(out.blockers.iter().any(|b| b.contains("cvar_exceeded")));
    }

    #[test]
    fn test_calibration_adjusts_probability() {
        let engine = DecisionEngine::with_defaults();
        let signal = make_signal("long", 0.8, 0.9, 200.0);
        let account = make_account();

        // 模拟校准数据：slope=0.8, intercept=0.05（降低高置信度，提升低置信度）
        let calibration = Some(CalibrationData {
            brier_score: Some(0.15),
            calibration_slope: Some(0.8),
            calibration_intercept: Some(0.05),
            sample_size: Some(100),
        });

        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration,
        };
        let out = engine.decide(&input);
        // 校准后概率 = 0.8 * 0.8 + 0.05 = 0.69
        assert!((out.confidence - 0.69).abs() < 0.01);
        assert!(out.reasons.iter().any(|r| r.contains("calibrated")));
    }

    #[test]
    fn test_unreliable_calibration_skipped() {
        let engine = DecisionEngine::with_defaults();
        let signal = make_signal("long", 0.8, 0.9, 200.0);
        let account = make_account();

        // 不可靠的校准数据（样本量不足）
        let calibration = Some(CalibrationData {
            brier_score: Some(0.4),
            calibration_slope: Some(0.5),
            calibration_intercept: Some(0.1),
            sample_size: Some(10),
        });

        let input = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: None,
            calibration,
        };
        let out = engine.decide(&input);
        // 不可靠 → 使用原始概率
        assert!((out.confidence - 0.8).abs() < 0.01);
        assert!(out.reasons.iter().any(|r| r.contains("calibration_skipped")));
    }

    #[test]
    fn test_funding_rate_increases_cost() {
        let signal = make_signal("long", 0.7, 0.8, 150.0);
        let account = make_account();

        // 无资金费率
        let input_no_funding = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: Some(0.0),
            calibration: None,
        };
        let out_no_funding = DecisionEngine::with_defaults().decide(&input_no_funding);

        // 高资金费率（10 bps / 8h，持仓 3 天 = 9 个周期）
        let input_high_funding = DecisionInput {
            signal: &signal,
            regime: Some(MarketRegime::TrendingBull),
            account: &account,
            current_price: 100.0,
            asset_volatility: Some(0.6),
            funding_rate_bps: Some(10.0),
            calibration: None,
        };
        let out_high_funding = DecisionEngine::with_defaults().decide(&input_high_funding);

        // 高资金费率下 EV 应更低
        assert!(
            out_high_funding.expected_value < out_no_funding.expected_value,
            "高资金费率应降低净 EV: {} < {}",
            out_high_funding.expected_value,
            out_no_funding.expected_value
        );
        // 成本日志应包含 funding
        assert!(out_high_funding.reasons.iter().any(|r| r.contains("funding")));
    }
}
