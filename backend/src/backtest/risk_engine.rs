//! Simple risk engine - position sizing, leverage, daily loss control.
//! 风险控制引擎

use crate::backtest::models::{AccountState, RiskCheckResult, TradeIntent};

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_single_position_pct: f64,
    pub max_total_leverage: f64,
    pub max_daily_loss_pct: f64,
    pub min_signal_confidence: f64,
    pub min_signal_strength: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_single_position_pct: 0.1,
            max_total_leverage: 3.0,
            max_daily_loss_pct: 0.03,
            min_signal_confidence: 0.3,
            min_signal_strength: 0.2,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DailyStats {
    pub day_pnl_start: f64,
    pub current_day_start_equity: f64,
}

#[derive(Debug, Clone)]
pub struct RiskEngine {
    pub config: RiskConfig,
    pub daily: DailyStats,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self { config, daily: DailyStats::default() }
    }

    pub fn reset_daily(&mut self, current_realized_pnl: f64, current_equity: f64) {
        self.daily.day_pnl_start = current_realized_pnl;
        self.daily.current_day_start_equity = current_equity;
    }

    /// Validate an intent's notional against portfolio constraints.
    pub fn validate_intent(
        &self,
        intent: &TradeIntent,
        account: &AccountState,
        existing_asset_notional: f64,
    ) -> RiskCheckResult {
        // 1. Daily loss
        let day_pnl = account.realized_pnl + account.unrealized_pnl - self.daily.day_pnl_start;
        let day_loss_pct = if self.daily.current_day_start_equity > 0.0 {
            -day_pnl / self.daily.current_day_start_equity
        } else {
            0.0
        };
        if day_loss_pct > self.config.max_daily_loss_pct {
            return RiskCheckResult::reject(format!(
                "daily_loss_exceeded: {}% > {}%",
                day_loss_pct * 100.0,
                self.config.max_daily_loss_pct * 100.0
            ));
        }

        // 2. Proposed notional vs single-asset cap
        let proposed_notional = intent.target_notional.unwrap_or(0.0);
        let single_asset_cap = account.total_equity.max(0.0) * self.config.max_single_position_pct;

        // If this is an increase / open, ensure new aggregate notional <= cap
        let new_notional = existing_asset_notional + proposed_notional;
        if new_notional > single_asset_cap && proposed_notional > 0.0 {
            // Reduce to cap
            let reduced = (single_asset_cap - existing_asset_notional).max(0.0);
            if reduced <= 0.0 {
                return RiskCheckResult::reject("single_asset_exposure_exceeded".to_string());
            }
            return RiskCheckResult {
                passed: true,
                reasons: vec![format!("reduced_to_single_asset_cap: {}", reduced)],
                reduced_notional: Some(reduced),
            };
        }

        // 3. Total leverage cap
        let new_total_notional = account.total_notional + proposed_notional;
        if account.total_equity > 0.0 {
            let new_leverage = new_total_notional / account.total_equity;
            if new_leverage > self.config.max_total_leverage {
                let allowed = account.total_equity * self.config.max_total_leverage - account.total_notional;
                if allowed <= 0.0 {
                    return RiskCheckResult::reject("total_leverage_exceeded".to_string());
                }
                return RiskCheckResult {
                    passed: true,
                    reasons: vec!["reduced_for_leverage_cap".into()],
                    reduced_notional: Some(allowed),
                };
            }
        }

        RiskCheckResult::pass()
    }
}
