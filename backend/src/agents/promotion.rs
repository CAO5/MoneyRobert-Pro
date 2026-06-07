use crate::agents::{
    errors::AgentResult,
    models::{AiSimulationConfig, DemotionTrigger, PromotionEligibility, RollingStats},
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LevelRequirements {
    pub level: i32,
    pub min_trades: i32,
    pub min_win_rate: f64,
    pub min_profit_loss_ratio: f64,
    pub min_running_days: i32,
    pub max_drawdown_percent: f64,
    pub max_daily_loss_percent: f64,
    pub max_consecutive_losses: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PromotionAudit {
    pub id: Uuid,
    pub config_id: Uuid,
    pub from_level: i32,
    pub to_level: i32,
    pub from_mode: String,
    pub to_mode: String,
    pub stats_snapshot: serde_json::Value,
    pub audit_report: Option<serde_json::Value>,
    pub status: String,
    pub review_step: i32,
    pub reviewed_by: Option<String>,
    pub review_comment: Option<String>,
    pub reviewed_at: Option<chrono::DateTime<Utc>>,
    pub observation_period_days: Option<i32>,
    pub observation_started_at: Option<chrono::DateTime<Utc>>,
    pub observation_completed_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RiskConfirmation {
    pub id: Uuid,
    pub user_id: i64,
    pub config_id: Option<Uuid>,
    pub version: String,
    pub accepted: bool,
    pub accept_reason: Option<String>,
    pub max_acceptable_loss: Option<f64>,
    pub signed_at: Option<chrono::DateTime<Utc>>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
}

pub struct PromotionSystem {
    pool: PgPool,
}

impl PromotionSystem {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn get_level_requirements(level: i32) -> LevelRequirements {
        match level {
            0 => LevelRequirements {
                level: 0,
                min_trades: 0,
                min_win_rate: 0.0,
                min_profit_loss_ratio: 0.0,
                min_running_days: 0,
                max_drawdown_percent: 100.0,
                max_daily_loss_percent: 100.0,
                max_consecutive_losses: i32::MAX,
            },
            1 => LevelRequirements {
                level: 1,
                min_trades: 50,
                min_win_rate: 0.55,
                min_profit_loss_ratio: 1.2,
                min_running_days: 14,
                max_drawdown_percent: 10.0,
                max_daily_loss_percent: 5.0,
                max_consecutive_losses: 5,
            },
            2 => LevelRequirements {
                level: 2,
                min_trades: 100,
                min_win_rate: 0.58,
                min_profit_loss_ratio: 1.5,
                min_running_days: 30,
                max_drawdown_percent: 8.0,
                max_daily_loss_percent: 4.0,
                max_consecutive_losses: 4,
            },
            3 => LevelRequirements {
                level: 3,
                min_trades: 200,
                min_win_rate: 0.60,
                min_profit_loss_ratio: 1.8,
                min_running_days: 60,
                max_drawdown_percent: 6.0,
                max_daily_loss_percent: 3.0,
                max_consecutive_losses: 3,
            },
            _ => LevelRequirements {
                level,
                min_trades: 200,
                min_win_rate: 0.60,
                min_profit_loss_ratio: 1.8,
                min_running_days: 60,
                max_drawdown_percent: 6.0,
                max_daily_loss_percent: 3.0,
                max_consecutive_losses: 3,
            },
        }
    }

    pub fn get_next_level(current_level: i32) -> Option<i32> {
        if current_level < 3 {
            Some(current_level + 1)
        } else {
            None
        }
    }

    pub async fn check_promotion_eligibility(
        &self,
        config: &AiSimulationConfig,
    ) -> AgentResult<PromotionEligibility> {
        let next_level = match Self::get_next_level(config.level) {
            Some(level) => level,
            None => {
                return Ok(PromotionEligibility {
                    eligible: false,
                    current_level: config.level,
                    next_level: None,
                    stats: Self::calculate_rolling_stats(config),
                    requirements_met: false,
                    missing_requirements: vec!["Already at maximum level".to_string()],
                });
            }
        };

        let requirements = Self::get_level_requirements(next_level);
        let stats = Self::calculate_rolling_stats(config);
        let mut missing_requirements = Vec::new();

        if config.total_trades < requirements.min_trades {
            missing_requirements.push(format!(
                "Minimum trades: {} (current: {})",
                requirements.min_trades, config.total_trades
            ));
        }

        if config.win_rate < requirements.min_win_rate {
            missing_requirements.push(format!(
                "Minimum win rate: {:.1}% (current: {:.1}%)",
                requirements.min_win_rate * 100.0,
                config.win_rate * 100.0
            ));
        }

        if config.profit_loss_ratio < requirements.min_profit_loss_ratio {
            missing_requirements.push(format!(
                "Minimum profit/loss ratio: {:.2} (current: {:.2})",
                requirements.min_profit_loss_ratio, config.profit_loss_ratio
            ));
        }

        if config.running_days < requirements.min_running_days {
            missing_requirements.push(format!(
                "Minimum running days: {} (current: {})",
                requirements.min_running_days, config.running_days
            ));
        }

        if config.max_drawdown_percent > requirements.max_drawdown_percent {
            missing_requirements.push(format!(
                "Maximum drawdown: {:.1}% (current: {:.1}%)",
                requirements.max_drawdown_percent, config.max_drawdown_percent
            ));
        }

        if config.consecutive_stop_losses > requirements.max_consecutive_losses {
            missing_requirements.push(format!(
                "Maximum consecutive losses: {} (current: {})",
                requirements.max_consecutive_losses, config.consecutive_stop_losses
            ));
        }

        let eligible = missing_requirements.is_empty();

        Ok(PromotionEligibility {
            eligible,
            current_level: config.level,
            next_level: Some(next_level),
            stats,
            requirements_met: eligible,
            missing_requirements,
        })
    }

    pub async fn initiate_promotion(
        &self,
        config_id: Uuid,
    ) -> AgentResult<PromotionAudit> {
        let config = sqlx::query_as::<_, AiSimulationConfig>(
            "SELECT * FROM ai_simulation_configs WHERE id = $1",
        )
        .bind(config_id)
        .fetch_one(&self.pool)
        .await?;

        let eligibility = self.check_promotion_eligibility(&config).await?;

        if !eligibility.eligible {
            return Err(crate::agents::errors::AgentError::PromotionError(
                "Not eligible for promotion".to_string(),
            ));
        }

        let next_level = eligibility.next_level.unwrap();
        let (from_mode, to_mode) = Self::get_mode_for_level(config.level, next_level);

        let stats_snapshot = serde_json::to_value(&eligibility.stats)?;

        let audit = sqlx::query_as::<_, PromotionAudit>(
            r#"
            INSERT INTO promotion_audits (
                config_id, from_level, to_level, from_mode, to_mode,
                stats_snapshot, status, review_step, observation_period_days,
                observation_started_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'pending', 1, 14, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(config_id)
        .bind(config.level)
        .bind(next_level)
        .bind(from_mode)
        .bind(to_mode)
        .bind(stats_snapshot)
        .fetch_one(&self.pool)
        .await?;

        info!(
            "Promotion initiated: config {} from level {} to {}",
            config_id, config.level, next_level
        );

        Ok(audit)
    }

    pub async fn check_observation_period(
        &self,
        audit_id: Uuid,
    ) -> AgentResult<bool> {
        let audit = sqlx::query_as::<_, PromotionAudit>(
            "SELECT * FROM promotion_audits WHERE id = $1",
        )
        .bind(audit_id)
        .fetch_one(&self.pool)
        .await?;

        if audit.status != "pending" || audit.review_step != 1 {
            return Ok(false);
        }

        let observation_started_at = match audit.observation_started_at {
            Some(dt) => dt,
            None => return Ok(false),
        };

        let observation_days = audit.observation_period_days.unwrap_or(14);
        let observation_end = observation_started_at + Duration::days(observation_days as i64);

        if Utc::now() >= observation_end {
            let mut tx = self.pool.begin().await?;

            sqlx::query(
                r#"
                UPDATE promotion_audits
                SET status = 'reviewing',
                    review_step = 2,
                    observation_completed_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(audit_id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            info!("Observation period completed for audit {}", audit_id);
            return Ok(true);
        }

        Ok(false)
    }

    pub async fn approve_promotion(
        &self,
        audit_id: Uuid,
        reviewed_by: Option<String>,
        review_comment: Option<String>,
    ) -> AgentResult<AiSimulationConfig> {
        let audit = sqlx::query_as::<_, PromotionAudit>(
            "SELECT * FROM promotion_audits WHERE id = $1",
        )
        .bind(audit_id)
        .fetch_one(&self.pool)
        .await?;

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE promotion_audits
            SET status = 'approved',
                review_step = 3,
                reviewed_by = $1,
                review_comment = $2,
                reviewed_at = NOW(),
                updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(reviewed_by)
        .bind(review_comment)
        .bind(audit_id)
        .execute(&mut *tx)
        .await?;

        let updated_config = sqlx::query_as::<_, AiSimulationConfig>(
            r#"
            UPDATE ai_simulation_configs
            SET level = $1,
                mode = $2,
                promotion_eligible = false,
                updated_at = NOW()
            WHERE id = $3
            RETURNING *
            "#,
        )
        .bind(audit.to_level)
        .bind(audit.to_mode)
        .bind(audit.config_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        info!(
            "Promotion approved: config {} now at level {}",
            audit.config_id, audit.to_level
        );

        Ok(updated_config)
    }

    pub async fn reject_promotion(
        &self,
        audit_id: Uuid,
        reviewed_by: Option<String>,
        review_comment: Option<String>,
    ) -> AgentResult<()> {
        sqlx::query(
            r#"
            UPDATE promotion_audits
            SET status = 'rejected',
                reviewed_by = $1,
                review_comment = $2,
                reviewed_at = NOW(),
                updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(reviewed_by)
        .bind(review_comment)
        .bind(audit_id)
        .execute(&self.pool)
        .await?;

        info!("Promotion rejected: audit {}", audit_id);

        Ok(())
    }

    pub async fn check_demotion(
        &self,
        config: &AiSimulationConfig,
    ) -> AgentResult<Option<DemotionTrigger>> {
        let requirements = Self::get_level_requirements(config.level);

        if config.level <= 0 {
            return Ok(None);
        }

        let mut reason = None;

        if config.max_drawdown_percent > requirements.max_drawdown_percent * 1.5 {
            reason = Some(format!(
                "Drawdown exceeds limit: {:.1}% (limit: {:.1}%)",
                config.max_drawdown_percent,
                requirements.max_drawdown_percent
            ));
        }

        if config.consecutive_stop_losses > requirements.max_consecutive_losses * 2 {
            reason = Some(format!(
                "Consecutive losses exceed limit: {} (limit: {})",
                config.consecutive_stop_losses, requirements.max_consecutive_losses
            ));
        }

        let current_balance_percent =
            (config.current_balance / config.initial_balance) * 100.0;
        if current_balance_percent < 50.0 {
            reason = Some(format!(
                "Balance dropped below 50%: {:.1}%",
                current_balance_percent
            ));
        }

        if let Some(reason) = reason {
            let to_level = config.level - 1;

            return Ok(Some(DemotionTrigger {
                from_level: config.level,
                to_level,
                reason,
            }));
        }

        Ok(None)
    }

    pub async fn execute_demotion(
        &self,
        config_id: Uuid,
        reason: String,
    ) -> AgentResult<AiSimulationConfig> {
        let config = sqlx::query_as::<_, AiSimulationConfig>(
            "SELECT * FROM ai_simulation_configs WHERE id = $1",
        )
        .bind(config_id)
        .fetch_one(&self.pool)
        .await?;

        if config.level <= 0 {
            return Err(crate::agents::errors::AgentError::PromotionError(
                "Cannot demote from level 0".to_string(),
            ));
        }

        let to_level = config.level - 1;
        let (from_mode, to_mode) = Self::get_mode_for_level(config.level, to_level);

        let stats_snapshot = serde_json::json!({
            "level": config.level,
            "balance": config.current_balance,
            "win_rate": config.win_rate,
            "total_trades": config.total_trades,
            "max_drawdown": config.max_drawdown_percent,
        });

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO demotion_records (
                config_id, from_level, to_level, from_mode, to_mode,
                trigger_reason, stats_snapshot, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            "#,
        )
        .bind(config_id)
        .bind(config.level)
        .bind(to_level)
        .bind(&from_mode)
        .bind(&to_mode)
        .bind(&reason)
        .bind(stats_snapshot)
        .execute(&mut *tx)
        .await?;

        let updated_config = sqlx::query_as::<_, AiSimulationConfig>(
            r#"
            UPDATE ai_simulation_configs
            SET level = $1,
                mode = $2,
                consecutive_stop_losses = 0,
                updated_at = NOW()
            WHERE id = $3
            RETURNING *
            "#,
        )
        .bind(to_level)
        .bind(to_mode)
        .bind(config_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        warn!(
            "Demotion executed: config {} from level {} to {} - {}",
            config_id, config.level, to_level, reason
        );

        Ok(updated_config)
    }

    pub async fn sign_risk_confirmation(
        &self,
        user_id: i64,
        config_id: Option<Uuid>,
        version: String,
        max_acceptable_loss: f64,
        accept_reason: Option<String>,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> AgentResult<RiskConfirmation> {
        let confirmation = sqlx::query_as::<_, RiskConfirmation>(
            r#"
            INSERT INTO risk_confirmations (
                user_id, config_id, version, accepted, accept_reason,
                max_acceptable_loss, signed_at, ip_address, user_agent, created_at
            )
            VALUES ($1, $2, $3, true, $4, $5, NOW(), $6, $7, NOW())
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(config_id)
        .bind(version)
        .bind(accept_reason)
        .bind(max_acceptable_loss)
        .bind(ip_address)
        .bind(user_agent)
        .fetch_one(&self.pool)
        .await?;

        if let Some(cfg_id) = config_id {
            sqlx::query(
                r#"
                UPDATE ai_simulation_configs
                SET risk_confirmation_signed = true,
                    risk_confirmation_signed_at = NOW(),
                    max_acceptable_loss_amount = $1,
                    updated_at = NOW()
                WHERE id = $2
                "#,
            )
            .bind(max_acceptable_loss)
            .bind(cfg_id)
            .execute(&self.pool)
            .await?;
        }

        info!("Risk confirmation signed for user {}", user_id);

        Ok(confirmation)
    }

    pub async fn get_latest_risk_confirmation(
        &self,
        user_id: i64,
    ) -> AgentResult<Option<RiskConfirmation>> {
        let confirmation = sqlx::query_as::<_, RiskConfirmation>(
            r#"
            SELECT * FROM risk_confirmations
            WHERE user_id = $1 AND accepted = true
            ORDER BY signed_at DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(confirmation)
    }

    pub async fn get_promotion_audits(
        &self,
        config_id: Uuid,
        limit: i64,
    ) -> AgentResult<Vec<PromotionAudit>> {
        let audits = sqlx::query_as::<_, PromotionAudit>(
            r#"
            SELECT * FROM promotion_audits
            WHERE config_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(config_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(audits)
    }

    fn calculate_rolling_stats(config: &AiSimulationConfig) -> RollingStats {
        RollingStats {
            total_trades: config.total_trades,
            winning_trades: config.winning_trades,
            losing_trades: config.losing_trades,
            win_rate: config.win_rate,
            avg_pnl_percent: config.avg_pnl_percent,
            profit_loss_ratio: config.profit_loss_ratio,
            max_drawdown_percent: config.max_drawdown_percent,
            running_days: config.running_days,
            daily_loss_percent: config.daily_loss_percent,
            consecutive_days_without_risk_trigger: 0,
            weekly_loss_percent: config.weekly_loss_percent,
        }
    }

    fn get_mode_for_level(from_level: i32, to_level: i32) -> (String, String) {
        let from_mode = match from_level {
            0 => "paper",
            1 => "demo",
            2 => "autonomous",
            _ => "live",
        };

        let to_mode = match to_level {
            0 => "paper",
            1 => "demo",
            2 => "autonomous",
            _ => "live",
        };

        (from_mode.to_string(), to_mode.to_string())
    }
}
