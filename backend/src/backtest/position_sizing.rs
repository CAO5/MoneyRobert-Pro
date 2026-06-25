//! Position Sizing & Money Management
//! 仓位计算与资金管理
//!
//! 依据《系统评估与演进规划》第三阶段任务 1：
//! Fractional Kelly + 波动率目标
//!
//! 替代当前的线性置信度缩放（confidence * constant），
//! 采用经过校准的 Fractional Kelly 公式和波动率目标仓位。

use serde::{Deserialize, Serialize};

/// 仓位计算配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizingConfig {
    /// Kelly 分数（0.0-1.0）
    /// 0.0 = 不交易，0.5 = 半 Kelly，1.0 = 全 Kelly
    /// 实践中通常使用 0.25-0.5 以降低方差
    pub kelly_fraction: f64,
    /// 波动率目标（年化，如 0.15 = 15%）
    /// 仓位 = 波动率目标 / 资产波动率
    pub volatility_target: f64,
    /// 单笔最大风险占比（如 0.005 = 0.5% NAV）
    /// 每笔交易最多损失 NAV 的此比例
    pub max_risk_per_trade: f64,
    /// 单资产最大仓位占比
    pub max_position_pct: f64,
    /// 组合最大杠杆
    pub max_leverage: f64,
    /// 最小仓位占比（低于此值不交易）
    pub min_position_pct: f64,
}

impl Default for PositionSizingConfig {
    fn default() -> Self {
        Self {
            kelly_fraction: 0.25,       // 1/4 Kelly
            volatility_target: 0.15,    // 15% 年化波动率目标
            max_risk_per_trade: 0.005,   // 单笔最大风险 0.5%
            max_position_pct: 0.10,     // 单资产最大 10%
            max_leverage: 3.0,           // 最大 3 倍杠杆
            min_position_pct: 0.01,      // 最小 1%
        }
    }
}

/// 仓位计算结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSizingResult {
    /// 建议仓位占比（占 NAV）
    pub position_pct: f64,
    /// 建议杠杆
    pub leverage: f64,
    /// Kelly 原始仓位（未取分数）
    pub kelly_raw: f64,
    /// 波动率目标仓位
    pub vol_target_pct: f64,
    /// 单笔风险仓位
    pub risk_based_pct: f64,
    /// 止损价格（基于 max_risk_per_trade）
    pub stop_loss_price: Option<f64>,
    /// 使用的计算方法
    pub method: PositionMethod,
    /// 调整原因
    pub reason: String,
}

/// 仓位计算方法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionMethod {
    /// Fractional Kelly
    Kelly,
    /// 波动率目标
    VolatilityTarget,
    /// 单笔风险预算
    RiskBudget,
    /// 最小值（三者取最小，保守策略）
    ConservativeMin,
}

impl PositionMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Kelly => "kelly",
            Self::VolatilityTarget => "volatility_target",
            Self::RiskBudget => "risk_budget",
            Self::ConservativeMin => "conservative_min",
        }
    }
}

/// Fractional Kelly 仓位计算引擎
pub struct PositionSizingEngine {
    config: PositionSizingConfig,
}

impl PositionSizingEngine {
    pub fn new(config: PositionSizingConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(PositionSizingConfig::default())
    }

    /// 计算建议仓位
    ///
    /// 输入：
    /// - entry_price: 入场价格
    /// - win_probability: 校准后的胜率（0.0-1.0）
    /// - avg_win: 平均盈利比例（如 0.03 = 3%）
    /// - avg_loss: 平均亏损比例（如 0.02 = 2%）
    /// - asset_volatility: 资产年化波动率（如 0.6 = 60%）
    /// - stop_loss_pct: 止损比例（如 0.02 = 2%）
    pub fn calculate(
        &self,
        entry_price: f64,
        win_probability: f64,
        avg_win: f64,
        avg_loss: f64,
        asset_volatility: f64,
        stop_loss_pct: Option<f64>,
    ) -> PositionSizingResult {
        // 1. Kelly 公式：f* = (p*b - q) / b
        //    其中 b = avg_win/avg_loss（盈亏比），p = 胜率，q = 1-p
        let kelly_raw = if avg_loss > 0.0 {
            let b = avg_win / avg_loss;
            let p = win_probability.clamp(0.01, 0.99);
            let q = 1.0 - p;
            let f = (p * b - q) / b;
            f.max(0.0) // Kelly 不为负
        } else {
            0.0
        };

        // 2. Fractional Kelly
        let kelly_pct = (kelly_raw * self.config.kelly_fraction)
            .min(self.config.max_position_pct);

        // 3. 波动率目标仓位
        //    position = volatility_target / asset_volatility
        let vol_target_pct = if asset_volatility > 0.0 {
            (self.config.volatility_target / asset_volatility)
                .min(self.config.max_position_pct)
        } else {
            0.0
        };

        // 4. 单笔风险预算仓位
        //    position = max_risk_per_trade / stop_loss_pct
        let risk_based_pct = match stop_loss_pct {
            Some(sl) if sl > 0.0 => {
                (self.config.max_risk_per_trade / sl)
                    .min(self.config.max_position_pct)
            }
            _ => self.config.max_risk_per_trade / 0.02, // 默认 2% 止损
        };

        // 5. 保守策略：三者取最小
        let position_pct = kelly_pct
            .min(vol_target_pct)
            .min(risk_based_pct)
            .min(self.config.max_position_pct)
            .max(0.0);

        // 6. 杠杆计算
        let leverage = if position_pct > 0.0 {
            (position_pct / self.config.max_position_pct * self.config.max_leverage)
                .min(self.config.max_leverage)
                .max(1.0)
        } else {
            1.0
        };

        // 7. 止损价格
        let stop_loss_price = stop_loss_pct
            .map(|sl| entry_price * (1.0 - sl))
            .or(Some(entry_price * (1.0 - self.config.max_risk_per_trade / position_pct.max(0.01))));

        // 8. 判断是否达到最小仓位
        let (final_pct, method, reason) = if position_pct < self.config.min_position_pct {
            (0.0, PositionMethod::ConservativeMin, "仓位低于最小阈值，不交易".to_string())
        } else if position_pct == kelly_pct {
            (position_pct, PositionMethod::Kelly, "Fractional Kelly 仓位".to_string())
        } else if position_pct == vol_target_pct {
            (position_pct, PositionMethod::VolatilityTarget, "波动率目标仓位".to_string())
        } else {
            (position_pct, PositionMethod::RiskBudget, "单笔风险预算仓位".to_string())
        };

        PositionSizingResult {
            position_pct: final_pct,
            leverage,
            kelly_raw,
            vol_target_pct,
            risk_based_pct,
            stop_loss_price,
            method,
            reason,
        }
    }

    /// 从历史交易统计计算胜率和盈亏比
    ///
    /// 输入：历史交易的 PnL 列表
    /// 输出：(胜率, 平均盈利比例, 平均亏损比例)
    pub fn estimate_from_history(pnls: &[f64]) -> (f64, f64, f64) {
        if pnls.is_empty() {
            return (0.5, 0.0, 0.0);
        }
        let wins: Vec<&f64> = pnls.iter().filter(|&&p| p > 0.0).collect();
        let losses: Vec<&f64> = pnls.iter().filter(|&&p| p < 0.0).collect();
        let win_prob = wins.len() as f64 / pnls.len() as f64;
        let avg_win = if wins.is_empty() {
            0.0
        } else {
            wins.iter().map(|p| **p).sum::<f64>() / wins.len() as f64
        };
        let avg_loss = if losses.is_empty() {
            0.0
        } else {
            losses.iter().map(|p| p.abs()).sum::<f64>() / losses.len() as f64
        };
        (win_prob, avg_win, avg_loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kelly_positive_expectancy() {
        // 胜率 60%，盈亏比 1.5
        let engine = PositionSizingEngine::with_defaults();
        let result = engine.calculate(
            100.0,
            0.6,   // 60% 胜率
            0.03,  // 3% 平均盈利
            0.02,  // 2% 平均亏损
            0.6,   // 60% 年化波动率
            Some(0.02),
        );
        assert!(result.position_pct > 0.0, "正期望应建议交易");
        assert!(result.kelly_raw > 0.0);
    }

    #[test]
    fn test_kelly_negative_expectancy() {
        // 胜率 30%，盈亏比 1.0
        let engine = PositionSizingEngine::with_defaults();
        let result = engine.calculate(
            100.0,
            0.3,
            0.02,
            0.02,
            0.6,
            Some(0.02),
        );
        assert_eq!(result.position_pct, 0.0, "负期望应不交易");
        assert_eq!(result.kelly_raw, 0.0);
    }

    #[test]
    fn test_volatility_target_reduces_high_vol() {
        // 使用较大的 max_position_pct 避免被 cap 掩盖差异
        let config = PositionSizingConfig {
            max_position_pct: 1.0, // 放大上限以观察波动率目标差异
            ..Default::default()
        };
        let engine = PositionSizingEngine::new(config);
        // 高波动率资产应降低仓位
        let low_vol = engine.calculate(100.0, 0.6, 0.03, 0.02, 0.3, Some(0.02));
        let high_vol = engine.calculate(100.0, 0.6, 0.03, 0.02, 1.2, Some(0.02));
        assert!(
            high_vol.vol_target_pct < low_vol.vol_target_pct,
            "高波动率 ({}) 应低于低波动率 ({}) 的波动率目标仓位",
            high_vol.vol_target_pct,
            low_vol.vol_target_pct
        );
    }

    #[test]
    fn test_risk_budget_limits_position() {
        let engine = PositionSizingEngine::with_defaults();
        // 大止损应被风险预算限制
        let result = engine.calculate(
            100.0,
            0.8,
            0.05,
            0.01,
            0.3,
            Some(0.10), // 10% 止损
        );
        // max_risk_per_trade = 0.005, stop_loss = 0.10
        // risk_based = 0.005 / 0.10 = 0.05 = 5%
        assert!(result.risk_based_pct <= 0.05 + 1e-9);
    }

    #[test]
    fn test_position_capped_at_max() {
        let config = PositionSizingConfig {
            max_position_pct: 0.05,
            ..Default::default()
        };
        let engine = PositionSizingEngine::new(config);
        let result = engine.calculate(100.0, 0.9, 0.10, 0.01, 0.1, Some(0.01));
        assert!(result.position_pct <= 0.05, "仓位不应超过最大限制");
    }

    #[test]
    fn test_min_position_threshold() {
        let config = PositionSizingConfig {
            min_position_pct: 0.10,
            max_position_pct: 0.20,
            ..Default::default()
        };
        let engine = PositionSizingEngine::new(config);
        // 低胜率应产生低于最小阈值的仓位
        let result = engine.calculate(100.0, 0.45, 0.01, 0.01, 0.8, Some(0.02));
        assert_eq!(result.position_pct, 0.0, "低于最小阈值应不交易");
    }

    #[test]
    fn test_leverage_scales_with_position() {
        let engine = PositionSizingEngine::with_defaults();
        let small = engine.calculate(100.0, 0.55, 0.02, 0.015, 0.5, Some(0.02));
        let large = engine.calculate(100.0, 0.75, 0.05, 0.01, 0.3, Some(0.01));
        assert!(large.leverage >= small.leverage, "更大仓位应有更高杠杆");
    }

    #[test]
    fn test_stop_loss_price() {
        let engine = PositionSizingEngine::with_defaults();
        let result = engine.calculate(100.0, 0.6, 0.03, 0.02, 0.6, Some(0.02));
        assert!(result.stop_loss_price.is_some());
        let sl = result.stop_loss_price.unwrap();
        assert!(sl < 100.0, "止损应低于入场价");
        assert!((sl - 98.0).abs() < 1.0, "2% 止损应在 98 附近");
    }

    #[test]
    fn test_estimate_from_history() {
        let pnls = vec![0.03, -0.02, 0.04, -0.01, 0.02, -0.03, 0.05, 0.01];
        let (win_prob, avg_win, avg_loss) = PositionSizingEngine::estimate_from_history(&pnls);
        assert!((win_prob - 0.625).abs() < 1e-9, "胜率应为 5/8 = 0.625");
        assert!(avg_win > 0.0);
        assert!(avg_loss > 0.0);
    }

    #[test]
    fn test_estimate_from_empty_history() {
        let (win_prob, avg_win, avg_loss) = PositionSizingEngine::estimate_from_history(&[]);
        assert!((win_prob - 0.5).abs() < 1e-9);
        assert_eq!(avg_win, 0.0);
        assert_eq!(avg_loss, 0.0);
    }

    #[test]
    fn test_conservive_min_takes_smallest() {
        // 验证保守策略取三者最小
        let engine = PositionSizingEngine::with_defaults();
        let result = engine.calculate(100.0, 0.7, 0.10, 0.02, 0.9, Some(0.05));
        // kelly 可能很大，vol_target = 0.15/0.9 = 0.167
        // risk_based = 0.005/0.05 = 0.10
        // 最小应为 risk_based = 0.10
        assert!(result.position_pct <= 0.10 + 1e-9);
        assert_eq!(result.method, PositionMethod::RiskBudget);
    }
}
