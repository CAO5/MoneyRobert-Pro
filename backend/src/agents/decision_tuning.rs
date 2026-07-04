use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DecisionTuningConfig {
    pub technical_weight: f64,
    pub capital_weight: f64,
    pub news_weight: f64,
    pub minimum_edge_floor: f64,
    pub minimum_edge_ceiling: f64,
    pub minimum_data_quality: f64,
    pub reliability_strength: f64,
    pub conflict_policy: String,
    /// 多周期背离加成（短期向上+日线bear 时给做多的 edge 加成）
    pub divergence_bonus: f64,
    /// 动量突破加成（4h强动量+MACD同向）
    pub momentum_bonus_4h: f64,
    /// 短期动量加成（1h强动量）
    pub momentum_bonus_1h: f64,
    /// 4h 动量触发阈值（涨幅绝对值百分比）
    pub momentum_threshold_4h: f64,
    /// 1h 动量触发阈值（涨幅绝对值百分比）
    pub momentum_threshold_1h: f64,
    /// 多头过热减分：RSI 阈值（单独触发）
    pub overheat_rsi_threshold: f64,
    /// 多头过热减分：资金费率阈值
    pub overheat_funding_threshold: f64,
    /// 多头过热减分幅度
    pub overheat_penalty: f64,
    /// 反驳后翻转方向的权重折扣（0.0-1.0，1.0=不折扣）
    pub sentiment_flip_penalty: f64,
    /// LLM 否决权强度（0.0=禁用，1.0=完全否决；LLM=hold 但评分器=long/short 时按此降 confidence）
    pub llm_veto_strength: f64,
}

impl Default for DecisionTuningConfig {
    fn default() -> Self {
        Self {
            technical_weight: 0.35,
            capital_weight: 0.35,
            news_weight: 0.30,
            minimum_edge_floor: 0.05,
            minimum_edge_ceiling: 0.08,
            minimum_data_quality: 0.35,
            reliability_strength: 1.0,
            conflict_policy: "score_wins".to_string(),
            // 市场上下文调整参数（微调后的默认值）
            divergence_bonus: 0.015,
            momentum_bonus_4h: 0.020,
            momentum_bonus_1h: 0.010,
            momentum_threshold_4h: 2.0,
            momentum_threshold_1h: 0.5,
            overheat_rsi_threshold: 75.0,
            overheat_funding_threshold: 0.0001,
            overheat_penalty: 0.015,
            sentiment_flip_penalty: 0.7,
            llm_veto_strength: 0.5,
        }
    }
}

impl DecisionTuningConfig {
    pub fn validate(&self) -> Result<(), String> {
        let weights = [self.technical_weight, self.capital_weight, self.news_weight];
        if weights
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 1.0)
        {
            return Err("部门权重必须在 0 到 1 之间".to_string());
        }
        let total: f64 = weights.iter().sum();
        if (total - 1.0).abs() > 0.001 {
            return Err(format!("部门权重之和必须为 1，当前为 {:.4}", total));
        }
        if !(0.0..=0.50).contains(&self.minimum_edge_floor)
            || !(0.0..=0.50).contains(&self.minimum_edge_ceiling)
            || self.minimum_edge_floor > self.minimum_edge_ceiling
        {
            return Err("方向优势范围必须在 0 到 0.5 之间，且最低值不能高于最高值".to_string());
        }
        if !(0.0..=1.0).contains(&self.minimum_data_quality) {
            return Err("最低数据质量必须在 0 到 1 之间".to_string());
        }
        if !(0.0..=1.0).contains(&self.reliability_strength) {
            return Err("历史可靠度影响强度必须在 0 到 1 之间".to_string());
        }
        if !matches!(
            self.conflict_policy.as_str(),
            "score_wins" | "hold_on_conflict"
        ) {
            return Err("冲突策略必须为 score_wins 或 hold_on_conflict".to_string());
        }
        // 新增参数校验
        if !(0.0..=0.20).contains(&self.divergence_bonus) {
            return Err("多周期背离加成必须在 0 到 0.20 之间".to_string());
        }
        if !(0.0..=0.20).contains(&self.momentum_bonus_4h)
            || !(0.0..=0.20).contains(&self.momentum_bonus_1h)
        {
            return Err("动量突破加成必须在 0 到 0.20 之间".to_string());
        }
        if self.momentum_threshold_4h < 0.0 || self.momentum_threshold_1h < 0.0 {
            return Err("动量触发阈值不能为负".to_string());
        }
        if !(0.0..=100.0).contains(&self.overheat_rsi_threshold) {
            return Err("RSI 过热阈值必须在 0 到 100 之间".to_string());
        }
        if self.overheat_funding_threshold < 0.0 {
            return Err("资金费率过热阈值不能为负".to_string());
        }
        if !(0.0..=0.20).contains(&self.overheat_penalty) {
            return Err("过热减分幅度必须在 0 到 0.20 之间".to_string());
        }
        if !(0.0..=1.0).contains(&self.sentiment_flip_penalty) {
            return Err("反驳翻转折扣必须在 0 到 1 之间".to_string());
        }
        if !(0.0..=1.0).contains(&self.llm_veto_strength) {
            return Err("LLM 否决权强度必须在 0 到 1 之间".to_string());
        }
        Ok(())
    }

    pub async fn load(pool: &PgPool) -> Self {
        let value = sqlx::query_scalar::<_, String>(
            "SELECT value FROM system_settings WHERE key = 'decision_tuning'",
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        value
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .filter(|config| config.validate().is_ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::DecisionTuningConfig;

    #[test]
    fn default_config_is_valid() {
        assert!(DecisionTuningConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_invalid_weight_sum_and_edge_range() {
        let mut config = DecisionTuningConfig::default();
        config.news_weight = 0.50;
        assert!(config.validate().is_err());

        let mut config = DecisionTuningConfig::default();
        config.minimum_edge_floor = 0.20;
        config.minimum_edge_ceiling = 0.10;
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_tuning_params() {
        let mut config = DecisionTuningConfig::default();
        config.divergence_bonus = 0.5;
        assert!(config.validate().is_err());

        let mut config = DecisionTuningConfig::default();
        config.llm_veto_strength = 1.5;
        assert!(config.validate().is_err());

        let mut config = DecisionTuningConfig::default();
        config.overheat_rsi_threshold = 150.0;
        assert!(config.validate().is_err());
    }
}