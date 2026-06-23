//! Portfolio Risk Management
//! 组合风险管理
//!
//! 依据《系统评估与演进规划》第三阶段任务 2：
//! 相关性、风险贡献、CVaR 和流动性约束
//!
//! 在单资产仓位计算（position_sizing）之上，引入组合层面的约束：
//! - 相关性矩阵与组合波动率
//! - 风险贡献（Risk Contribution）均衡
//! - CVaR 预算约束
//! - 流动性约束（基于成交量的仓位上限）

use serde::{Deserialize, Serialize};

/// 组合风险配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRiskConfig {
    /// 组合 CVaR 预算（占 NAV 比例，如 0.05 = 5%）
    pub max_portfolio_cvar: f64,
    /// 单资产风险贡献上限（占组合风险比例，如 0.30 = 30%）
    pub max_risk_concentration: f64,
    /// 流动性约束：单资产仓位占该资产近期成交量的最大比例
    /// 例如 0.01 = 最多吃掉 1% 的成交量，避免冲击成本
    pub max_volume_participation: f64,
    /// 相关性阈值：高于此值的两资产视为高度相关，合并风险敞口
    pub high_correlation_threshold: f64,
    /// 高相关资产组合的最大合并敞口（占 NAV）
    pub max_correlated_exposure: f64,
}

impl Default for PortfolioRiskConfig {
    fn default() -> Self {
        Self {
            max_portfolio_cvar: 0.05,       // 组合 CVaR 不超过 5%
            max_risk_concentration: 0.30,    // 单资产风险贡献不超过 30%
            max_volume_participation: 0.01,  // 最多吃掉 1% 成交量
            high_correlation_threshold: 0.70, // 相关系数 > 0.70 视为高相关
            max_correlated_exposure: 0.20,   // 高相关组合最大 20% NAV
        }
    }
}

/// 单资产的风险度量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRiskProfile {
    pub symbol: String,
    /// 当前仓位占比（占 NAV）
    pub position_pct: f64,
    /// 资产年化波动率
    pub volatility: f64,
    /// 近期日均成交量（USD notional）
    pub avg_daily_volume: f64,
    /// 该资产的边际风险贡献（占组合风险比例）
    pub risk_contribution: f64,
}

/// 组合风险检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRiskCheck {
    /// 是否通过所有检查
    pub passed: bool,
    /// 组合 CVaR（占 NAV）
    pub portfolio_cvar: f64,
    /// 组合波动率（年化）
    pub portfolio_volatility: f64,
    /// 违反的约束列表
    pub violations: Vec<String>,
    /// 建议的仓位调整（symbol -> 调整后的仓位占比）
    pub adjusted_positions: HashMap<String, f64>,
}

use std::collections::HashMap;

/// 组合风险管理引擎
pub struct PortfolioRiskEngine {
    config: PortfolioRiskConfig,
}

impl PortfolioRiskEngine {
    pub fn new(config: PortfolioRiskConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(PortfolioRiskConfig::default())
    }

    /// 查找两资产的相关系数（支持双向查找）
    fn lookup_correlation(
        correlation_matrix: &HashMap<(String, String), f64>,
        a: &str,
        b: &str,
    ) -> f64 {
        if a == b {
            return 1.0;
        }
        correlation_matrix
            .get(&(a.to_string(), b.to_string()))
            .or_else(|| correlation_matrix.get(&(b.to_string(), a.to_string())))
            .copied()
            .unwrap_or(0.0)
    }

    /// 计算组合波动率
    ///
    /// σ_p = sqrt(w' Σ w)
    /// 其中 w 是仓位权重向量，Σ 是协方差矩阵
    /// 协方差矩阵由波动率和相关系数矩阵推导：Σ_ij = σ_i * σ_j * ρ_ij
    pub fn portfolio_volatility(
        &self,
        assets: &[AssetRiskProfile],
        correlation_matrix: &HashMap<(String, String), f64>,
    ) -> f64 {
        if assets.is_empty() {
            return 0.0;
        }
        let mut variance = 0.0;
        for i in 0..assets.len() {
            for j in 0..assets.len() {
                let w_i = assets[i].position_pct;
                let w_j = assets[j].position_pct;
                let sigma_i = assets[i].volatility;
                let sigma_j = assets[j].volatility;
                let rho = Self::lookup_correlation(correlation_matrix, &assets[i].symbol, &assets[j].symbol);
                variance += w_i * w_j * sigma_i * sigma_j * rho;
            }
        }
        variance.max(0.0).sqrt()
    }

    /// 计算组合 CVaR（占 NAV）
    ///
    /// 简化模型：假设收益服从正态分布
    /// CVaR_95 = -E[r | r < VaR_95]
    /// 对于正态分布：CVaR_95 = σ * φ(z_95) / (1 - 0.95)
    /// 其中 z_95 = -1.645，φ(z) 是标准正态密度函数
    /// φ(-1.645) ≈ 0.1031，所以 CVaR_95 ≈ σ * 2.063
    pub fn portfolio_cvar(&self, portfolio_volatility: f64) -> f64 {
        // 正态分布假设下的 95% CVaR 因子
        // z_0.05 = -1.645
        // φ(z) = (1/√(2π)) * exp(-z²/2)
        // CVaR = σ * φ(z) / α，其中 α = 0.05
        let z = 1.645_f64;
        let phi = (1.0 / (2.0 * std::f64::consts::PI).sqrt()) * (-z * z / 2.0).exp();
        let cvar_factor = phi / 0.05; // ≈ 2.063
        portfolio_volatility * cvar_factor
    }

    /// 计算每个资产的边际风险贡献（Marginal Risk Contribution）
    ///
    /// MRC_i = ∂σ_p / ∂w_i = (Σ w)_i / σ_p
    /// 风险贡献 RC_i = w_i * MRC_i
    /// 归一化：RC_i% = RC_i / σ_p
    pub fn risk_contributions(
        &self,
        assets: &[AssetRiskProfile],
        correlation_matrix: &HashMap<(String, String), f64>,
    ) -> Vec<f64> {
        if assets.is_empty() {
            return vec![];
        }
        let portfolio_vol = self.portfolio_volatility(assets, correlation_matrix);
        if portfolio_vol <= 0.0 {
            return vec![0.0; assets.len()];
        }

        let n = assets.len();
        let mut contributions = vec![0.0; n];

        // 计算 (Σ w)_i = Σ_j (σ_i * σ_j * ρ_ij * w_j)
        for i in 0..n {
            let mut sigma_w_sum = 0.0;
            for j in 0..n {
                let rho = Self::lookup_correlation(correlation_matrix, &assets[i].symbol, &assets[j].symbol);
                sigma_w_sum += assets[i].volatility * assets[j].volatility * rho * assets[j].position_pct;
            }
            // 边际风险贡献
            let mrc = sigma_w_sum / portfolio_vol;
            // 风险贡献 = w_i * MRC_i
            contributions[i] = assets[i].position_pct * mrc;
        }

        // 归一化为占比
        let total_rc: f64 = contributions.iter().sum();
        if total_rc > 0.0 {
            for c in &mut contributions {
                *c /= total_rc;
            }
        }
        contributions
    }

    /// 流动性约束：基于成交量的仓位上限
    ///
    /// 最大仓位 = max_volume_participation * avg_daily_volume / NAV
    pub fn liquidity_constrained_position(
        &self,
        avg_daily_volume: f64,
        nav: f64,
    ) -> f64 {
        if nav <= 0.0 || avg_daily_volume <= 0.0 {
            return 0.0;
        }
        (self.config.max_volume_participation * avg_daily_volume / nav).min(1.0)
    }

    /// 检测高相关资产组，并限制合并敞口
    ///
    /// 返回需要调整的资产及其调整后的仓位
    fn enforce_correlation_limit(
        &self,
        assets: &[AssetRiskProfile],
        correlation_matrix: &HashMap<(String, String), f64>,
    ) -> Vec<(String, f64)> {
        let mut adjustments = Vec::new();
        let n = assets.len();

        // 对每个资产，找到与其高相关的其他资产
        for i in 0..n {
            let mut correlated_exposure = assets[i].position_pct;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let rho = Self::lookup_correlation(correlation_matrix, &assets[i].symbol, &assets[j].symbol);
                if rho > self.config.high_correlation_threshold {
                    correlated_exposure += assets[j].position_pct;
                }
            }
            // 如果合并敞口超过限制，按比例缩减
            if correlated_exposure > self.config.max_correlated_exposure {
                let scale = self.config.max_correlated_exposure / correlated_exposure;
                let adjusted = assets[i].position_pct * scale;
                adjustments.push((assets[i].symbol.clone(), adjusted));
            }
        }
        adjustments
    }

    /// 综合组合风险检查
    ///
    /// 输入：各资产风险画像 + 相关性矩阵 + NAV
    /// 输出：是否通过检查 + 违反的约束 + 建议调整
    pub fn check(
        &self,
        assets: &[AssetRiskProfile],
        correlation_matrix: &HashMap<(String, String), f64>,
        nav: f64,
    ) -> PortfolioRiskCheck {
        let mut violations = Vec::new();
        let mut adjusted: HashMap<String, f64> = HashMap::new();

        // 1. 计算组合波动率和 CVaR
        let portfolio_vol = self.portfolio_volatility(assets, correlation_matrix);
        let portfolio_cvar = self.portfolio_cvar(portfolio_vol);

        if portfolio_cvar > self.config.max_portfolio_cvar {
            violations.push(format!(
                "portfolio_cvar_exceeded: {:.4} > {:.4}",
                portfolio_cvar, self.config.max_portfolio_cvar
            ));
            // 按比例缩减所有仓位以满足 CVaR 预算
            let scale = if portfolio_cvar > 0.0 {
                self.config.max_portfolio_cvar / portfolio_cvar
            } else {
                1.0
            };
            for a in assets {
                adjusted.insert(a.symbol.clone(), a.position_pct * scale);
            }
        }

        // 2. 风险贡献集中度检查
        let contributions = self.risk_contributions(assets, correlation_matrix);
        for (i, &rc) in contributions.iter().enumerate() {
            if rc > self.config.max_risk_concentration {
                violations.push(format!(
                    "risk_concentration_{}: {:.4} > {:.4}",
                    assets[i].symbol, rc, self.config.max_risk_concentration
                ));
                // 缩减该资产仓位使其风险贡献回到上限
                let scale = self.config.max_risk_concentration / rc;
                let adj = adjusted
                    .get(&assets[i].symbol)
                    .copied()
                    .unwrap_or(assets[i].position_pct)
                    * scale;
                adjusted.insert(assets[i].symbol.clone(), adj);
            }
        }

        // 3. 相关性约束
        let corr_adjustments = self.enforce_correlation_limit(assets, correlation_matrix);
        for (symbol, adj) in corr_adjustments {
            violations.push(format!("correlated_exposure_{}_reduced", symbol));
            // 取更小的调整值
            let current = adjusted.get(&symbol).copied().unwrap_or_else(|| {
                assets.iter().find(|a| a.symbol == symbol).map(|a| a.position_pct).unwrap_or(0.0)
            });
            adjusted.insert(symbol, adj.min(current));
        }

        // 4. 流动性约束
        for a in assets {
            let liq_max = self.liquidity_constrained_position(a.avg_daily_volume, nav);
            if a.position_pct > liq_max && liq_max > 0.0 {
                violations.push(format!(
                    "liquidity_{}_exceeded: {:.4} > {:.4}",
                    a.symbol, a.position_pct, liq_max
                ));
                let current = adjusted.get(&a.symbol).copied().unwrap_or(a.position_pct);
                adjusted.insert(a.symbol.clone(), current.min(liq_max));
            }
        }

        let passed = violations.is_empty();
        PortfolioRiskCheck {
            passed,
            portfolio_cvar,
            portfolio_volatility: portfolio_vol,
            violations,
            adjusted_positions: adjusted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_asset(symbol: &str, pos: f64, vol: f64, volume: f64) -> AssetRiskProfile {
        AssetRiskProfile {
            symbol: symbol.to_string(),
            position_pct: pos,
            volatility: vol,
            avg_daily_volume: volume,
            risk_contribution: 0.0,
        }
    }

    #[test]
    fn test_portfolio_volatility_single_asset() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets = vec![make_asset("BTC", 0.10, 0.60, 1_000_000.0)];
        let corr = HashMap::new();
        let vol = engine.portfolio_volatility(&assets, &corr);
        // 单资产：σ_p = w * σ = 0.10 * 0.60 = 0.06
        assert!((vol - 0.06).abs() < 1e-9, "单资产组合波动率应为 0.06，实际: {}", vol);
    }

    #[test]
    fn test_portfolio_volatility_uncorrelated() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets = vec![
            make_asset("BTC", 0.10, 0.60, 1_000_000.0),
            make_asset("ETH", 0.10, 0.70, 500_000.0),
        ];
        let corr = HashMap::new(); // 无相关性
        let vol = engine.portfolio_volatility(&assets, &corr);
        // 不相关：σ_p = sqrt((0.10*0.60)² + (0.10*0.70)²) = sqrt(0.0036 + 0.0049) = sqrt(0.0085)
        let expected = (0.0085_f64).sqrt();
        assert!((vol - expected).abs() < 1e-9, "不相关组合波动率应为 {}，实际: {}", expected, vol);
    }

    #[test]
    fn test_portfolio_volatility_correlated() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets = vec![
            make_asset("BTC", 0.10, 0.60, 1_000_000.0),
            make_asset("ETH", 0.10, 0.70, 500_000.0),
        ];
        let mut corr = HashMap::new();
        corr.insert(("BTC".to_string(), "ETH".to_string()), 0.80);
        let vol = engine.portfolio_volatility(&assets, &corr);
        // 相关：σ_p² = w1²σ1² + w2²σ2² + 2*w1*w2*σ1*σ2*ρ
        // = 0.0036 + 0.0049 + 2*0.10*0.10*0.60*0.70*0.80
        // = 0.0085 + 0.00672 = 0.01522
        let expected = (0.01522_f64).sqrt();
        assert!((vol - expected).abs() < 1e-9, "相关组合波动率应为 {}，实际: {}", expected, vol);
    }

    #[test]
    fn test_portfolio_cvar_positive() {
        let engine = PortfolioRiskEngine::with_defaults();
        let cvar = engine.portfolio_cvar(0.10);
        // CVaR = σ * 2.063 ≈ 0.2063
        assert!(cvar > 0.20 && cvar < 0.21, "CVaR 应在 0.20-0.21 之间，实际: {}", cvar);
    }

    #[test]
    fn test_portfolio_cvar_zero_volatility() {
        let engine = PortfolioRiskEngine::with_defaults();
        let cvar = engine.portfolio_cvar(0.0);
        assert_eq!(cvar, 0.0);
    }

    #[test]
    fn test_risk_contributions_sum_to_one() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets = vec![
            make_asset("BTC", 0.10, 0.60, 1_000_000.0),
            make_asset("ETH", 0.10, 0.70, 500_000.0),
            make_asset("SOL", 0.05, 0.90, 200_000.0),
        ];
        let mut corr = HashMap::new();
        corr.insert(("BTC".to_string(), "ETH".to_string()), 0.50);
        corr.insert(("BTC".to_string(), "SOL".to_string()), 0.30);
        corr.insert(("ETH".to_string(), "SOL".to_string()), 0.40);
        let contributions = engine.risk_contributions(&assets, &corr);
        let sum: f64 = contributions.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9, "风险贡献总和应为 1，实际: {}", sum);
    }

    #[test]
    fn test_risk_contributions_higher_vol_contributes_more() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets = vec![
            make_asset("LOW", 0.10, 0.20, 1_000_000.0),
            make_asset("HIGH", 0.10, 0.80, 1_000_000.0),
        ];
        let corr = HashMap::new();
        let contributions = engine.risk_contributions(&assets, &corr);
        assert!(
            contributions[1] > contributions[0],
            "高波动率资产应有更大风险贡献，HIGH={} LOW={}",
            contributions[1],
            contributions[0]
        );
    }

    #[test]
    fn test_liquidity_constraint() {
        let engine = PortfolioRiskEngine::with_defaults();
        // max_volume_participation = 0.01, volume = 1_000_000, nav = 100_000
        // max_pos = 0.01 * 1_000_000 / 100_000 = 0.10
        let max_pos = engine.liquidity_constrained_position(1_000_000.0, 100_000.0);
        assert!((max_pos - 0.10).abs() < 1e-9, "流动性约束仓位应为 0.10，实际: {}", max_pos);
    }

    #[test]
    fn test_liquidity_constraint_zero_nav() {
        let engine = PortfolioRiskEngine::with_defaults();
        let max_pos = engine.liquidity_constrained_position(1_000_000.0, 0.0);
        assert_eq!(max_pos, 0.0);
    }

    #[test]
    fn test_check_passes_with_small_positions() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets = vec![
            make_asset("BTC", 0.01, 0.60, 10_000_000.0),
            make_asset("ETH", 0.01, 0.70, 5_000_000.0),
            make_asset("SOL", 0.01, 0.80, 2_000_000.0),
            make_asset("ADA", 0.01, 0.75, 1_000_000.0),
        ];
        let mut corr = HashMap::new();
        corr.insert(("BTC".to_string(), "ETH".to_string()), 0.50);
        corr.insert(("BTC".to_string(), "SOL".to_string()), 0.40);
        corr.insert(("BTC".to_string(), "ADA".to_string()), 0.30);
        corr.insert(("ETH".to_string(), "SOL".to_string()), 0.45);
        corr.insert(("ETH".to_string(), "ADA".to_string()), 0.35);
        corr.insert(("SOL".to_string(), "ADA".to_string()), 0.30);
        let result = engine.check(&assets, &corr, 100_000.0);
        assert!(result.passed, "小仓位应通过检查，违反: {:?}", result.violations);
    }

    #[test]
    fn test_check_fails_on_cvar_exceeded() {
        let engine = PortfolioRiskEngine::with_defaults();
        // 大仓位 + 高波动率 → CVaR 超限
        let assets = vec![
            make_asset("BTC", 0.50, 0.80, 10_000_000.0),
        ];
        let corr = HashMap::new();
        let result = engine.check(&assets, &corr, 100_000.0);
        assert!(!result.passed, "大仓位应触发 CVaR 超限");
        assert!(
            result.violations.iter().any(|v| v.contains("cvar")),
            "应包含 CVaR 违规，实际: {:?}",
            result.violations
        );
        // 应建议缩减仓位
        assert!(result.adjusted_positions.contains_key("BTC"));
        let adj = result.adjusted_positions["BTC"];
        assert!(adj < 0.50, "调整后仓位应小于原始 0.50，实际: {}", adj);
    }

    #[test]
    fn test_check_fails_on_risk_concentration() {
        let engine = PortfolioRiskEngine::with_defaults();
        // 一个资产占绝大部分风险
        let assets = vec![
            make_asset("BTC", 0.02, 0.30, 10_000_000.0),
            make_asset("VOL", 0.10, 0.95, 10_000_000.0), // 高波动率
        ];
        let corr = HashMap::new();
        let result = engine.check(&assets, &corr, 100_000.0);
        // VOL 的风险贡献应超过 30%
        assert!(
            result.violations.iter().any(|v| v.contains("risk_concentration")),
            "应触发风险集中度违规，实际: {:?}",
            result.violations
        );
    }

    #[test]
    fn test_check_fails_on_correlation_limit() {
        let engine = PortfolioRiskEngine::with_defaults();
        // 两个高相关资产，合并敞口超限
        let assets = vec![
            make_asset("BTC", 0.15, 0.60, 10_000_000.0),
            make_asset("WBTC", 0.15, 0.60, 10_000_000.0),
        ];
        let mut corr = HashMap::new();
        corr.insert(("BTC".to_string(), "WBTC".to_string()), 0.95); // 极高相关
        let result = engine.check(&assets, &corr, 100_000.0);
        // 合并敞口 0.30 > max_correlated_exposure 0.20
        assert!(
            result.violations.iter().any(|v| v.contains("correlated")),
            "应触发相关性违规，实际: {:?}",
            result.violations
        );
    }

    #[test]
    fn test_check_fails_on_liquidity() {
        let engine = PortfolioRiskEngine::with_defaults();
        // 低成交量资产，仓位超过流动性约束
        // max_volume_participation = 0.01, volume = 100_000, nav = 100_000
        // max_pos = 0.01 * 100_000 / 100_000 = 0.01
        let assets = vec![make_asset("ILLIQUID", 0.05, 0.30, 100_000.0)];
        let corr = HashMap::new();
        let result = engine.check(&assets, &corr, 100_000.0);
        assert!(
            result.violations.iter().any(|v| v.contains("liquidity")),
            "应触发流动性违规，实际: {:?}",
            result.violations
        );
    }

    #[test]
    fn test_empty_portfolio() {
        let engine = PortfolioRiskEngine::with_defaults();
        let assets: Vec<AssetRiskProfile> = vec![];
        let corr = HashMap::new();
        let result = engine.check(&assets, &corr, 100_000.0);
        assert!(result.passed);
        assert_eq!(result.portfolio_volatility, 0.0);
        assert_eq!(result.portfolio_cvar, 0.0);
    }
}
