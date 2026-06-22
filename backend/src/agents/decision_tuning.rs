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
}

impl Default for DecisionTuningConfig {
    fn default() -> Self {
        Self {
            technical_weight: 0.35,
            capital_weight: 0.35,
            news_weight: 0.30,
            minimum_edge_floor: 0.06,
            minimum_edge_ceiling: 0.12,
            minimum_data_quality: 0.35,
            reliability_strength: 1.0,
            conflict_policy: "score_wins".to_string(),
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
}