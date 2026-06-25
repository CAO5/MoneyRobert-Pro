//! Decision Orchestrator
//! 决策编排器 — 统一决策流水线
//!
//! 依据《系统 2.0 待补全问题清单》P1-4 / 8.3.1：
//! 接收 symbol/timeframe/user_id → 获取市场快照 → 计算特征 → 调用信号 →
//! 校准概率 → 计算 EV/CVaR → 调用仓位模型 → 调用风险模型 → 输出最终建议

use crate::backtest::models::{AccountState, AlphaSignal, RiskCheckResult};
use crate::backtest::position_sizing::{PositionSizingEngine, PositionSizingResult};
use crate::backtest::risk_engine::RiskEngine;
use crate::features::MarketRegime;
use crate::signals::decision_engine::{
    DecisionAction, DecisionEngine, DecisionInput, DecisionOutput,
};
use chrono::Utc;
use uuid::Uuid;

/// 编排器配置
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// 是否启用校准层（需要数据库中的校准模型）
    pub enable_calibration: bool,
    /// 是否启用信任检查
    pub enable_trust_check: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            enable_calibration: true,
            enable_trust_check: true,
        }
    }
}

/// 编排器输出结果
#[derive(Debug, Clone)]
pub struct OrchestratorResult {
    /// 唯一追踪 ID
    pub trace_id: Uuid,
    /// 决策引擎输出
    pub decision: DecisionOutput,
    /// 仓位计算结果
    pub sizing: Option<PositionSizingResult>,
    /// 风控检查结果
    pub risk_check: Option<RiskCheckResult>,
    /// 最终建议动作
    pub final_action: DecisionAction,
    /// 最终仓位占比（占 NAV）
    pub final_position_pct: f64,
    /// 最终 notional
    pub final_notional: f64,
    /// 决策流水线步骤
    pub pipeline_steps: Vec<String>,
    /// 阻断原因
    pub blockers: Vec<String>,
}

impl OrchestratorResult {
    /// 是否通过所有检查可以下单
    pub fn can_execute(&self) -> bool {
        self.final_action != DecisionAction::Hold
            && self.final_position_pct > 0.0
            && self.blockers.is_empty()
    }
}

/// 决策编排器
///
/// 统一编排：信号 → 决策引擎 → 仓位计算 → 风控检查
/// 可用于回测、模拟盘、实盘
pub struct DecisionOrchestrator {
    config: OrchestratorConfig,
    decision_engine: DecisionEngine,
    position_sizing: PositionSizingEngine,
    risk_engine: RiskEngine,
}

impl DecisionOrchestrator {
    pub fn new(
        config: OrchestratorConfig,
        decision_engine: DecisionEngine,
        position_sizing: PositionSizingEngine,
        risk_engine: RiskEngine,
    ) -> Self {
        Self {
            config,
            decision_engine,
            position_sizing,
            risk_engine,
        }
    }

    /// 使用默认配置创建编排器
    pub fn with_defaults(risk_engine: RiskEngine) -> Self {
        Self::new(
            OrchestratorConfig::default(),
            DecisionEngine::with_defaults(),
            PositionSizingEngine::with_defaults(),
            risk_engine,
        )
    }

    /// 执行完整决策流水线
    ///
    /// 流程：
    /// 1. 决策引擎（EV/CVaR/regime）→ 确定方向和基础仓位
    /// 2. 仓位计算（Kelly/波动率目标/风险预算）→ 精确仓位
    /// 3. 风控检查（单仓上限/杠杆/日亏）→ 最终约束
    /// 4. 输出最终建议
    pub fn orchestrate(
        &self,
        signal: &AlphaSignal,
        regime: Option<MarketRegime>,
        account: &AccountState,
        current_price: f64,
        asset_volatility: f64,
        existing_position_notional: f64,
    ) -> OrchestratorResult {
        let trace_id = Uuid::new_v4();
        let mut pipeline_steps = Vec::new();
        let mut blockers = Vec::new();

        pipeline_steps.push(format!(
            "[{}] orchestrator_start: asset={} price={} vol={}",
            trace_id, signal.asset, current_price, asset_volatility
        ));

        // Step 1: 决策引擎
        let decision_input = DecisionInput {
            signal,
            regime,
            account,
            current_price,
            asset_volatility: Some(asset_volatility),
        };
        let decision = self.decision_engine.decide(&decision_input);
        pipeline_steps.push(format!(
            "[{}] decision: action={} ev={:.6} cvar={:.4}% position_pct={:.4}%",
            trace_id,
            decision.action.as_str(),
            decision.expected_value,
            decision.cvar * 100.0,
            decision.position_pct * 100.0
        ));

        if decision.action == DecisionAction::Hold {
            blockers.extend(decision.blockers.clone());
            return OrchestratorResult {
                trace_id,
                decision,
                sizing: None,
                risk_check: None,
                final_action: DecisionAction::Hold,
                final_position_pct: 0.0,
                final_notional: 0.0,
                pipeline_steps,
                blockers,
            };
        }

        // Step 2: 仓位计算
        let confidence = signal.confidence.unwrap_or(0.5);
        let expected_return_bps = signal.expected_return_bps.unwrap_or(100.0);
        let avg_win = expected_return_bps.max(0.0) / 10000.0;
        let avg_loss = expected_return_bps.abs().min(500.0) / 10000.0;
        let stop_loss_pct = Some(0.02);

        let sizing = self.position_sizing.calculate(
            current_price,
            confidence,
            avg_win,
            avg_loss,
            asset_volatility,
            stop_loss_pct,
        );
        pipeline_steps.push(format!(
            "[{}] sizing: method={} pct={:.4}% kelly_raw={:.4}",
            trace_id,
            sizing.method.as_str(),
            sizing.position_pct * 100.0,
            sizing.kelly_raw
        ));

        // 取决策引擎和仓位引擎的较小值
        let position_pct = sizing.position_pct.min(decision.position_pct.max(0.0));
        if position_pct <= 0.0 {
            blockers.push("position_too_small".into());
            return OrchestratorResult {
                trace_id,
                decision,
                sizing: Some(sizing),
                risk_check: None,
                final_action: DecisionAction::Hold,
                final_position_pct: 0.0,
                final_notional: 0.0,
                pipeline_steps,
                blockers,
            };
        }

        // Step 3: 风控检查
        let notional = account.total_equity.max(0.0) * position_pct;
        let side = if decision.action == DecisionAction::OpenLong {
            "buy"
        } else {
            "sell"
        };

        let intent = crate::backtest::models::TradeIntent {
            intent_id: Uuid::new_v4(),
            job_id: signal.job_id,
            source_signal_id: Some(signal.signal_id),
            strategy_id: signal.strategy_id.clone(),
            agent_id: signal.agent_id.clone(),
            asset: signal.asset.clone(),
            exchange: signal.exchange.clone(),
            side: side.into(),
            intent_type: "open_position".into(),
            target_position_pct: Some(position_pct),
            target_notional: Some(notional),
            target_quantity: Some(notional / current_price),
            order_type: "market".into(),
            limit_price: None,
            max_slippage_bps: None,
            leverage: 1,
            stop_loss_price: sizing.stop_loss_price,
            take_profit_price: None,
            event_time: Utc::now(),
        };

        let risk_check = self.risk_engine.validate_intent(&intent, account, existing_position_notional);
        pipeline_steps.push(format!(
            "[{}] risk: passed={} reasons={:?}",
            trace_id, risk_check.passed, risk_check.reasons
        ));

        if !risk_check.passed {
            blockers.extend(risk_check.reasons.clone());
            return OrchestratorResult {
                trace_id,
                decision,
                sizing: Some(sizing),
                risk_check: Some(risk_check),
                final_action: DecisionAction::Hold,
                final_position_pct: 0.0,
                final_notional: 0.0,
                pipeline_steps,
                blockers,
            };
        }

        // Step 4: 最终建议
        let final_notional = risk_check.reduced_notional.unwrap_or(notional);
        let final_pct = if account.total_equity > 0.0 {
            final_notional / account.total_equity
        } else {
            0.0
        };

        let final_action = decision.action;
        pipeline_steps.push(format!(
            "[{}] final: action={} notional={:.2} pct={:.4}%",
            trace_id,
            final_action.as_str(),
            final_notional,
            final_pct * 100.0
        ));

        OrchestratorResult {
            trace_id,
            decision,
            sizing: Some(sizing),
            risk_check: Some(risk_check),
            final_action,
            final_position_pct: final_pct,
            final_notional,
            pipeline_steps,
            blockers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_orchestrator_accepts_high_ev_signal() {
        let risk_engine = RiskEngine::new(crate::backtest::risk_engine::RiskConfig::default());
        let orchestrator = DecisionOrchestrator::with_defaults(risk_engine);

        let signal = make_signal("long", 0.85, 0.9, 300.0);
        let account = AccountState::new(Uuid::new_v4(), 100000.0, Utc::now());

        let result = orchestrator.orchestrate(
            &signal,
            Some(MarketRegime::TrendingBull),
            &account,
            100.0,
            0.6,
            0.0,
        );

        assert!(result.can_execute(), "高 EV 信号应通过所有检查: blockers={:?}", result.blockers);
        assert_eq!(result.final_action, DecisionAction::OpenLong);
        assert!(result.final_notional > 0.0);
        assert!(result.pipeline_steps.len() >= 4);
    }

    #[test]
    fn test_orchestrator_rejects_crisis_regime() {
        let risk_engine = RiskEngine::new(crate::backtest::risk_engine::RiskConfig::default());
        let orchestrator = DecisionOrchestrator::with_defaults(risk_engine);

        let signal = make_signal("long", 0.9, 0.9, 500.0);
        let account = AccountState::new(Uuid::new_v4(), 100000.0, Utc::now());

        let result = orchestrator.orchestrate(
            &signal,
            Some(MarketRegime::Crisis),
            &account,
            100.0,
            0.6,
            0.0,
        );

        assert!(!result.can_execute());
        assert_eq!(result.final_action, DecisionAction::Hold);
        assert!(result.blockers.iter().any(|b| b.contains("crisis")));
    }

    #[test]
    fn test_orchestrator_rejects_low_ev() {
        let risk_engine = RiskEngine::new(crate::backtest::risk_engine::RiskConfig::default());
        let orchestrator = DecisionOrchestrator::with_defaults(risk_engine);

        let signal = make_signal("long", 0.3, 0.2, 10.0);
        let account = AccountState::new(Uuid::new_v4(), 100000.0, Utc::now());

        let result = orchestrator.orchestrate(
            &signal,
            Some(MarketRegime::Ranging),
            &account,
            100.0,
            0.6,
            0.0,
        );

        assert!(!result.can_execute());
        assert_eq!(result.final_action, DecisionAction::Hold);
    }

    #[test]
    fn test_orchestrator_has_trace_id() {
        let risk_engine = RiskEngine::new(crate::backtest::risk_engine::RiskConfig::default());
        let orchestrator = DecisionOrchestrator::with_defaults(risk_engine);

        let signal = make_signal("hold", 0.5, 0.5, 0.0);
        let account = AccountState::new(Uuid::new_v4(), 100000.0, Utc::now());

        let result = orchestrator.orchestrate(
            &signal,
            None,
            &account,
            100.0,
            0.6,
            0.0,
        );

        assert!(result.pipeline_steps.iter().all(|s| s.contains(&result.trace_id.to_string())));
    }
}
