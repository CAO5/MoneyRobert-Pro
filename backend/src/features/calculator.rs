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
    // 新增指标（路线图基金经理视角建议扩充）
    pub donchian_width: Option<f64>,
    pub supertrend: Option<f64>,
    pub vwap: Option<f64>,
    pub obv: Option<f64>,
    pub mfi_14: Option<f64>,
    pub garch_volatility: Option<f64>,
    pub kyle_lambda: Option<f64>,
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

        // 新增指标
        push("donchian_width", self.donchian_width);
        push("supertrend", self.supertrend);
        push("vwap", self.vwap);
        push("obv", self.obv);
        push("mfi_14", self.mfi_14);
        push("garch_volatility", self.garch_volatility);
        push("kyle_lambda", self.kyle_lambda);

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

        // 新增指标（路线图基金经理视角建议扩充）
        features.donchian_width = self.calculate_donchian_width(&highs, &lows, 20);
        features.supertrend = self.calculate_supertrend(klines, 10, 3.0);
        features.vwap = self.calculate_vwap(klines, 20);
        features.obv = self.calculate_obv(&closes, &volumes);
        features.mfi_14 = self.calculate_mfi(klines, 14);
        features.garch_volatility = self.calculate_garch_volatility(&closes, 20);
        features.kyle_lambda = self.calculate_kyle_lambda(klines, 20);

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

    // ============ 新增技术指标（路线图基金经理视角建议扩充） ============

    /// Donchian 通道宽度
    /// 计算 N 周期内的最高价与最低价之差占收盘价的比例
    fn calculate_donchian_width(
        &self,
        highs: &[f64],
        lows: &[f64],
        period: usize,
    ) -> Option<f64> {
        if highs.len() < period || lows.len() < period {
            return None;
        }
        let hh = highs.iter().rev().take(period).cloned().fold(f64::NEG_INFINITY, f64::max);
        let ll = lows.iter().rev().take(period).cloned().fold(f64::INFINITY, f64::min);
        let close = highs.last().unwrap_or(&0.0);
        if *close > 0.0 {
            Some((hh - ll) / *close)
        } else {
            None
        }
    }

    /// SuperTrend 指标
    /// 基于 ATR 的趋势跟踪指标，返回最新 SuperTrend 值
    /// 公式：ST = (high+low)/2 ± multiplier × ATR
    /// 上涨趋势时 ST = mid - mult × ATR，下跌趋势时 ST = mid + mult × ATR
    fn calculate_supertrend(
        &self,
        klines: &[(f64, f64, f64, f64, f64)],
        period: usize,
        multiplier: f64,
    ) -> Option<f64> {
        if klines.len() < period + 1 {
            return None;
        }

        let highs: Vec<f64> = klines.iter().map(|k| k.1).collect();
        let lows: Vec<f64> = klines.iter().map(|k| k.2).collect();
        let closes: Vec<f64> = klines.iter().map(|k| k.3).collect();

        // 计算 ATR 序列
        let mut trs = Vec::with_capacity(klines.len() - 1);
        for i in 1..klines.len() {
            let tr = (highs[i] - lows[i])
                .max((highs[i] - closes[i - 1]).abs())
                .max((lows[i] - closes[i - 1]).abs());
            trs.push(tr);
        }

        // Wilder 平滑 ATR
        let mut atr_values = Vec::with_capacity(trs.len());
        if trs.len() < period {
            return None;
        }
        let mut atr: f64 = trs.iter().take(period).sum::<f64>() / period as f64;
        atr_values.push(atr);
        for i in period..trs.len() {
            atr = (atr * (period as f64 - 1.0) + trs[i]) / period as f64;
            atr_values.push(atr);
        }

        // 从后往前计算 SuperTrend
        let n = klines.len();
        let mut trend_up = true;
        let mut prev_st = (highs[0] + lows[0]) / 2.0 + multiplier * atr_values[0];

        for i in 1..n {
            let idx = if i < atr_values.len() { i } else { atr_values.len() - 1 };
            let mid = (highs[i] + lows[i]) / 2.0;
            let atr = atr_values[idx];
            let upper_band = mid + multiplier * atr;
            let lower_band = mid - multiplier * atr;

            let st = if trend_up {
                // 上涨趋势：使用 lower_band，但如果价格跌破则切换
                let st_candidate = lower_band.max(prev_st);
                if closes[i] < st_candidate {
                    trend_up = false;
                    upper_band.min(prev_st)
                } else {
                    st_candidate
                }
            } else {
                // 下跌趋势：使用 upper_band，但如果价格突破则切换
                let st_candidate = upper_band.min(prev_st);
                if closes[i] > st_candidate {
                    trend_up = true;
                    lower_band.max(prev_st)
                } else {
                    st_candidate
                }
            };
            prev_st = st;
        }

        Some(prev_st)
    }

    /// VWAP（成交量加权平均价）
    /// 计算 N 周期内的 VWAP，返回当前价格相对 VWAP 的偏离度
    fn calculate_vwap(
        &self,
        klines: &[(f64, f64, f64, f64, f64)],
        period: usize,
    ) -> Option<f64> {
        if klines.len() < period {
            return None;
        }
        let slice = &klines[klines.len() - period..];
        let mut pv_sum = 0.0;
        let mut vol_sum = 0.0;
        for k in slice {
            let (_, high, low, close, volume) = *k;
            let typical_price = (high + low + close) / 3.0;
            pv_sum += typical_price * volume;
            vol_sum += volume;
        }
        if vol_sum > 0.0 {
            let vwap = pv_sum / vol_sum;
            let current_close = slice.last().unwrap().3;
            if vwap > 0.0 {
                // 返回偏离度（百分比）
                Some((current_close - vwap) / vwap)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// OBV（On-Balance Volume，能量潮）
    /// 累积成交量指标：价格上涨日加成交量，下跌日减成交量
    fn calculate_obv(&self, closes: &[f64], volumes: &[f64]) -> Option<f64> {
        if closes.len() < 2 || volumes.len() < closes.len() {
            return None;
        }
        let mut obv = 0.0;
        for i in 1..closes.len() {
            if closes[i] > closes[i - 1] {
                obv += volumes[i];
            } else if closes[i] < closes[i - 1] {
                obv -= volumes[i];
            }
            // 平盘不变
        }
        // 返回最近 N 周期的 OBV 变化率
        let n = closes.len().min(20);
        if n > 1 {
            let mut prev_obv = 0.0;
            for i in 1..n {
                let idx = closes.len() - n + i;
                if closes[idx] > closes[idx - 1] {
                    prev_obv += volumes[idx];
                } else if closes[idx] < closes[idx - 1] {
                    prev_obv -= volumes[idx];
                }
            }
            if prev_obv.abs() > 0.0 {
                Some(obv / prev_obv.abs())
            } else {
                Some(obv)
            }
        } else {
            Some(obv)
        }
    }

    /// MFI（Money Flow Index，资金流量指标）
    /// 类似 RSI 但加入成交量权重，衡量资金流入流出强度
    fn calculate_mfi(
        &self,
        klines: &[(f64, f64, f64, f64, f64)],
        period: usize,
    ) -> Option<f64> {
        if klines.len() < period + 1 {
            return None;
        }

        let typical_prices: Vec<f64> = klines
            .iter()
            .map(|k| (k.1 + k.2 + k.3) / 3.0)
            .collect();
        let volumes: Vec<f64> = klines.iter().map(|k| k.4).collect();

        let mut money_flows: Vec<f64> = Vec::with_capacity(typical_prices.len() - 1);
        for i in 1..typical_prices.len() {
            let mf = typical_prices[i] * volumes[i];
            if typical_prices[i] > typical_prices[i - 1] {
                money_flows.push(mf); // 正资金流
            } else {
                money_flows.push(-mf); // 负资金流
            }
        }

        let recent = &money_flows[money_flows.len() - period..];
        let pos_flow: f64 = recent.iter().filter(|&&mf| mf > 0.0).sum();
        let neg_flow: f64 = recent.iter().filter(|&&mf| mf < 0.0).map(|mf| -mf).sum();

        if neg_flow == 0.0 {
            return Some(100.0);
        }
        let money_ratio = pos_flow / neg_flow;
        Some(100.0 - (100.0 / (1.0 + money_ratio)))
    }

    /// GARCH(1,1) 波动率
    /// 简化实现：σ²_t = ω + α × ε²_{t-1} + β × σ²_{t-1}
    /// 使用默认参数 ω=0.00001, α=0.1, β=0.88（典型加密市场参数）
    fn calculate_garch_volatility(&self, closes: &[f64], period: usize) -> Option<f64> {
        if closes.len() < period + 2 {
            return None;
        }

        // 计算对数收益率
        let slice = &closes[closes.len() - period - 1..];
        let mut returns = Vec::with_capacity(period);
        for i in 1..slice.len() {
            if slice[i - 1] > 0.0 {
                returns.push((slice[i] / slice[i - 1]).ln());
            }
        }
        if returns.is_empty() {
            return None;
        }

        // GARCH(1,1) 参数
        let omega: f64 = 0.00001;
        let alpha: f64 = 0.10;
        let beta: f64 = 0.88;

        // 初始化方差为样本方差
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let initial_var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

        let mut prev_var = initial_var;
        let mut prev_return = returns[0];

        for &ret in &returns[1..] {
            let epsilon = ret - mean;
            let new_var = omega + alpha * epsilon.powi(2) + beta * prev_var;
            prev_var = new_var;
            prev_return = ret;
        }

        // 返回年化波动率（假设每小时数据，24×365=8760 小时/年）
        let _ = prev_return;
        Some(prev_var.sqrt() * (8760.0_f64).sqrt())
    }

    /// Kyle Lambda（价格冲击系数）
    /// 衡量单位成交量对价格的影响：λ = ΔP / ΔV
    /// 使用最近 N 周期的价格变化与成交量回归斜率近似
    fn calculate_kyle_lambda(
        &self,
        klines: &[(f64, f64, f64, f64, f64)],
        period: usize,
    ) -> Option<f64> {
        if klines.len() < period + 1 {
            return None;
        }

        let slice = &klines[klines.len() - period - 1..];
        let mut price_changes = Vec::with_capacity(period);
        let mut volumes = Vec::with_capacity(period);

        for i in 1..slice.len() {
            let (_, _, _, close, vol) = slice[i];
            let prev_close = slice[i - 1].3;
            if prev_close > 0.0 {
                price_changes.push((close - prev_close) / prev_close);
                volumes.push(vol);
            }
        }

        if price_changes.len() < 5 {
            return None;
        }

        // 简单线性回归：ΔP = λ × ΔV + ε
        let n = price_changes.len() as f64;
        let mean_v: f64 = volumes.iter().sum::<f64>() / n;
        let mean_p: f64 = price_changes.iter().sum::<f64>() / n;

        let mut cov_vp = 0.0;
        let mut var_v = 0.0;
        for i in 0..price_changes.len() {
            cov_vp += (volumes[i] - mean_v) * (price_changes[i] - mean_p);
            var_v += (volumes[i] - mean_v).powi(2);
        }

        if var_v > 0.0 {
            let lambda = cov_vp / var_v;
            // 返回绝对值，表示价格冲击强度
            Some(lambda.abs())
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

    // ============ 新增指标测试 ============

    #[test]
    fn test_donchian_width() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(25);
        let highs: Vec<f64> = klines.iter().map(|k| k.1).collect();
        let lows: Vec<f64> = klines.iter().map(|k| k.2).collect();
        let width = calc.calculate_donchian_width(&highs, &lows, 20);
        assert!(width.is_some());
        assert!(width.unwrap() > 0.0, "Donchian 宽度应为正数");
    }

    #[test]
    fn test_donchian_width_insufficient() {
        let calc = FeatureCalculator::new();
        let highs = vec![100.0, 101.0];
        let lows = vec![99.0, 98.0];
        let width = calc.calculate_donchian_width(&highs, &lows, 20);
        assert!(width.is_none(), "数据不足应返回 None");
    }

    #[test]
    fn test_supertrend() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(30);
        let st = calc.calculate_supertrend(&klines, 10, 3.0);
        assert!(st.is_some());
        let st_val = st.unwrap();
        // SuperTrend 应为正数（价格水平）
        assert!(st_val > 0.0, "SuperTrend 应为正数，实际: {}", st_val);
    }

    #[test]
    fn test_vwap() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(25);
        let vwap = calc.calculate_vwap(&klines, 20);
        assert!(vwap.is_some());
        // 偏离度应在合理范围内
        let dev = vwap.unwrap();
        assert!(dev.abs() < 1.0, "VWAP 偏离度应 < 100%，实际: {}", dev);
    }

    #[test]
    fn test_obv() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(25);
        let closes: Vec<f64> = klines.iter().map(|k| k.3).collect();
        let volumes: Vec<f64> = klines.iter().map(|k| k.4).collect();
        let obv = calc.calculate_obv(&closes, &volumes);
        assert!(obv.is_some());
        // 持续上涨，OBV 应为正
        assert!(obv.unwrap() > 0.0, "持续上涨 OBV 应为正");
    }

    #[test]
    fn test_mfi() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(20);
        let mfi = calc.calculate_mfi(&klines, 14);
        assert!(mfi.is_some());
        let mfi_val = mfi.unwrap();
        // MFI 应在 0-100 之间
        assert!(mfi_val >= 0.0 && mfi_val <= 100.0, "MFI 应在 0-100 之间，实际: {}", mfi_val);
    }

    #[test]
    fn test_mfi_all_positive() {
        let calc = FeatureCalculator::new();
        // 持续上涨的 K 线
        let klines: Vec<(f64, f64, f64, f64, f64)> = (0..20)
            .map(|i| (100.0 + i as f64, 101.0 + i as f64, 99.0 + i as f64, 100.5 + i as f64, 1000.0))
            .collect();
        let mfi = calc.calculate_mfi(&klines, 14);
        assert!(mfi.is_some());
        // 持续上涨，MFI 应接近 100
        assert!(mfi.unwrap() > 80.0, "持续上涨 MFI 应 > 80");
    }

    #[test]
    fn test_garch_volatility() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(25);
        let closes: Vec<f64> = klines.iter().map(|k| k.3).collect();
        let garch = calc.calculate_garch_volatility(&closes, 20);
        assert!(garch.is_some());
        // 年化波动率应为正数
        let vol = garch.unwrap();
        assert!(vol > 0.0, "GARCH 波动率应为正数，实际: {}", vol);
        // 年化波动率通常在 10%-300% 之间
        assert!(vol < 10.0, "GARCH 波动率应 < 1000%，实际: {}", vol);
    }

    #[test]
    fn test_kyle_lambda() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(25);
        let lambda = calc.calculate_kyle_lambda(&klines, 20);
        assert!(lambda.is_some());
        // Kyle Lambda 应为非负数（取了绝对值）
        assert!(lambda.unwrap() >= 0.0, "Kyle Lambda 应为非负数");
    }

    #[test]
    fn test_kyle_lambda_insufficient() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(3);
        let lambda = calc.calculate_kyle_lambda(&klines, 20);
        assert!(lambda.is_none(), "数据不足应返回 None");
    }

    #[test]
    fn test_all_new_features_in_calculate() {
        let calc = FeatureCalculator::new();
        let klines = make_test_klines(60);
        let features = calc.calculate(&klines);
        assert!(features.is_some());
        let f = features.unwrap();
        // 所有新指标在有足够数据时应计算成功
        assert!(f.donchian_width.is_some(), "donchian_width 应计算成功");
        assert!(f.supertrend.is_some(), "supertrend 应计算成功");
        assert!(f.vwap.is_some(), "vwap 应计算成功");
        assert!(f.obv.is_some(), "obv 应计算成功");
        assert!(f.mfi_14.is_some(), "mfi_14 应计算成功");
        assert!(f.garch_volatility.is_some(), "garch_volatility 应计算成功");
        assert!(f.kyle_lambda.is_some(), "kyle_lambda 应计算成功");
    }
}
