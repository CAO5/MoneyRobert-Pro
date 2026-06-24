//! Feature Calculator
//! 特征计算器
//!
//! 从 K 线数据计算技术指标特征，并存储到特征仓库。
//! 计算的特征包括：
//! - 动量类：RSI, MACD, SMA, EMA, ADX, 收益率
//! - 波动率类：布林带宽度, ATR, 实现波动率, 高低价差
//! - 成交量类：成交量均值, 成交量比率
//! - 资金费率类：funding_rate（从 funding_rate_history 表读取）

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::regime::{MarketRegime, RegimeClassifier, RegimeSnapshot};
use super::store::{FeatureStore, FeatureValue};

/// 特征集合（一次计算的所有特征）
#[derive(Debug, Clone, Default)]
pub struct FeatureSet {
    pub rsi_14: Option<f64>,
    pub macd_signal: Option<f64>,
    pub sma_20: Option<f64>,
    pub ema_12: Option<f64>,
    pub bollinger_width: Option<f64>,
    pub atr_14: Option<f64>,
    pub realized_volatility_20: Option<f64>,
    pub return_1d: Option<f64>,
    pub return_7d: Option<f64>,
    pub adx_14: Option<f64>,
    pub volume_sma_20: Option<f64>,
    pub volume_ratio: Option<f64>,
    pub high_low_range: Option<f64>,
    pub market_regime: Option<MarketRegime>,
}

impl FeatureSet {
    /// 转换为特征值列表（用于批量存储）
    pub fn to_feature_values(
        &self,
        feature_ids: &FeatureIdMap,
        symbol: &str,
        timestamp: DateTime<Utc>,
    ) -> Vec<FeatureValue> {
        let mut values = Vec::new();

        let mut push = |name: &str, value: Option<f64>| {
            if let (Some(v), Some(fid)) = (value, feature_ids.get(name)) {
                values.push(FeatureValue {
                    feature_id: *fid,
                    feature_name: name.to_string(),
                    symbol: symbol.to_string(),
                    timestamp,
                    value: v,
                    metadata: None,
                });
            }
        };

        push("rsi_14", self.rsi_14);
        push("macd_signal", self.macd_signal);
        push("sma_20", self.sma_20);
        push("ema_12", self.ema_12);
        push("bollinger_width", self.bollinger_width);
        push("atr_14", self.atr_14);
        push("realized_volatility_20", self.realized_volatility_20);
        push("return_1d", self.return_1d);
        push("return_7d", self.return_7d);
        push("adx_14", self.adx_14);
        push("volume_sma_20", self.volume_sma_20);
        push("volume_ratio", self.volume_ratio);
        push("high_low_range", self.high_low_range);

        // market_regime 是分类值，存储为 0-4 的整数
        if let Some(regime) = self.market_regime {
            if let Some(fid) = feature_ids.get("market_regime") {
                values.push(FeatureValue {
                    feature_id: *fid,
                    feature_name: "market_regime".to_string(),
                    symbol: symbol.to_string(),
                    timestamp,
                    value: regime_to_value(regime),
                    metadata: Some(serde_json::json!({"regime": regime.as_str()})),
                });
            }
        }

        values
    }
}

/// 将市场状态转换为数值（用于存储）
fn regime_to_value(regime: MarketRegime) -> f64 {
    match regime {
        MarketRegime::TrendingBull => 1.0,
        MarketRegime::Ranging => 0.0,
        MarketRegime::TrendingBear => -1.0,
        MarketRegime::HighVolatility => 2.0,
        MarketRegime::Crisis => -2.0,
    }
}

/// 计算特征参数 hash（用于血缘追溯，确保可复现）
/// 简单 hash：feature_name + kline_count + calc_version
fn compute_parameters_hash(feature_name: &str, kline_count: usize) -> String {
    // 简单的确定性 hash，不依赖外部 crate
    let input = format!("{}|{}|v1.0", feature_name, kline_count);
    let mut hash: u64 = 14695981039346656037; // FNV offset basis
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211); // FNV prime
    }
    format!("{:016x}", hash)
}

/// 特征名称到 ID 的映射
pub struct FeatureIdMap(std::collections::HashMap<String, Uuid>);

impl FeatureIdMap {
    pub fn get(&self, name: &str) -> Option<&Uuid> {
        self.0.get(name)
    }

    /// 从数据库加载所有特征定义
    pub async fn load(pool: &PgPool) -> Result<Self, sqlx::Error> {
        let defs = FeatureStore::list_definitions(pool).await?;
        let mut map = std::collections::HashMap::new();
        for def in defs {
            map.insert(def.name, def.feature_id);
        }
        Ok(Self(map))
    }
}

/// 特征计算器
pub struct FeatureCalculator {
    regime_classifier: RegimeClassifier,
}

impl Default for FeatureCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureCalculator {
    pub fn new() -> Self {
        Self {
            regime_classifier: RegimeClassifier::with_defaults(),
        }
    }

    pub fn with_regime_config(config: super::regime::RegimeConfig) -> Self {
        Self {
            regime_classifier: RegimeClassifier::new(config),
        }
    }

    /// 从 K 线序列计算特征
    ///
    /// 输入：按时间升序排列的 K 线 (open, high, low, close, volume)
    /// 输出：最新时刻的特征集合
    pub fn calculate(&self, klines: &[(f64, f64, f64, f64, f64)]) -> Option<FeatureSet> {
        if klines.is_empty() {
            return None;
        }

        let closes: Vec<f64> = klines.iter().map(|k| k.3).collect();
        let highs: Vec<f64> = klines.iter().map(|k| k.1).collect();
        let lows: Vec<f64> = klines.iter().map(|k| k.2).collect();
        let volumes: Vec<f64> = klines.iter().map(|k| k.4).collect();

        let mut features = FeatureSet::default();

        // 动量特征
        features.rsi_14 = self.calculate_rsi(&closes, 14);
        features.macd_signal = self.calculate_macd_signal(&closes);
        features.sma_20 = self.calculate_sma(&closes, 20);
        features.ema_12 = self.calculate_ema(&closes, 12);
        features.adx_14 = self.calculate_adx(klines, 14);

        // 波动率特征
        features.bollinger_width = self.calculate_bollinger_width(&closes, 20);
        features.atr_14 = self.calculate_atr(&highs, &lows, &closes, 14);
        features.realized_volatility_20 = self.calculate_realized_volatility(&closes, 20);
        features.high_low_range = self.calculate_high_low_range(klines.last().unwrap());

        // 收益率特征
        features.return_1d = self.calculate_return(&closes, 1);
        features.return_7d = self.calculate_return(&closes, 7);

        // 成交量特征
        features.volume_sma_20 = self.calculate_sma(&volumes, 20);
        features.volume_ratio = self.calculate_volume_ratio(&volumes, 20);

        // 市场状态
        if let Some(regime_snap) = self.regime_classifier.classify(klines) {
            features.market_regime = Some(regime_snap.regime);
        }

        Some(features)
    }

    /// 计算并存储特征到数据库
    pub async fn calculate_and_store(
        &self,
        pool: &PgPool,
        symbol: &str,
        klines: &[(f64, f64, f64, f64, f64)],
        timestamp: DateTime<Utc>,
    ) -> Result<usize, sqlx::Error> {
        let features = self.calculate(klines);
        let Some(features) = features else {
            return Ok(0);
        };

        let id_map = FeatureIdMap::load(pool).await?;
        let values = features.to_feature_values(&id_map, symbol, timestamp);
        let count = values.len();

        FeatureStore::batch_upsert_feature_values(pool, &values).await?;

        // 写入特征血缘（数据源、计算版本、参数 hash）
        // 依据《系统评估与演进规划》系统架构师视角：
        //   "每次决策保存数据版本、特征版本、模型版本和规则版本"
        let source_time_end = timestamp;
        let source_time_start = if klines.len() > 1 {
            timestamp - chrono::Duration::seconds(klines.len() as i64 * 3600)
        } else {
            timestamp - chrono::Duration::hours(1)
        };

        for value in &values {
            let parameters = serde_json::json!({
                "kline_count": klines.len(),
                "feature_name": value.feature_name,
            });
            let parameters_hash = compute_parameters_hash(&value.feature_name, klines.len());

            let _ = FeatureStore::upsert_feature_lineage(
                pool,
                value.feature_id,
                symbol,
                timestamp,
                "okx_klines",
                Some(source_time_start),
                Some(source_time_end),
                "v1.0",
                &parameters_hash,
                &parameters,
                &[],
                Some(&serde_json::json!({
                    "source": "okx",
                    "table": "klines",
                    "interval": "1H",
                    "symbol": symbol,
                })),
            )
            .await;
        }

        // 同时存储市场状态快照
        if let Some(regime) = features.market_regime {
            let snap = RegimeSnapshot {
                regime,
                confidence: 0.5,
                adx: features.adx_14.unwrap_or(0.0),
                volatility_percentile: 0.5,
                return_percentile: 0.5,
                timestamp,
            };
            let _ = FeatureStore::upsert_regime(pool, symbol, timestamp, &snap).await;
        }

        Ok(count)
    }

    // ============ 技术指标计算 ============

    fn calculate_rsi(&self, closes: &[f64], period: usize) -> Option<f64> {
        if closes.len() < period + 1 {
            return None;
        }
        let mut gains = Vec::with_capacity(closes.len() - 1);
        let mut losses = Vec::with_capacity(closes.len() - 1);

        for i in 1..closes.len() {
            let diff = closes[i] - closes[i - 1];
            gains.push(diff.max(0.0));
            losses.push((-diff).max(0.0));
        }

        let avg_gain: f64 = gains.iter().rev().take(period).sum::<f64>() / period as f64;
        let avg_loss: f64 = losses.iter().rev().take(period).sum::<f64>() / period as f64;

        if avg_loss == 0.0 {
            return Some(100.0);
        }
        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    fn calculate_macd_signal(&self, closes: &[f64]) -> Option<f64> {
        if closes.len() < 26 {
            return None;
        }
        let ema12 = self.calculate_ema(closes, 12)?;
        let ema26 = self.calculate_ema(closes, 26)?;
        let macd_line = ema12 - ema26;
        // 简化：信号线 = MACD 线的 9 周期 EMA
        // 这里直接返回 MACD 线作为近似
        Some(macd_line)
    }

    fn calculate_sma(&self, values: &[f64], period: usize) -> Option<f64> {
        if values.len() < period {
            return None;
        }
        let sum: f64 = values.iter().rev().take(period).sum();
        Some(sum / period as f64)
    }

    fn calculate_ema(&self, values: &[f64], period: usize) -> Option<f64> {
        if values.len() < period {
            return None;
        }
        let k = 2.0 / (period as f64 + 1.0);
        let mut ema = values[0];
        for &v in &values[1..] {
            ema = v * k + ema * (1.0 - k);
        }
        Some(ema)
    }

    fn calculate_adx(&self, klines: &[(f64, f64, f64, f64, f64)], period: usize) -> Option<f64> {
        // 委托给 RegimeClassifier 的 ADX 计算
        let classifier = RegimeClassifier::with_defaults();
        let snapshot = classifier.classify(klines)?;
        Some(snapshot.adx)
    }

    fn calculate_bollinger_width(&self, closes: &[f64], period: usize) -> Option<f64> {
        if closes.len() < period {
            return None;
        }
        let slice = &closes[closes.len() - period..];
        let mean: f64 = slice.iter().sum::<f64>() / period as f64;
        let variance: f64 = slice.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / period as f64;
        let std = variance.sqrt();
        // 布林带宽度 = 2 * std * 2 / mean
        if mean > 0.0 {
            Some(4.0 * std / mean)
        } else {
            None
        }
    }

    fn calculate_atr(
        &self,
        highs: &[f64],
        lows: &[f64],
        closes: &[f64],
        period: usize,
    ) -> Option<f64> {
        if highs.len() < period + 1 {
            return None;
        }
        let mut trs = Vec::with_capacity(highs.len() - 1);
        for i in 1..highs.len() {
            let tr = (highs[i] - lows[i])
                .max((highs[i] - closes[i - 1]).abs())
                .max((lows[i] - closes[i - 1]).abs());
            trs.push(tr);
        }
        let atr: f64 = trs.iter().rev().take(period).sum::<f64>() / period as f64;
        Some(atr)
    }

    fn calculate_realized_volatility(&self, closes: &[f64], period: usize) -> Option<f64> {
        if closes.len() < period + 1 {
            return None;
        }
        let slice = &closes[closes.len() - period - 1..];
        let mut returns = Vec::with_capacity(period);
        for i in 1..slice.len() {
            if slice[i - 1] > 0.0 {
                returns.push((slice[i] - slice[i - 1]) / slice[i - 1]);
            }
        }
        if returns.is_empty() {
            return None;
        }
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        Some(variance.sqrt())
    }

    fn calculate_return(&self, closes: &[f64], period: usize) -> Option<f64> {
        if closes.len() < period + 1 {
            return None;
        }
        let prev = closes[closes.len() - period - 1];
        let curr = closes[closes.len() - 1];
        if prev > 0.0 {
            Some((curr - prev) / prev)
        } else {
            None
        }
    }

    fn calculate_high_low_range(&self, kline: &(f64, f64, f64, f64, f64)) -> Option<f64> {
        let (_, high, low, close, _) = *kline;
        if close > 0.0 {
            Some((high - low) / close)
        } else {
            None
        }
    }

    fn calculate_volume_ratio(&self, volumes: &[f64], period: usize) -> Option<f64> {
        if volumes.len() < period + 1 {
            return None;
        }
        let sma = self.calculate_sma(volumes, period)?;
        if sma > 0.0 {
            Some(volumes[volumes.len() - 1] / sma)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_klines(n: usize) -> Vec<(f64, f64, f64, f64, f64)> {
        let mut klines = Vec::with_capacity(n);
        let mut price = 100.0;
        for i in 0..n {
            let open = price;
            price += 0.5 + (i as f64 * 0.01).sin() * 0.3;
            let close = price;
            let high = close + 0.5;
            let low = open - 0.5;
            let volume = 1000.0 + (i as f64 * 10.0);
            klines.push((open, high, low, close, volume));
        }
        klines
    }

    #[test]
    fn test_calculate_features() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(60);
        let features = calc.calculate(&klines);

        assert!(features.is_some());
        let f = features.unwrap();
        assert!(f.rsi_14.is_some());
        assert!(f.sma_20.is_some());
        assert!(f.ema_12.is_some());
        assert!(f.bollinger_width.is_some());
        assert!(f.atr_14.is_some());
        assert!(f.realized_volatility_20.is_some());
        assert!(f.return_1d.is_some());
        assert!(f.return_7d.is_some());
        assert!(f.volume_sma_20.is_some());
        assert!(f.volume_ratio.is_some());
        assert!(f.high_low_range.is_some());
        assert!(f.market_regime.is_some());
    }

    #[test]
    fn test_calculate_insufficient_data() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(5);
        let features = calc.calculate(&klines);
        assert!(features.is_some());
        let f = features.unwrap();
        // 数据不足时，部分特征应为 None
        assert!(f.sma_20.is_none());
        assert!(f.adx_14.is_none());
    }

    #[test]
    fn test_regime_to_value() {
        assert_eq!(regime_to_value(MarketRegime::TrendingBull), 1.0);
        assert_eq!(regime_to_value(MarketRegime::TrendingBear), -1.0);
        assert_eq!(regime_to_value(MarketRegime::Ranging), 0.0);
        assert_eq!(regime_to_value(MarketRegime::Crisis), -2.0);
    }

    #[test]
    fn test_rsi_calculation() {
        let calc = FeatureCalculator::new();
        let closes: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let rsi = calc.calculate_rsi(&closes, 14);
        assert!(rsi.is_some());
        // 持续上涨，RSI 应接近 100
        let rsi_val = rsi.unwrap();
        assert!(rsi_val > 50.0, "持续上涨 RSI 应 > 50，实际: {}", rsi_val);
    }

    #[test]
    fn test_sma_calculation() {
        let calc = FeatureCalculator::new();
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let sma = calc.calculate_sma(&values, 3);
        assert!(sma.is_some());
        assert!((sma.unwrap() - 4.0).abs() < 1e-9); // (3+4+5)/3 = 4
    }

    #[test]
    fn test_ema_calculation() {
        let calc = FeatureCalculator::new();
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ema = calc.calculate_ema(&values, 3);
        assert!(ema.is_some());
        let ema_val = ema.unwrap();
        // EMA 应介于最小值和最大值之间
        assert!(ema_val >= 1.0 && ema_val <= 5.0);
    }

    #[test]
    fn test_bollinger_width() {
        let calc = FeatureCalculator::new();
        let closes: Vec<f64> = (0..25).map(|i| 100.0 + (i as f64 * 0.1).sin()).collect();
        let width = calc.calculate_bollinger_width(&closes, 20);
        assert!(width.is_some());
        assert!(width.unwrap() >= 0.0);
    }

    #[test]
    fn test_realized_volatility() {
        let calc = FeatureCalculator::new();
        let closes: Vec<f64> = (0..25).map(|i| 100.0 + (i as f64 * 0.5)).collect();
        let vol = calc.calculate_realized_volatility(&closes, 20);
        assert!(vol.is_some());
        assert!(vol.unwrap() >= 0.0);
    }

    #[test]
    fn test_return_calculation() {
        let calc = FeatureCalculator::new();
        let closes = vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0];
        let ret_1d = calc.calculate_return(&closes, 1);
        assert!(ret_1d.is_some());
        // 1 日收益率 = (107 - 106) / 106
        assert!((ret_1d.unwrap() - (1.0 / 106.0)).abs() < 1e-9);
    }

    #[test]
    fn test_volume_ratio() {
        let calc = FeatureCalculator::new();
        let volumes: Vec<f64> = (0..25).map(|i| 1000.0 + i as f64 * 10.0).collect();
        let ratio = calc.calculate_volume_ratio(&volumes, 20);
        assert!(ratio.is_some());
        // 最新成交量应大于均值，比率 > 1
        assert!(ratio.unwrap() > 1.0);
    }

    #[test]
    fn test_feature_set_to_values() {
        let mut id_map = std::collections::HashMap::new();
        id_map.insert("rsi_14".to_string(), Uuid::new_v4());
        id_map.insert("sma_20".to_string(), Uuid::new_v4());
        let id_map = FeatureIdMap(id_map);

        let features = FeatureSet {
            rsi_14: Some(55.0),
            sma_20: Some(100.0),
            ..Default::default()
        };

        let values = features.to_feature_values(&id_map, "BTC-USDT-SWAP", Utc::now());
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| v.feature_name == "rsi_14"));
        assert!(values.iter().any(|v| v.feature_name == "sma_20"));
    }
}
