use crate::agents::config::AgentConfig;
use crate::agents::errors::AgentResult;
use crate::agents::models::AiSimulationConfig;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckResult {
    pub passed: bool,
    pub alerts: Vec<String>,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub struct RiskChecker {
    db: PgPool,
    config: AgentConfig,
}

impl RiskChecker {
    pub fn new(db: PgPool, config: AgentConfig) -> Self {
        Self { db, config }
    }

    pub async fn check_all_risk_factors(
        &self,
        user_id: i64,
        config_id: Option<uuid::Uuid>,
        position_size_percent: f64,
        leverage: i32,
    ) -> AgentResult<RiskCheckResult> {
        let mut alerts = Vec::new();
        let mut risk_level = RiskLevel::Low;

        let position_check = self.check_position_size(position_size_percent);
        if !position_check.passed {
            alerts.extend(position_check.alerts);
            risk_level = std::cmp::max(risk_level, position_check.risk_level);
        }

        let leverage_check = self.check_leverage(leverage);
        if !leverage_check.passed {
            alerts.extend(leverage_check.alerts);
            risk_level = std::cmp::max(risk_level, leverage_check.risk_level);
        }

        if let Some(cfg_id) = config_id {
            let daily_loss_check = self.check_daily_loss_limit(user_id, cfg_id).await?;
            if !daily_loss_check.passed {
                alerts.extend(daily_loss_check.alerts);
                risk_level = std::cmp::max(risk_level, daily_loss_check.risk_level);
            }

            let weekly_loss_check = self.check_weekly_loss_limit(user_id, cfg_id).await?;
            if !weekly_loss_check.passed {
                alerts.extend(weekly_loss_check.alerts);
                risk_level = std::cmp::max(risk_level, weekly_loss_check.risk_level);
            }

            let daily_trades_check = self.check_daily_trade_limit(user_id, cfg_id).await?;
            if !daily_trades_check.passed {
                alerts.extend(daily_trades_check.alerts);
                risk_level = std::cmp::max(risk_level, daily_trades_check.risk_level);
            }

            let consecutive_losses_check = self.check_consecutive_losses(user_id, cfg_id).await?;
            if !consecutive_losses_check.passed {
                alerts.extend(consecutive_losses_check.alerts);
                risk_level = std::cmp::max(risk_level, consecutive_losses_check.risk_level);
            }

            let emergency_stop_check = self.check_emergency_stop(cfg_id).await?;
            if !emergency_stop_check.passed {
                alerts.extend(emergency_stop_check.alerts);
                risk_level = std::cmp::max(risk_level, emergency_stop_check.risk_level);
            }
        }

        Ok(RiskCheckResult {
            passed: alerts.is_empty(),
            alerts,
            risk_level,
        })
    }

    fn check_position_size(&self, position_size_percent: f64) -> RiskCheckResult {
        let max_size = self.config.autonomous.max_position_size_percent;
        if position_size_percent > max_size {
            return RiskCheckResult {
                passed: false,
                alerts: vec![format!(
                    "Position size {}% exceeds maximum allowed {}%",
                    position_size_percent, max_size
                )],
                risk_level: RiskLevel::High,
            };
        }

        if position_size_percent > max_size * 0.8 {
            return RiskCheckResult {
                passed: true,
                alerts: vec![format!(
                    "Position size {}% is approaching maximum allowed {}%",
                    position_size_percent, max_size
                )],
                risk_level: RiskLevel::Medium,
            };
        }

        RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        }
    }

    fn check_leverage(&self, leverage: i32) -> RiskCheckResult {
        let max_leverage = self.config.autonomous.max_leverage;
        if leverage > max_leverage {
            return RiskCheckResult {
                passed: false,
                alerts: vec![format!(
                    "Leverage {}x exceeds maximum allowed {}x",
                    leverage, max_leverage
                )],
                risk_level: RiskLevel::High,
            };
        }

        if (leverage as f64) > (max_leverage as f64) * 0.8 {
            return RiskCheckResult {
                passed: true,
                alerts: vec![format!(
                    "Leverage {}x is approaching maximum allowed {}x",
                    leverage, max_leverage
                )],
                risk_level: RiskLevel::Medium,
            };
        }

        RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        }
    }

    async fn check_daily_loss_limit(
        &self,
        user_id: i64,
        config_id: uuid::Uuid,
    ) -> AgentResult<RiskCheckResult> {
        let sim_config = self.get_simulation_config(user_id, config_id).await?;
        let max_daily_loss = sim_config.max_daily_loss_percent;

        if sim_config.daily_loss_percent >= max_daily_loss {
            return Ok(RiskCheckResult {
                passed: false,
                alerts: vec![format!(
                    "Daily loss {}% exceeds maximum allowed {}% - trading halted",
                    sim_config.daily_loss_percent, max_daily_loss
                )],
                risk_level: RiskLevel::Critical,
            });
        }

        if sim_config.daily_loss_percent >= max_daily_loss * 0.8 {
            return Ok(RiskCheckResult {
                passed: true,
                alerts: vec![format!(
                    "Daily loss {}% is approaching maximum allowed {}%",
                    sim_config.daily_loss_percent, max_daily_loss
                )],
                risk_level: RiskLevel::High,
            });
        }

        Ok(RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        })
    }

    async fn check_weekly_loss_limit(
        &self,
        user_id: i64,
        config_id: uuid::Uuid,
    ) -> AgentResult<RiskCheckResult> {
        let sim_config = self.get_simulation_config(user_id, config_id).await?;
        let max_weekly_loss = sim_config.max_weekly_loss_percent;

        if sim_config.weekly_loss_percent >= max_weekly_loss {
            return Ok(RiskCheckResult {
                passed: false,
                alerts: vec![format!(
                    "Weekly loss {}% exceeds maximum allowed {}% - trading halted",
                    sim_config.weekly_loss_percent, max_weekly_loss
                )],
                risk_level: RiskLevel::Critical,
            });
        }

        if sim_config.weekly_loss_percent >= max_weekly_loss * 0.8 {
            return Ok(RiskCheckResult {
                passed: true,
                alerts: vec![format!(
                    "Weekly loss {}% is approaching maximum allowed {}%",
                    sim_config.weekly_loss_percent, max_weekly_loss
                )],
                risk_level: RiskLevel::High,
            });
        }

        Ok(RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        })
    }

    async fn check_daily_trade_limit(
        &self,
        user_id: i64,
        config_id: uuid::Uuid,
    ) -> AgentResult<RiskCheckResult> {
        let sim_config = self.get_simulation_config(user_id, config_id).await?;
        let max_daily_trades = sim_config.max_daily_trades;

        if sim_config.total_trades >= max_daily_trades {
            return Ok(RiskCheckResult {
                passed: false,
                alerts: vec![format!(
                    "Daily trade count {} exceeds maximum allowed {}",
                    sim_config.total_trades, max_daily_trades
                )],
                risk_level: RiskLevel::Medium,
            });
        }

        if (sim_config.total_trades as f64) >= (max_daily_trades as f64) * 0.8 {
            return Ok(RiskCheckResult {
                passed: true,
                alerts: vec![format!(
                    "Daily trade count {} is approaching maximum allowed {}",
                    sim_config.total_trades, max_daily_trades
                )],
                risk_level: RiskLevel::Low,
            });
        }

        Ok(RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        })
    }

    async fn check_consecutive_losses(
        &self,
        user_id: i64,
        config_id: uuid::Uuid,
    ) -> AgentResult<RiskCheckResult> {
        let sim_config = self.get_simulation_config(user_id, config_id).await?;
        let max_consecutive_losses = 5;

        if sim_config.consecutive_stop_losses >= max_consecutive_losses {
            return Ok(RiskCheckResult {
                passed: false,
                alerts: vec![format!(
                    "{} consecutive losses detected - trading paused",
                    sim_config.consecutive_stop_losses
                )],
                risk_level: RiskLevel::Critical,
            });
        }

        if sim_config.consecutive_stop_losses >= 3 {
            return Ok(RiskCheckResult {
                passed: true,
                alerts: vec![format!(
                    "{} consecutive losses - monitor closely",
                    sim_config.consecutive_stop_losses
                )],
                risk_level: RiskLevel::Medium,
            });
        }

        Ok(RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        })
    }

    async fn check_emergency_stop(&self, config_id: uuid::Uuid) -> AgentResult<RiskCheckResult> {
        let row =
            sqlx::query(r#"SELECT autonomous_config FROM ai_simulation_configs WHERE id = $1"#)
                .bind(config_id)
                .fetch_optional(&self.db)
                .await?;

        if let Some(row) = row {
            let autonomous_config: Option<serde_json::Value> = row.get("autonomous_config");
            if let Some(config) = autonomous_config {
                if let Some(emergency_stop) = config.get("emergency_stop").and_then(|v| v.as_bool())
                {
                    if emergency_stop {
                        return Ok(RiskCheckResult {
                            passed: false,
                            alerts: vec![
                                "Emergency stop is active - all trading halted".to_string()
                            ],
                            risk_level: RiskLevel::Critical,
                        });
                    }
                }
            }
        }

        Ok(RiskCheckResult {
            passed: true,
            alerts: vec![],
            risk_level: RiskLevel::Low,
        })
    }

    async fn get_simulation_config(
        &self,
        _user_id: i64,
        config_id: uuid::Uuid,
    ) -> AgentResult<AiSimulationConfig> {
        let config = sqlx::query_as::<_, AiSimulationConfig>(
            r#"SELECT * FROM ai_simulation_configs WHERE id = $1"#,
        )
        .bind(config_id)
        .fetch_one(&self.db)
        .await?;

        Ok(config)
    }

    pub async fn trigger_emergency_stop(
        &self,
        config_id: uuid::Uuid,
        reason: &str,
    ) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE ai_simulation_configs
               SET autonomous_config = jsonb_set(
                   COALESCE(autonomous_config, '{}'::jsonb),
                   '{emergency_stop}',
                   'true'::jsonb
               ),
               updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(config_id)
        .execute(&self.db)
        .await?;

        tracing::warn!(
            "Emergency stop triggered for config {}: {}",
            config_id,
            reason
        );
        Ok(())
    }

    pub async fn reset_emergency_stop(&self, config_id: uuid::Uuid) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE ai_simulation_configs
               SET autonomous_config = jsonb_set(
                   COALESCE(autonomous_config, '{}'::jsonb),
                   '{emergency_stop}',
                   'false'::jsonb
               ),
               updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(config_id)
        .execute(&self.db)
        .await?;

        tracing::info!("Emergency stop reset for config {}", config_id);
        Ok(())
    }
}
