//! Market Regime Detection
//! 市场状态识别
//!
//! 5 类市场状态（依据《系统评估与演进规划》3.1 节）：
//! - TrendingBull: 趋势性上涨（高 ADX + 正收益）
//! - TrendingBear: 趋势性下跌（高 ADX + 负收益）
//! - Ranging: 震荡（低 ADX + 低波动率）
//! - HighVolatility: 高波动（高波动率分位数 + 低 ADX）
//! - Crisis: 危机（极端负收益 + 高波动率）
//!
//! 算法：基于 ADX（趋势强度）+ 波动率分位数 + 收益率分位数的规则分类器
//! 不依赖 HMM，保持确定性和可解释性

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 市场状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketRegime {
    /// 趋势性上涨
    TrendingBull,
    /// 趋势性下跌
    TrendingBear,
    /// 震荡
    Ranging,
    /// 高波动
    HighVolatility,
    /// 危机
    Crisis,
}

impl MarketRegime {
    pub fn as_str(&self) -> &'static str {
        match self {
            MarketRegime::TrendingBull => "trending_bull",
            MarketRegime::TrendingBear => "trending_bear",
            MarketRegime::Ranging => "ranging",
            MarketRegime::HighVolatility => "high_volatility",
            MarketRegime::Crisis => "crisis",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "trending_bull" => Some(MarketRegime::TrendingBull),
            "trending_bear" => Some(MarketRegime::TrendingBear),
            "ranging" => Some(MarketRegime::Ranging),
            "high_volatility" => Some(MarketRegime::HighVolatility),
            "crisis" => Some(MarketRegime::Crisis),
            _ => None,
        }
    }

    /// 是否为趋势状态
    pub fn is_trending(&self) -> bool {
        matches!(self, MarketRegime::TrendingBull | MarketRegime::TrendingBear)
    }

    /// 是否为看涨状态
    pub fn is_bullish(&self) -> bool {
        matches!(self, MarketRegime::TrendingBull)
    }

    /// 是否为看跌状态
    pub fn is_bearish(&self) -> bool {
        matches!(self, MarketRegime::TrendingBear | MarketRegime::Crisis)
    }
}

/// 市场状态识别配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeConfig {
    /// ADX 阈值：高于此值视为有趋势
    pub adx_threshold: f64,
    /// 波动率分位数阈值：高于此值视为高波动
    pub volatility_percentile_threshold: f64,
    /// 收益率分位数阈值：极端收益判定
    pub return_percentile_extreme: f64,
    /// 危机判定：收益率低于此分位数 + 高波动
    pub crisis_return_percentile: f64,
    /// 滚动窗口长度（K 线数量）
    pub lookback: usize,
}

impl Default for RegimeConfig {
    fn default() -> Self {
        Self {
            adx_threshold: 25.0,           // ADX > 25 视为有趋势
            volatility_percentile_threshold: 0.8,  // 波动率 > 80 分位
            return_percentile_extreme: 0.9,       // 收益率 > 90 分位为极端
            crisis_return_percentile: 0.1,        // 收益率 < 10 分位 + 高波动 = 危机
            lookback: 50,                          // 50 根 K 线滚动窗口
        }
    }
}

/// 市场状态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeSnapshot {
    pub regime: MarketRegime,
    pub confidence: f64,
    pub adx: f64,
    pub volatility_percentile: f64,
    pub return_percentile: f64,
    pub timestamp: DateTime<Utc>,
}

impl Default for RegimeSnapshot {
    fn default() -> Self {
        Self {
            regime: MarketRegime::Ranging,
            confidence: 0.5,
            adx: 0.0,
            volatility_percentile: 0.5,
            return_percentile: 0.5,
            timestamp: Utc::now(),
        }
    }
}

/// 市场状态分类器
pub struct RegimeClassifier {
    config: RegimeConfig,
}

impl RegimeClassifier {
    pub fn new(config: RegimeConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(RegimeConfig::default())
    }

    /// 从 K 线序列计算市场状态
    ///
    /// 输入：按时间升序排列的 K 线（open, high, low, close, volume）
    /// 输出：最新时刻的市场状态快照
    pub fn classify(&self, klines: &[(f64, f64, f64, f64, f64)]) -> Option<RegimeSnapshot> {
        let n = klines.len();
        if n < self.config.lookback.min(20) {
            return None;
        }

        let lookback = self.config.lookback.min(n);
        let window = &klines[n - lookback..];

        // 1. 计算 ADX（趋势强度）
        let adx = self.calculate_adx(window);

        // 2. 计算波动率分位数
        let returns: Vec<f64> = window
            .windows(2)
            .filter_map(|w| {
                let prev_close = w[0].3;
                let curr_close = w[1].3;
                if prev_close > 0.0 {
                    Some((curr_close - prev_close) / prev_close)
                } else {
                    None
                }
            })
            .collect();
        let volatility = self.calculate_volatility(&returns);
        // 计算滚动波动率序列作为历史，用于计算当前波动率的分位数
        let rolling_vols: Vec<f64> = if returns.len() >= 10 {
            returns
                .windows(10)
                .map(|w| {
                    let mean = w.iter().sum::<f64>() / w.len() as f64;
                    let var = w.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / w.len() as f64;
                    var.sqrt()
                })
                .collect()
        } else {
            returns.clone()
        };
        let volatility_percentile = self.percentile_rank(&rolling_vols, volatility);

        // 3. 计算最近收益率分位数
        let recent_return = if window.len() >= 2 {
            let first_close = window[0].3;
            let last_close = window[window.len() - 1].3;
            if first_close > 0.0 {
                (last_close - first_close) / first_close
            } else {
                0.0
            }
        } else {
            0.0
        };
        let return_percentile = self.percentile_rank(&returns, recent_return);

        // 4. 分类
        let (regime, confidence) = self.classify_regime(
            adx,
            volatility_percentile,
            return_percentile,
            recent_return,
        );

        Some(RegimeSnapshot {
            regime,
            confidence,
            adx,
            volatility_percentile,
            return_percentile,
            timestamp: Utc::now(),
        })
    }

    /// 计算 ADX（Average Directional Index）
    /// 简化实现：使用 directional movement 的平滑平均
    fn calculate_adx(&self, klines: &[(f64, f64, f64, f64, f64)]) -> f64 {
        if klines.len() < 2 {
            return 0.0;
        }

        let period = 14.min(klines.len() - 1);
        let mut plus_dm: Vec<f64> = Vec::with_capacity(klines.len());
        let mut minus_dm: Vec<f64> = Vec::with_capacity(klines.len());
        let mut true_ranges: Vec<f64> = Vec::with_capacity(klines.len());

        for i in 1..klines.len() {
            let (_, prev_high, prev_low, prev_close, _) = klines[i - 1];
            let (_, high, low, _, _) = klines[i];

            let up_move = high - prev_high;
            let down_move = prev_low - low;

            let pdm = if up_move > down_move && up_move > 0.0 {
                up_move
            } else {
                0.0
            };
            let mdm = if down_move > up_move && down_move > 0.0 {
                down_move
            } else {
                0.0
            };

            plus_dm.push(pdm);
            minus_dm.push(mdm);

            let tr = (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs());
            true_ranges.push(tr);
        }

        // Wilder 平滑
        let smooth_plus_dm = self.wilder_smooth(&plus_dm, period);
        let smooth_minus_dm = self.wilder_smooth(&minus_dm, period);
        let smooth_tr = self.wilder_smooth(&true_ranges, period);

        if smooth_tr == 0.0 {
            return 0.0;
        }

        let plus_di = 100.0 * smooth_plus_dm / smooth_tr;
        let minus_di = 100.0 * smooth_minus_dm / smooth_tr;

        let dx = if (plus_di + minus_di) > 0.0 {
            100.0 * (plus_di - minus_di).abs() / (plus_di + minus_di)
        } else {
            0.0
        };

        // ADX 是 DX 的平滑平均，简化为最后 DX 值
        dx.min(100.0)
    }

    /// Wilder 平滑（指数移动平均）
    fn wilder_smooth(&self, values: &[f64], period: usize) -> f64 {
        if values.is_empty() || period == 0 {
            return 0.0;
        }
        let p = period.min(values.len());
        if p == 0 {
            return 0.0;
        }

        // 初始 SMA
        let mut smoothed: f64 = values[..p].iter().sum::<f64>() / p as f64;
        // Wilder 平滑：smoothed = (prev * (period - 1) + current) / period
        for &v in &values[p..] {
            smoothed = (smoothed * (p as f64 - 1.0) + v) / p as f64;
        }
        smoothed
    }

    /// 计算波动率（收益率标准差）
    fn calculate_volatility(&self, returns: &[f64]) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        variance.sqrt()
    }

    /// 计算分位数排名（当前值在历史序列中的位置）
    fn percentile_rank(&self, history: &[f64], current: f64) -> f64 {
        if history.is_empty() {
            return 0.5;
        }
        let below = history.iter().filter(|&&v| v < current).count();
        below as f64 / history.len() as f64
    }

    /// 根据指标组合分类市场状态
    fn classify_regime(
        &self,
        adx: f64,
        vol_percentile: f64,
        ret_percentile: f64,
        recent_return: f64,
    ) -> (MarketRegime, f64) {
        let is_trending = adx >= self.config.adx_threshold;
        let is_high_vol = vol_percentile >= self.config.volatility_percentile_threshold;
        let is_extreme_negative = ret_percentile <= self.config.crisis_return_percentile;
        let is_extreme_positive = ret_percentile >= self.config.return_percentile_extreme;

        // 优先级 1：危机（极端负收益 + 高波动）
        if is_extreme_negative && is_high_vol {
            return (MarketRegime::Crisis, 0.9);
        }

        // 优先级 2：趋势性上涨（高 ADX + 正收益）
        if is_trending && recent_return > 0.0 {
            let confidence = (adx / 50.0).min(1.0) * 0.7 + 0.3;
            return (MarketRegime::TrendingBull, confidence);
        }

        // 优先级 3：趋势性下跌（高 ADX + 负收益）
        if is_trending && recent_return < 0.0 {
            let confidence = (adx / 50.0).min(1.0) * 0.7 + 0.3;
            return (MarketRegime::TrendingBear, confidence);
        }

        // 优先级 4：高波动（高波动率分位数 + 低 ADX）
        if is_high_vol && !is_trending {
            return (MarketRegime::HighVolatility, 0.6);
        }

        // 优先级 5：极端正收益但无趋势（可能反弹）
        if is_extreme_positive && !is_trending {
            return (MarketRegime::HighVolatility, 0.5);
        }

        // 默认：震荡
        (MarketRegime::Ranging, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_kline(open: f64, high: f64, low: f64, close: f64) -> (f64, f64, f64, f64, f64) {
        (open, high, low, close, 1000.0)
    }

    #[test]
    fn test_market_regime_as_str() {
        assert_eq!(MarketRegime::TrendingBull.as_str(), "trending_bull");
        assert_eq!(MarketRegime::Crisis.as_str(), "crisis");
    }

    #[test]
    fn test_market_regime_from_str() {
        assert_eq!(
            MarketRegime::from_str("trending_bear"),
            Some(MarketRegime::TrendingBear)
        );
        assert_eq!(MarketRegime::from_str("invalid"), None);
    }

    #[test]
    fn test_market_regime_classification() {
        let classifier = RegimeClassifier::with_defaults();

        // 构造一个趋势性上涨的 K 线序列
        let mut klines: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
        let mut price = 100.0;
        for i in 0..60 {
            let open = price;
            price += 1.5; // 稳定上涨
            let close = price;
            let high = close + 0.5;
            let low = open - 0.5;
            klines.push(make_kline(open, high, low, close));
            let _ = i;
        }

        let snapshot = classifier.classify(&klines);
        assert!(snapshot.is_some());
        let snap = snapshot.unwrap();
        // 应识别为趋势性上涨或至少非危机
        assert!(
            snap.regime == MarketRegime::TrendingBull || snap.regime == MarketRegime::Ranging,
            "稳定上涨应识别为 TrendingBull 或 Ranging，实际: {:?}",
            snap.regime
        );
    }

    #[test]
    fn test_crisis_detection() {
        let classifier = RegimeClassifier::with_defaults();

        // 构造一个危机序列：先正常，后暴跌
        let mut klines: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
        let mut price = 100.0;
        for i in 0..40 {
            let open: f64 = price;
            let close: f64 = price + (if i < 30 { 0.1 } else { -3.0 }); // 后 10 根暴跌
            let high = open.max(close) + 1.0;
            let low = open.min(close) - 1.0;
            klines.push(make_kline(open, high, low, close));
            price = close;
        }

        let snapshot = classifier.classify(&klines);
        assert!(snapshot.is_some());
        let snap = snapshot.unwrap();
        // 暴跌应识别为 TrendingBear 或 Crisis
        assert!(
            snap.regime == MarketRegime::TrendingBear || snap.regime == MarketRegime::Crisis,
            "暴跌应识别为 TrendingBear 或 Crisis，实际: {:?}",
            snap.regime
        );
    }

    #[test]
    fn test_ranging_detection() {
        let classifier = RegimeClassifier::with_defaults();

        // 构造一个震荡序列：价格在 100-102 之间波动
        let mut klines: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
        for i in 0..60 {
            let base = 100.0 + (i % 4) as f64 * 0.5; // 周期性小幅波动
            let open = base;
            let close = base + 0.2;
            let high = base + 0.5;
            let low = base - 0.5;
            klines.push(make_kline(open, high, low, close));
        }

        let snapshot = classifier.classify(&klines);
        assert!(snapshot.is_some());
        let snap = snapshot.unwrap();
        // 低波动 + 无明显趋势应识别为 Ranging
        assert!(
            snap.regime == MarketRegime::Ranging || snap.regime == MarketRegime::HighVolatility,
            "小幅震荡应识别为 Ranging 或 HighVolatility，实际: {:?}",
            snap.regime
        );
    }

    #[test]
    fn test_insufficient_data_returns_none() {
        let classifier = RegimeClassifier::with_defaults();
        let klines = vec![make_kline(100.0, 101.0, 99.0, 100.5)];
        assert!(classifier.classify(&klines).is_none());
    }

    #[test]
    fn test_percentile_rank() {
        let classifier = RegimeClassifier::with_defaults();
        let history = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        // 3.0 在 5 个值中排第 2（有 2 个小于它）
        let rank = classifier.percentile_rank(&history, 3.0);
        assert!((rank - 0.4).abs() < 1e-9);
    }

    #[test]
    fn test_volatility_calculation() {
        let classifier = RegimeClassifier::with_defaults();
        let returns = vec![0.01, -0.02, 0.03, -0.01, 0.02];
        let vol = classifier.calculate_volatility(&returns);
        assert!(vol > 0.0);
    }

    #[test]
    fn test_regime_snapshot_default() {
        let snap = RegimeSnapshot::default();
        assert_eq!(snap.regime, MarketRegime::Ranging);
        assert!((snap.confidence - 0.5).abs() < 1e-9);
    }
}
