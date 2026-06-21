// Self-evolving Fund Manager Agent System
// Implements: AGENT_SYSTEM_DESIGN.md Chapter 13
//
// Four-dimensional evolution: Prompt / Strategy / Memory / Architecture
// Reflexion loop: Actor → Evaluator → Self-Reflector → Evolution Engine
// Periodic reflection: daily morning / weekly review / monthly architecture / triggered

use crate::agents::errors::{AgentError, AgentResult};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

// ============================================================================
// Evolution Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvolutionType {
    PromptEvolution,
    StrategyEvolution,
    MemoryEvolution,
    ArchitectureEvolution,
}

impl std::fmt::Display for EvolutionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvolutionType::PromptEvolution => write!(f, "prompt"),
            EvolutionType::StrategyEvolution => write!(f, "strategy"),
            EvolutionType::MemoryEvolution => write!(f, "memory"),
            EvolutionType::ArchitectureEvolution => write!(f, "architecture"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReflectionType {
    DailyMorning,
    WeeklyReview,
    MonthlyArchitecture,
    Triggered,
}

impl std::fmt::Display for ReflectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReflectionType::DailyMorning => write!(f, "daily_morning"),
            ReflectionType::WeeklyReview => write!(f, "weekly_review"),
            ReflectionType::MonthlyArchitecture => write!(f, "monthly_architecture"),
            ReflectionType::Triggered => write!(f, "triggered"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VersionStatus {
    Draft,
    PendingReview,
    Approved,
    Active,
    Deprecated,
    RolledBack,
}

impl std::fmt::Display for VersionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionStatus::Draft => write!(f, "draft"),
            VersionStatus::PendingReview => write!(f, "pending_review"),
            VersionStatus::Approved => write!(f, "approved"),
            VersionStatus::Active => write!(f, "active"),
            VersionStatus::Deprecated => write!(f, "deprecated"),
            VersionStatus::RolledBack => write!(f, "rolled_back"),
        }
    }
}

// ============================================================================
// Prompt Version
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub id: Uuid,
    pub agent_id: String,
    pub version_number: i32,
    pub prompt_text: String,
    pub description: Option<String>,
    pub change_reason: Option<String>,
    pub performance_score: Option<f64>,
    pub status: String,
    pub parent_version_id: Option<Uuid>,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct PromptVersionStore {
    db_pool: PgPool,
}

impl PromptVersionStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn create(
        &self,
        agent_id: &str,
        prompt_text: &str,
        description: Option<&str>,
        change_reason: Option<&str>,
        parent_version_id: Option<Uuid>,
    ) -> AgentResult<PromptVersion> {
        // Get next version number
        let next_version: i32 = sqlx::query_scalar(
            r#"SELECT COALESCE(MAX(version_number), 0) + 1 FROM prompt_versions WHERE agent_id = $1"#,
        )
        .bind(agent_id)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| AgentError::DatabaseError(format!("Failed to get next version: {}", e)))?;

        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"INSERT INTO prompt_versions
               (id, agent_id, version_number, prompt_text, description, change_reason,
                parent_version_id, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'draft')
               RETURNING id, agent_id, version_number, prompt_text, description,
                         change_reason, performance_score, status, parent_version_id,
                         created_by, approved_by, approved_at, activated_at, created_at"#,
        )
        .bind(id)
        .bind(agent_id)
        .bind(next_version)
        .bind(prompt_text)
        .bind(description)
        .bind(change_reason)
        .bind(parent_version_id)
        .fetch_one(&self.db_pool)
        .await?;

        Ok(self.row_to_version(&row))
    }

    pub async fn get(&self, id: Uuid) -> AgentResult<Option<PromptVersion>> {
        let row = sqlx::query(
            r#"SELECT id, agent_id, version_number, prompt_text, description,
                      change_reason, performance_score, status, parent_version_id,
                      created_by, approved_by, approved_at, activated_at, created_at
               FROM prompt_versions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.db_pool)
        .await?;

        Ok(row.map(|r| self.row_to_version(&r)))
    }

    pub async fn list_by_agent(&self, agent_id: &str, limit: i64) -> AgentResult<Vec<PromptVersion>> {
        let rows = sqlx::query(
            r#"SELECT id, agent_id, version_number, prompt_text, description,
                      change_reason, performance_score, status, parent_version_id,
                      created_by, approved_by, approved_at, activated_at, created_at
               FROM prompt_versions
               WHERE agent_id = $1
               ORDER BY version_number DESC
               LIMIT $2"#,
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_version(r)).collect())
    }

    pub async fn get_active(&self, agent_id: &str) -> AgentResult<Option<PromptVersion>> {
        let row = sqlx::query(
            r#"SELECT id, agent_id, version_number, prompt_text, description,
                      change_reason, performance_score, status, parent_version_id,
                      created_by, approved_by, approved_at, activated_at, created_at
               FROM prompt_versions
               WHERE agent_id = $1 AND status = 'active'
               ORDER BY version_number DESC
               LIMIT 1"#,
        )
        .bind(agent_id)
        .fetch_optional(&self.db_pool)
        .await?;

        Ok(row.map(|r| self.row_to_version(&r)))
    }

    pub async fn approve(&self, id: Uuid, approved_by: &str) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE prompt_versions
               SET status = 'approved', approved_by = $2, approved_at = NOW()
               WHERE id = $1 AND status = 'pending_review'"#,
        )
        .bind(id)
        .bind(approved_by)
        .execute(&self.db_pool)
        .await?;
        Ok(())
    }

    pub async fn activate(&self, id: Uuid) -> AgentResult<()> {
        let mut tx = self.db_pool.begin().await?;

        // Get agent_id for this version
        let agent_id: String = sqlx::query("SELECT agent_id FROM prompt_versions WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?
            .get("agent_id");

        // Deactivate previous active version
        sqlx::query(
            r#"UPDATE prompt_versions
               SET status = 'deprecated'
               WHERE agent_id = $1 AND status = 'active'"#,
        )
        .bind(&agent_id)
        .execute(&mut *tx)
        .await?;

        // Activate new version
        sqlx::query(
            r#"UPDATE prompt_versions
               SET status = 'active', activated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn rollback(&self, id: Uuid, reason: &str) -> AgentResult<()> {
        let mut tx = self.db_pool.begin().await?;

        // Get version info
        let row = sqlx::query(
            r#"SELECT agent_id, parent_version_id FROM prompt_versions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let agent_id: String = row.get("agent_id");
        let parent_id: Option<Uuid> = row.get("parent_version_id");

        // Mark current as rolled_back
        sqlx::query(
            r#"UPDATE prompt_versions SET status = 'rolled_back' WHERE id = $1"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // Reactivate parent if exists
        if let Some(parent) = parent_id {
            sqlx::query(
                r#"UPDATE prompt_versions
                   SET status = 'active', activated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(parent)
            .execute(&mut *tx)
            .await?;
        }

        // Log the rollback
        sqlx::query(
            r#"INSERT INTO evolution_logs
               (evolution_type, target_agent, from_version, to_version,
                change_description, rationale, rollback_available, rolled_back,
                rolled_back_reason, status)
               VALUES ('prompt', $1, NULL, NULL, $2, $3, FALSE, TRUE, $4, 'rolled_back')"#,
        )
        .bind(&agent_id)
        .bind(format!("Rollback of prompt version {}", id))
        .bind(reason)
        .bind(reason)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    fn row_to_version(&self, row: &sqlx::postgres::PgRow) -> PromptVersion {
        PromptVersion {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            version_number: row.get("version_number"),
            prompt_text: row.get("prompt_text"),
            description: row.get("description"),
            change_reason: row.get("change_reason"),
            performance_score: row.get("performance_score"),
            status: row.get("status"),
            parent_version_id: row.get("parent_version_id"),
            created_by: row.get("created_by"),
            approved_by: row.get("approved_by"),
            approved_at: row.get("approved_at"),
            activated_at: row.get("activated_at"),
            created_at: row.get("created_at"),
        }
    }
}

// ============================================================================
// Strategy Version
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyVersion {
    pub id: Uuid,
    pub name: String,
    pub version_number: i32,
    pub strategy_type: String,
    pub parameters: serde_json::Value,
    pub risk_params: serde_json::Value,
    pub description: Option<String>,
    pub change_reason: Option<String>,
    pub backtest_score: Option<f64>,
    pub live_score: Option<f64>,
    pub status: String,
    pub parent_version_id: Option<Uuid>,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct StrategyVersionStore {
    db_pool: PgPool,
}

impl StrategyVersionStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn create(
        &self,
        name: &str,
        strategy_type: &str,
        parameters: serde_json::Value,
        risk_params: serde_json::Value,
        description: Option<&str>,
        change_reason: Option<&str>,
        parent_version_id: Option<Uuid>,
    ) -> AgentResult<StrategyVersion> {
        let next_version: i32 = sqlx::query_scalar(
            r#"SELECT COALESCE(MAX(version_number), 0) + 1 FROM strategy_versions WHERE name = $1"#,
        )
        .bind(name)
        .fetch_one(&self.db_pool)
        .await?;

        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"INSERT INTO strategy_versions
               (id, name, version_number, strategy_type, parameters, risk_params,
                description, change_reason, parent_version_id, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'draft')
               RETURNING id, name, version_number, strategy_type, parameters, risk_params,
                         description, change_reason, backtest_score, live_score, status,
                         parent_version_id, created_by, approved_by, approved_at, activated_at, created_at"#,
        )
        .bind(id)
        .bind(name)
        .bind(next_version)
        .bind(strategy_type)
        .bind(&parameters)
        .bind(&risk_params)
        .bind(description)
        .bind(change_reason)
        .bind(parent_version_id)
        .fetch_one(&self.db_pool)
        .await?;

        Ok(self.row_to_version(&row))
    }

    pub async fn get(&self, id: Uuid) -> AgentResult<Option<StrategyVersion>> {
        let row = sqlx::query(
            r#"SELECT id, name, version_number, strategy_type, parameters, risk_params,
                      description, change_reason, backtest_score, live_score, status,
                      parent_version_id, created_by, approved_by, approved_at, activated_at, created_at
               FROM strategy_versions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.db_pool)
        .await?;

        Ok(row.map(|r| self.row_to_version(&r)))
    }

    pub async fn list(&self, limit: i64, offset: i64) -> AgentResult<Vec<StrategyVersion>> {
        let rows = sqlx::query(
            r#"SELECT id, name, version_number, strategy_type, parameters, risk_params,
                      description, change_reason, backtest_score, live_score, status,
                      parent_version_id, created_by, approved_by, approved_at, activated_at, created_at
               FROM strategy_versions
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_version(r)).collect())
    }

    pub async fn get_active(&self, name: &str) -> AgentResult<Option<StrategyVersion>> {
        let row = sqlx::query(
            r#"SELECT id, name, version_number, strategy_type, parameters, risk_params,
                      description, change_reason, backtest_score, live_score, status,
                      parent_version_id, created_by, approved_by, approved_at, activated_at, created_at
               FROM strategy_versions
               WHERE name = $1 AND status = 'active'
               ORDER BY version_number DESC LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.db_pool)
        .await?;

        Ok(row.map(|r| self.row_to_version(&r)))
    }

    pub async fn update_backtest_score(&self, id: Uuid, score: f64) -> AgentResult<()> {
        sqlx::query("UPDATE strategy_versions SET backtest_score = $2 WHERE id = $1")
            .bind(id)
            .bind(score)
            .execute(&self.db_pool)
            .await?;
        Ok(())
    }

    pub async fn activate(&self, id: Uuid) -> AgentResult<()> {
        let mut tx = self.db_pool.begin().await?;
        let name: String = sqlx::query("SELECT name FROM strategy_versions WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?
            .get("name");

        sqlx::query(
            r#"UPDATE strategy_versions SET status = 'deprecated'
               WHERE name = $1 AND status = 'active'"#,
        )
        .bind(&name)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"UPDATE strategy_versions SET status = 'active', activated_at = NOW() WHERE id = $1"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    fn row_to_version(&self, row: &sqlx::postgres::PgRow) -> StrategyVersion {
        StrategyVersion {
            id: row.get("id"),
            name: row.get("name"),
            version_number: row.get("version_number"),
            strategy_type: row.get("strategy_type"),
            parameters: row.get("parameters"),
            risk_params: row.get("risk_params"),
            description: row.get("description"),
            change_reason: row.get("change_reason"),
            backtest_score: row.get("backtest_score"),
            live_score: row.get("live_score"),
            status: row.get("status"),
            parent_version_id: row.get("parent_version_id"),
            created_by: row.get("created_by"),
            approved_by: row.get("approved_by"),
            approved_at: row.get("approved_at"),
            activated_at: row.get("activated_at"),
            created_at: row.get("created_at"),
        }
    }
}

// ============================================================================
// Reflection Log (Self-evolution specific)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionReflectionLog {
    pub id: Uuid,
    pub reflection_type: String,
    pub trigger: Option<String>,
    pub scope: String,
    pub observations: serde_json::Value,
    pub lessons_learned: serde_json::Value,
    pub proposed_changes: serde_json::Value,
    pub applied_changes: serde_json::Value,
    pub effectiveness_score: Option<f64>,
    pub status: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct EvolutionReflectionStore {
    db_pool: PgPool,
}

impl EvolutionReflectionStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn record(
        &self,
        reflection_type: &str,
        trigger: Option<&str>,
        scope: &str,
        observations: serde_json::Value,
        lessons_learned: serde_json::Value,
        proposed_changes: serde_json::Value,
    ) -> AgentResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO reflection_logs
               (id, reflection_type, trigger, scope, observations, lessons_learned,
                proposed_changes, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')"#,
        )
        .bind(id)
        .bind(reflection_type)
        .bind(trigger)
        .bind(scope)
        .bind(observations)
        .bind(lessons_learned)
        .bind(proposed_changes)
        .execute(&self.db_pool)
        .await?;
        Ok(id)
    }

    pub async fn list(&self, limit: i64) -> AgentResult<Vec<EvolutionReflectionLog>> {
        let rows = sqlx::query(
            r#"SELECT id, reflection_type, trigger, scope, observations, lessons_learned,
                      proposed_changes, applied_changes, effectiveness_score, status,
                      reviewed_by, reviewed_at, created_at, updated_at
               FROM reflection_logs
               ORDER BY created_at DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| EvolutionReflectionLog {
                id: r.get("id"),
                reflection_type: r.get("reflection_type"),
                trigger: r.get("trigger"),
                scope: r.get("scope"),
                observations: r.get("observations"),
                lessons_learned: r.get("lessons_learned"),
                proposed_changes: r.get("proposed_changes"),
                applied_changes: r.get("applied_changes"),
                effectiveness_score: r.get("effectiveness_score"),
                status: r.get("status"),
                reviewed_by: r.get("reviewed_by"),
                reviewed_at: r.get("reviewed_at"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn update_effectiveness(&self, id: Uuid, score: f64) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE reflection_logs
               SET effectiveness_score = $2, status = 'reviewed', updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(score)
        .execute(&self.db_pool)
        .await?;
        Ok(())
    }
}

// ============================================================================
// Evolution Log
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionLog {
    pub id: Uuid,
    pub evolution_type: String,
    pub target_agent: Option<String>,
    pub from_version: Option<i32>,
    pub to_version: Option<i32>,
    pub change_description: String,
    pub change_data: serde_json::Value,
    pub rationale: Option<String>,
    pub expected_improvement: Option<String>,
    pub actual_improvement: Option<f64>,
    pub rollback_available: bool,
    pub rolled_back: bool,
    pub rolled_back_at: Option<DateTime<Utc>>,
    pub rolled_back_reason: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

pub struct EvolutionLogStore {
    db_pool: PgPool,
}

impl EvolutionLogStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn record(
        &self,
        evolution_type: &str,
        target_agent: Option<&str>,
        from_version: Option<i32>,
        to_version: Option<i32>,
        change_description: &str,
        change_data: serde_json::Value,
        rationale: Option<&str>,
        expected_improvement: Option<&str>,
    ) -> AgentResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO evolution_logs
               (id, evolution_type, target_agent, from_version, to_version,
                change_description, change_data, rationale, expected_improvement,
                rollback_available, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, TRUE, 'applied')"#,
        )
        .bind(id)
        .bind(evolution_type)
        .bind(target_agent)
        .bind(from_version)
        .bind(to_version)
        .bind(change_description)
        .bind(change_data)
        .bind(rationale)
        .bind(expected_improvement)
        .execute(&self.db_pool)
        .await?;
        Ok(id)
    }

    pub async fn list(&self, limit: i64) -> AgentResult<Vec<EvolutionLog>> {
        let rows = sqlx::query(
            r#"SELECT id, evolution_type, target_agent, from_version, to_version,
                      change_description, change_data, rationale, expected_improvement,
                      actual_improvement, rollback_available, rolled_back, rolled_back_at,
                      rolled_back_reason, status, created_at
               FROM evolution_logs
               ORDER BY created_at DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| EvolutionLog {
                id: r.get("id"),
                evolution_type: r.get("evolution_type"),
                target_agent: r.get("target_agent"),
                from_version: r.get("from_version"),
                to_version: r.get("to_version"),
                change_description: r.get("change_description"),
                change_data: r.get("change_data"),
                rationale: r.get("rationale"),
                expected_improvement: r.get("expected_improvement"),
                actual_improvement: r.get("actual_improvement"),
                rollback_available: r.get("rollback_available"),
                rolled_back: r.get("rolled_back"),
                rolled_back_at: r.get("rolled_back_at"),
                rolled_back_reason: r.get("rolled_back_reason"),
                status: r.get("status"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn update_actual_improvement(&self, id: Uuid, improvement: f64) -> AgentResult<()> {
        sqlx::query("UPDATE evolution_logs SET actual_improvement = $2 WHERE id = $1")
            .bind(id)
            .bind(improvement)
            .execute(&self.db_pool)
            .await?;
        Ok(())
    }
}

// ============================================================================
// Evolution Engine - Reflexion Loop
// ============================================================================

pub struct EvolutionEngine {
    db_pool: PgPool,
    prompt_store: Arc<PromptVersionStore>,
    strategy_store: Arc<StrategyVersionStore>,
    reflection_store: Arc<EvolutionReflectionStore>,
    evolution_log_store: Arc<EvolutionLogStore>,
    // Safety guardrails
    max_evolution_per_day: i32,
    require_human_review: bool,
    immutable_core_rules: Vec<String>,
}

impl EvolutionEngine {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            prompt_store: Arc::new(PromptVersionStore::new(db_pool.clone())),
            strategy_store: Arc::new(StrategyVersionStore::new(db_pool.clone())),
            reflection_store: Arc::new(EvolutionReflectionStore::new(db_pool.clone())),
            evolution_log_store: Arc::new(EvolutionLogStore::new(db_pool.clone())),
            db_pool,
            max_evolution_per_day: 3,
            require_human_review: true,
            immutable_core_rules: vec![
                "Never exceed max position size".to_string(),
                "Always enforce stop-loss".to_string(),
                "Never trade without risk confirmation".to_string(),
                "Always respect circuit breaker".to_string(),
            ],
        }
    }

    /// Run daily morning reflection: review yesterday's decisions and identify improvements.
    pub async fn run_daily_reflection(&self) -> AgentResult<EvolutionReflectionLog> {
        info!("Running daily morning reflection");

        // Gather yesterday's decision outcomes
        let decisions = sqlx::query(
            r#"SELECT agent_id, action, was_correct, confidence, pnl_percent,
                      market_trend, volatility
               FROM decision_memory
               WHERE created_at > NOW() - INTERVAL '1 day'
               ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.db_pool)
        .await?;

        let total = decisions.len();
        let correct = decisions
            .iter()
            .filter(|d| d.get::<Option<bool>, _>("was_correct") == Some(true))
            .count();

        let observations = serde_json::json!({
            "total_decisions": total,
            "correct_decisions": correct,
            "accuracy": if total > 0 { correct as f64 / total as f64 } else { 0.0 },
        });

        let lessons = self.extract_lessons(&decisions).await?;
        let proposed = self.propose_prompt_improvements(&lessons).await?;

        let id = self
            .reflection_store
            .record(
                "daily_morning",
                Some("scheduled"),
                "fund_manager",
                observations.clone(),
                serde_json::json!(lessons.clone()),
                serde_json::json!(proposed.clone()),
            )
            .await?;

        // Get the created log
        let logs = self.reflection_store.list(1).await?;
        Ok(logs.into_iter().next().unwrap_or(EvolutionReflectionLog {
            id,
            reflection_type: "daily_morning".to_string(),
            trigger: Some("scheduled".to_string()),
            scope: "fund_manager".to_string(),
            observations,
            lessons_learned: serde_json::json!(lessons),
            proposed_changes: serde_json::json!(proposed),
            applied_changes: serde_json::json!([]),
            effectiveness_score: None,
            status: "pending".to_string(),
            reviewed_by: None,
            reviewed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }

    /// Run weekly review: comprehensive performance analysis and strategy evolution.
    pub async fn run_weekly_review(&self) -> AgentResult<EvolutionReflectionLog> {
        info!("Running weekly review");

        let weekly_stats = sqlx::query(
            r#"SELECT
                    COUNT(*) as total_trades,
                    SUM(CASE WHEN pnl > 0 THEN 1 ELSE 0 END) as winning,
                    SUM(CASE WHEN pnl < 0 THEN 1 ELSE 0 END) as losing,
                    AVG(pnl_percent) as avg_pnl,
                    SUM(pnl) as total_pnl
               FROM ai_simulation_trades
               WHERE opened_at > NOW() - INTERVAL '7 days'
                 AND status = 'closed'"#,
        )
        .fetch_one(&self.db_pool)
        .await?;

        let total: i64 = weekly_stats.get("total_trades");
        let winning: i64 = weekly_stats.get("winning");
        let losing: i64 = weekly_stats.get("losing");
        let avg_pnl: Option<f64> = weekly_stats.get("avg_pnl");
        let total_pnl: Option<f64> = weekly_stats.get("total_pnl");

        let observations = serde_json::json!({
            "total_trades": total,
            "winning_trades": winning,
            "losing_trades": losing,
            "win_rate": if total > 0 { winning as f64 / total as f64 } else { 0.0 },
            "avg_pnl_percent": avg_pnl,
            "total_pnl": total_pnl,
        });

        let lessons = serde_json::json!({
            "performance_summary": if total_pnl.unwrap_or(0.0) > 0.0 { "profitable" } else { "unprofitable" },
            "key_observations": [],
        });

        let proposed = serde_json::json!({
            "strategy_adjustments": [],
            "risk_param_tuning": [],
        });

        let id = self
            .reflection_store
            .record(
                "weekly_review",
                Some("scheduled"),
                "fund_manager",
                observations.clone(),
                lessons.clone(),
                proposed.clone(),
            )
            .await?;

        let logs = self.reflection_store.list(1).await?;
        Ok(logs.into_iter().next().unwrap_or(EvolutionReflectionLog {
            id,
            reflection_type: "weekly_review".to_string(),
            trigger: Some("scheduled".to_string()),
            scope: "fund_manager".to_string(),
            observations: serde_json::json!({}),
            lessons_learned: lessons,
            proposed_changes: proposed,
            applied_changes: serde_json::json!([]),
            effectiveness_score: None,
            status: "pending".to_string(),
            reviewed_by: None,
            reviewed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }

    /// Run monthly architecture review: evaluate overall system design.
    pub async fn run_monthly_architecture_review(&self) -> AgentResult<EvolutionReflectionLog> {
        info!("Running monthly architecture review");

        let observations = serde_json::json!({
            "review_scope": "architecture",
            "components_reviewed": ["debate_engine", "memory_system", "risk_engine", "execution_engine"],
        });

        let lessons = serde_json::json!({
            "architecture_strengths": [],
            "architecture_weaknesses": [],
            "improvement_opportunities": [],
        });

        let proposed = serde_json::json!({
            "architecture_changes": [],
            "new_components": [],
            "deprecated_components": [],
        });

        let id = self
            .reflection_store
            .record(
                "monthly_architecture",
                Some("scheduled"),
                "system",
                observations.clone(),
                lessons.clone(),
                proposed.clone(),
            )
            .await?;

        let logs = self.reflection_store.list(1).await?;
        Ok(logs.into_iter().next().unwrap_or(EvolutionReflectionLog {
            id,
            reflection_type: "monthly_architecture".to_string(),
            trigger: Some("scheduled".to_string()),
            scope: "system".to_string(),
            observations,
            lessons_learned: lessons,
            proposed_changes: proposed,
            applied_changes: serde_json::json!([]),
            effectiveness_score: None,
            status: "pending".to_string(),
            reviewed_by: None,
            reviewed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }

    /// Triggered reflection: executed when a significant event occurs (e.g., large loss).
    pub async fn run_triggered_reflection(
        &self,
        trigger: &str,
    ) -> AgentResult<EvolutionReflectionLog> {
        info!(trigger, "Running triggered reflection");

        let observations = serde_json::json!({
            "trigger": trigger,
            "timestamp": Utc::now(),
        });

        let lessons = serde_json::json!({
            "trigger_analysis": trigger,
            "immediate_lessons": [],
        });

        let proposed = serde_json::json!({
            "immediate_actions": [],
            "preventive_measures": [],
        });

        let id = self
            .reflection_store
            .record(
                "triggered",
                Some(trigger),
                "fund_manager",
                observations.clone(),
                lessons.clone(),
                proposed.clone(),
            )
            .await?;

        let logs = self.reflection_store.list(1).await?;
        Ok(logs.into_iter().next().unwrap_or(EvolutionReflectionLog {
            id,
            reflection_type: "triggered".to_string(),
            trigger: Some(trigger.to_string()),
            scope: "fund_manager".to_string(),
            observations,
            lessons_learned: lessons,
            proposed_changes: proposed,
            applied_changes: serde_json::json!([]),
            effectiveness_score: None,
            status: "pending".to_string(),
            reviewed_by: None,
            reviewed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }

    /// Check if evolution is allowed (rate limit + safety guardrails).
    pub async fn check_evolution_allowed(&self) -> AgentResult<bool> {
        // Check daily evolution limit
        let today_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM evolution_logs
               WHERE created_at > NOW() - INTERVAL '1 day'
                 AND status = 'applied'"#,
        )
        .fetch_one(&self.db_pool)
        .await?;

        if today_count >= self.max_evolution_per_day as i64 {
            warn!("Evolution rate limit reached");
            return Ok(false);
        }

        Ok(true)
    }

    /// Verify that a proposed change doesn't violate immutable core rules.
    pub fn validate_against_core_rules(&self, change: &serde_json::Value) -> bool {
        let change_str = change.to_string().to_lowercase();
        for rule in &self.immutable_core_rules {
            let rule_lower = rule.to_lowercase();
            // If change attempts to modify a core rule, reject
            if change_str.contains(&rule_lower) && change_str.contains("remove") {
                return false;
            }
        }
        true
    }

    async fn extract_lessons(
        &self,
        decisions: &[sqlx::postgres::PgRow],
    ) -> AgentResult<Vec<serde_json::Value>> {
        let mut lessons = Vec::new();

        // Analyze confidence calibration
        let high_conf_wrong: Vec<_> = decisions
            .iter()
            .filter(|d| {
                let conf: Option<f64> = d.get("confidence");
                let correct: Option<bool> = d.get("was_correct");
                conf.unwrap_or(0.0) > 0.7 && correct == Some(false)
            })
            .collect();

        if !high_conf_wrong.is_empty() {
            lessons.push(serde_json::json!({
                "type": "overconfidence",
                "description": format!("Found {} high-confidence predictions that were wrong", high_conf_wrong.len()),
                "recommendation": "Reduce confidence thresholds or improve calibration",
            }));
        }

        // Analyze by market trend
        let mut trend_accuracy: std::collections::HashMap<String, (i32, i32)> = std::collections::HashMap::new();
        for d in decisions {
            let trend: Option<String> = d.get("market_trend");
            let correct: Option<bool> = d.get("was_correct");
            if let Some(t) = trend {
                let entry = trend_accuracy.entry(t).or_insert((0, 0));
                entry.0 += 1;
                if correct == Some(true) {
                    entry.1 += 1;
                }
            }
        }

        for (trend, (total, correct)) in trend_accuracy {
            if total > 3 {
                let accuracy = correct as f64 / total as f64;
                if accuracy < 0.4 {
                    lessons.push(serde_json::json!({
                        "type": "trend_bias",
                        "trend": trend,
                        "accuracy": accuracy,
                        "recommendation": format!("Poor performance in {} markets - consider strategy adjustment", trend),
                    }));
                }
            }
        }

        Ok(lessons)
    }

    async fn propose_prompt_improvements(
        &self,
        lessons: &[serde_json::Value],
    ) -> AgentResult<Vec<serde_json::Value>> {
        let mut proposals = Vec::new();

        for lesson in lessons {
            let lesson_type = lesson.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match lesson_type {
                "overconfidence" => {
                    proposals.push(serde_json::json!({
                        "type": "prompt_adjustment",
                        "target_agent": "fund_manager",
                        "change": "Add explicit confidence calibration instructions",
                        "new_instruction": "When confidence > 0.7, explicitly enumerate risk factors that could invalidate the prediction.",
                    }));
                }
                "trend_bias" => {
                    let trend = lesson.get("trend").and_then(|v| v.as_str()).unwrap_or("");
                    proposals.push(serde_json::json!({
                        "type": "prompt_adjustment",
                        "target_agent": "fund_manager",
                        "change": format!("Add {} market awareness", trend),
                        "new_instruction": format!("Pay extra attention to {} market conditions and be more conservative.", trend),
                    }));
                }
                _ => {}
            }
        }

        Ok(proposals)
    }

    /// Get evolution statistics.
    pub async fn get_stats(&self) -> AgentResult<serde_json::Value> {
        let prompt_versions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM prompt_versions")
                .fetch_one(&self.db_pool)
                .await
                .unwrap_or(0);

        let strategy_versions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM strategy_versions")
                .fetch_one(&self.db_pool)
                .await
                .unwrap_or(0);

        let reflections: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM reflection_logs")
                .fetch_one(&self.db_pool)
                .await
                .unwrap_or(0);

        let evolutions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM evolution_logs")
                .fetch_one(&self.db_pool)
                .await
                .unwrap_or(0);

        let rolled_back: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM evolution_logs WHERE rolled_back = TRUE",
        )
        .fetch_one(&self.db_pool)
        .await
        .unwrap_or(0);

        Ok(serde_json::json!({
            "prompt_versions": prompt_versions,
            "strategy_versions": strategy_versions,
            "reflections": reflections,
            "evolutions": evolutions,
            "rolled_back": rolled_back,
            "immutable_core_rules": self.immutable_core_rules,
            "max_evolution_per_day": self.max_evolution_per_day,
            "require_human_review": self.require_human_review,
        }))
    }

    pub fn prompt_store(&self) -> &Arc<PromptVersionStore> {
        &self.prompt_store
    }

    pub fn strategy_store(&self) -> &Arc<StrategyVersionStore> {
        &self.strategy_store
    }

    pub fn reflection_store(&self) -> &Arc<EvolutionReflectionStore> {
        &self.reflection_store
    }

    pub fn evolution_log_store(&self) -> &Arc<EvolutionLogStore> {
        &self.evolution_log_store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolution_type_display() {
        assert_eq!(EvolutionType::PromptEvolution.to_string(), "prompt");
        assert_eq!(EvolutionType::StrategyEvolution.to_string(), "strategy");
    }

    #[test]
    fn test_reflection_type_display() {
        assert_eq!(ReflectionType::DailyMorning.to_string(), "daily_morning");
        assert_eq!(ReflectionType::WeeklyReview.to_string(), "weekly_review");
    }

    #[test]
    fn test_version_status_display() {
        assert_eq!(VersionStatus::Draft.to_string(), "draft");
        assert_eq!(VersionStatus::Active.to_string(), "active");
    }

    #[tokio::test]
    async fn test_core_rules_validation() {
        let engine = EvolutionEngine::new(PgPool::connect_lazy("postgres://localhost/test").unwrap());
        let valid_change = serde_json::json!({"adjust": "confidence_threshold"});
        assert!(engine.validate_against_core_rules(&valid_change));

        let invalid_change = serde_json::json!({"action": "remove always enforce stop-loss rule"});
        assert!(!engine.validate_against_core_rules(&invalid_change));
    }
}
