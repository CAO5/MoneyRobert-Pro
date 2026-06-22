use crate::agents::errors::{AgentError, AgentResult};
use crate::agents::models::AutonomousConfig;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub market_data_update_interval_seconds: u64,
    pub analysis_interval_minutes: i32,
    pub max_concurrent_debates: usize,
    pub enable_technical_analysis: bool,
    pub enable_fundamental_analysis: bool,
    pub enable_sentiment_analysis: bool,
    pub rsi_period: usize,
    pub macd_fast_period: usize,
    pub macd_slow_period: usize,
    pub macd_signal_period: usize,
    pub bollinger_bands_period: usize,
    pub bollinger_bands_std_dev: f64,
    pub autonomous: AutonomousConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            market_data_update_interval_seconds: 10,
            analysis_interval_minutes: 5,
            max_concurrent_debates: 5,
            enable_technical_analysis: true,
            enable_fundamental_analysis: true,
            enable_sentiment_analysis: true,
            rsi_period: 14,
            macd_fast_period: 12,
            macd_slow_period: 26,
            macd_signal_period: 9,
            bollinger_bands_period: 20,
            bollinger_bands_std_dev: 2.0,
            autonomous: AutonomousConfig::default(),
        }
    }
}

impl AgentConfig {
    pub fn load() -> AgentResult<Self> {
        let mut config = Self::default();

        if let Ok(val) = env::var("AGENT_MARKET_UPDATE_INTERVAL") {
            config.market_data_update_interval_seconds = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MARKET_UPDATE_INTERVAL".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_ANALYSIS_INTERVAL_MINUTES") {
            config.analysis_interval_minutes = val.parse().map_err(|_| {
                AgentError::ConfigurationError(
                    "Invalid AGENT_ANALYSIS_INTERVAL_MINUTES".to_string(),
                )
            })?;
        }

        if let Ok(val) = env::var("AGENT_MAX_CONCURRENT_DEBATES") {
            config.max_concurrent_debates = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MAX_CONCURRENT_DEBATES".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_ENABLE_TECHNICAL_ANALYSIS") {
            config.enable_technical_analysis = val.to_lowercase() == "true";
        }

        if let Ok(val) = env::var("AGENT_ENABLE_FUNDAMENTAL_ANALYSIS") {
            config.enable_fundamental_analysis = val.to_lowercase() == "true";
        }

        if let Ok(val) = env::var("AGENT_ENABLE_SENTIMENT_ANALYSIS") {
            config.enable_sentiment_analysis = val.to_lowercase() == "true";
        }

        if let Ok(val) = env::var("AGENT_RSI_PERIOD") {
            config.rsi_period = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_RSI_PERIOD".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_MACD_FAST_PERIOD") {
            config.macd_fast_period = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MACD_FAST_PERIOD".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_MACD_SLOW_PERIOD") {
            config.macd_slow_period = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MACD_SLOW_PERIOD".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_MACD_SIGNAL_PERIOD") {
            config.macd_signal_period = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MACD_SIGNAL_PERIOD".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_BOLLINGER_BANDS_PERIOD") {
            config.bollinger_bands_period = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_BOLLINGER_BANDS_PERIOD".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_BOLLINGER_BANDS_STD_DEV") {
            config.bollinger_bands_std_dev = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_BOLLINGER_BANDS_STD_DEV".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_AUTONOMOUS_ENABLED") {
            config.autonomous.enabled = val.to_lowercase() == "true";
        }

        if let Ok(val) = env::var("AGENT_MAX_POSITION_SIZE_PERCENT") {
            config.autonomous.max_position_size_percent = val.parse().map_err(|_| {
                AgentError::ConfigurationError(
                    "Invalid AGENT_MAX_POSITION_SIZE_PERCENT".to_string(),
                )
            })?;
        }

        if let Ok(val) = env::var("AGENT_MAX_LEVERAGE") {
            config.autonomous.max_leverage = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MAX_LEVERAGE".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_MAX_DAILY_TRADES") {
            config.autonomous.max_daily_trades = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MAX_DAILY_TRADES".to_string())
            })?;
        }

        if let Ok(val) = env::var("AGENT_MAX_DAILY_LOSS_PERCENT") {
            config.autonomous.max_daily_loss_percent = val.parse().map_err(|_| {
                AgentError::ConfigurationError("Invalid AGENT_MAX_DAILY_LOSS_PERCENT".to_string())
            })?;
        }

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> AgentResult<()> {
        if self.market_data_update_interval_seconds < 1 {
            return Err(AgentError::ValidationError(
                "Market update interval must be at least 1 second".to_string(),
            ));
        }

        if self.analysis_interval_minutes < 1 {
            return Err(AgentError::ValidationError(
                "Analysis interval must be at least 1 minute".to_string(),
            ));
        }

        if self.max_concurrent_debates < 1 {
            return Err(AgentError::ValidationError(
                "Max concurrent debates must be at least 1".to_string(),
            ));
        }

        if self.rsi_period < 2 || self.rsi_period > 200 {
            return Err(AgentError::ValidationError(
                "RSI period must be between 2 and 200".to_string(),
            ));
        }

        if self.macd_fast_period < 2 || self.macd_fast_period >= self.macd_slow_period {
            return Err(AgentError::ValidationError(
                "MACD fast period must be at least 2 and less than slow period".to_string(),
            ));
        }

        if self.macd_signal_period < 1 {
            return Err(AgentError::ValidationError(
                "MACD signal period must be at least 1".to_string(),
            ));
        }

        if self.bollinger_bands_period < 2 {
            return Err(AgentError::ValidationError(
                "Bollinger Bands period must be at least 2".to_string(),
            ));
        }

        if self.bollinger_bands_std_dev <= 0.0 {
            return Err(AgentError::ValidationError(
                "Bollinger Bands standard deviation must be positive".to_string(),
            ));
        }

        if self.autonomous.max_position_size_percent <= 0.0
            || self.autonomous.max_position_size_percent > 100.0
        {
            return Err(AgentError::ValidationError(
                "Max position size percent must be between 0 and 100".to_string(),
            ));
        }

        if self.autonomous.max_leverage < 1 || self.autonomous.max_leverage > 125 {
            return Err(AgentError::ValidationError(
                "Max leverage must be between 1 and 125".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_rsi_period() {
        let mut config = AgentConfig::default();
        config.rsi_period = 1;
        assert!(config.validate().is_err());
    }
}
