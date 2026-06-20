// Multi-tier Memory Management System
// Implements: AGENT_SYSTEM_DESIGN.md Chapter 12
//
// L1 Short-term Working Memory (Redis, session-scoped cache)
// L2 Episodic Memory (PostgreSQL + pgvector, RAG retrieval)
// L3 Knowledge Memory (PostgreSQL + pgvector, validated knowledge)

use crate::agents::errors::{AgentError, AgentResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ============================================================================
// Memory Tier Enum
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryTier {
    L1ShortTerm,
    L2Episodic,
    L3Knowledge,
}

// ============================================================================
// L1 Short-term Memory (in-memory cache, simulating Redis)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermMemory {
    pub key: String,
    pub value: serde_json::Value,
    pub expires_at: DateTime<Utc>,
}

/// L1 Short-term working memory. Uses in-memory DashMap as a Redis-like cache.
/// In production, this should be backed by Redis connection.
pub struct ShortTermMemoryStore {
    cache: dashmap::DashMap<String, ShortTermMemory>,
    default_ttl_seconds: i64,
}

impl ShortTermMemoryStore {
    pub fn new() -> Self {
        Self {
            cache: dashmap::DashMap::new(),
            default_ttl_seconds: 300, // 5 minutes
        }
    }

    pub fn set(&self, key: &str, value: serde_json::Value) {
        self.set_with_ttl(key, value, self.default_ttl_seconds)
    }

    pub fn set_with_ttl(&self, key: &str, value: serde_json::Value, ttl_seconds: i64) {
        let expires_at = Utc::now() + chrono::Duration::seconds(ttl_seconds);
        self.cache.insert(
            key.to_string(),
            ShortTermMemory {
                key: key.to_string(),
                value,
                expires_at,
            },
        );
    }

    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        if let Some(entry) = self.cache.get(key) {
            if entry.expires_at > Utc::now() {
                return Some(entry.value.clone());
            }
        }
        self.cache.remove(key);
        None
    }

    pub fn remove(&self, key: &str) {
        self.cache.remove(key);
    }

    pub fn cleanup_expired(&self) -> usize {
        let now = Utc::now();
        let expired_keys: Vec<String> = self
            .cache
            .iter()
            .filter(|entry| entry.expires_at <= now)
            .map(|entry| entry.key.clone())
            .collect();
        let count = expired_keys.len();
        for key in expired_keys {
            self.cache.remove(&key);
        }
        count
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

impl Default for ShortTermMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// L2 Episodic Memory
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: Option<Uuid>,
    pub symbol: String,
    pub event_type: String,
    pub content: String,
    pub context: serde_json::Value,
    pub outcome: Option<serde_json::Value>,
    pub importance_score: f64,
    pub access_count: i32,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// L2 Episodic memory store backed by PostgreSQL + pgvector.
pub struct EpisodicMemoryStore {
    db_pool: PgPool,
}

impl EpisodicMemoryStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Store an episodic memory. Embedding is optional (computed externally).
    pub async fn store(
        &self,
        agent_id: &str,
        session_id: Option<Uuid>,
        symbol: &str,
        event_type: &str,
        content: &str,
        context: serde_json::Value,
        outcome: Option<serde_json::Value>,
        importance_score: f64,
    ) -> AgentResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO episodic_memory
                (id, agent_id, session_id, symbol, event_type, content, context, outcome, importance_score)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(id)
        .bind(agent_id)
        .bind(session_id)
        .bind(symbol)
        .bind(event_type)
        .bind(content)
        .bind(context)
        .bind(outcome)
        .bind(importance_score)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AgentError::DatabaseError(format!("Failed to store episodic memory: {}", e)))?;

        debug!(agent_id, symbol, "Stored episodic memory");
        Ok(id)
    }

    /// Retrieve an episodic memory by ID and update access count.
    pub async fn get(&self, id: Uuid) -> AgentResult<Option<EpisodicMemory>> {
        let row = sqlx::query(
            r#"
            UPDATE episodic_memory
            SET access_count = access_count + 1, last_accessed_at = NOW()
            WHERE id = $1
            RETURNING id, agent_id, session_id, symbol, event_type, content, context, outcome,
                      importance_score, access_count, created_at, last_accessed_at, expires_at
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AgentError::DatabaseError(format!("Failed to fetch episodic memory: {}", e)))?;

        Ok(row.map(|r| self.row_to_episodic(&r)))
    }

    /// List episodic memories with optional filters.
    pub async fn list(
        &self,
        agent_id: Option<&str>,
        symbol: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AgentResult<Vec<EpisodicMemory>> {
        let rows = if let Some(aid) = agent_id {
            if let Some(sym) = symbol {
                sqlx::query(
                    r#"SELECT id, agent_id, session_id, symbol, event_type, content, context, outcome,
                              importance_score, access_count, created_at, last_accessed_at, expires_at
                       FROM episodic_memory
                       WHERE agent_id = $1 AND symbol = $2
                       ORDER BY created_at DESC LIMIT $3 OFFSET $4"#,
                )
                .bind(aid)
                .bind(sym)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await?
            } else {
                sqlx::query(
                    r#"SELECT id, agent_id, session_id, symbol, event_type, content, context, outcome,
                              importance_score, access_count, created_at, last_accessed_at, expires_at
                       FROM episodic_memory
                       WHERE agent_id = $1
                       ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
                )
                .bind(aid)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await?
            }
        } else {
            sqlx::query(
                r#"SELECT id, agent_id, session_id, symbol, event_type, content, context, outcome,
                          importance_score, access_count, created_at, last_accessed_at, expires_at
                   FROM episodic_memory
                   ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db_pool)
            .await?
        };

        Ok(rows.iter().map(|r| self.row_to_episodic(r)).collect())
    }

    /// Semantic search using pgvector cosine distance.
    /// Note: embedding must be provided externally (e.g., from OpenAI text-embedding-3-small).
    pub async fn search_by_embedding(
        &self,
        embedding: &[f32],
        symbol: Option<&str>,
        limit: i64,
    ) -> AgentResult<Vec<EpisodicMemory>> {
        let embedding_str = format!(
            "[{}]",
            embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let rows = if let Some(sym) = symbol {
            sqlx::query(
                r#"SELECT id, agent_id, session_id, symbol, event_type, content, context, outcome,
                          importance_score, access_count, created_at, last_accessed_at, expires_at
                   FROM episodic_memory
                   WHERE symbol = $1 AND embedding IS NOT NULL
                   ORDER BY embedding <=> $2::vector
                   LIMIT $3"#,
            )
            .bind(sym)
            .bind(&embedding_str)
            .bind(limit)
            .fetch_all(&self.db_pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT id, agent_id, session_id, symbol, event_type, content, context, outcome,
                          importance_score, access_count, created_at, last_accessed_at, expires_at
                   FROM episodic_memory
                   WHERE embedding IS NOT NULL
                   ORDER BY embedding <=> $1::vector
                   LIMIT $2"#,
            )
            .bind(&embedding_str)
            .bind(limit)
            .fetch_all(&self.db_pool)
            .await?
        };

        Ok(rows.iter().map(|r| self.row_to_episodic(r)).collect())
    }

    /// Apply Ebbinghaus forgetting curve: decay importance over time.
    pub async fn apply_forgetting_curve(&self) -> AgentResult<usize> {
        // Decay: importance *= exp(-t / tau), where tau = 30 days
        let result = sqlx::query(
            r#"
            UPDATE episodic_memory
            SET importance_score = importance_score * EXP(-EXTRACT(EPOCH FROM (NOW() - created_at)) / (30 * 86400.0))
            WHERE importance_score > 0.01
            "#,
        )
        .execute(&self.db_pool)
        .await?;

        let cleaned = sqlx::query("DELETE FROM episodic_memory WHERE importance_score < 0.01")
            .execute(&self.db_pool)
            .await?;

        let decayed = result.rows_affected();
        let removed = cleaned.rows_affected();
        info!(decayed, removed, "Applied forgetting curve");
        Ok(decayed as usize)
    }

    fn row_to_episodic(&self, row: &sqlx::postgres::PgRow) -> EpisodicMemory {
        EpisodicMemory {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            session_id: row.get("session_id"),
            symbol: row.get("symbol"),
            event_type: row.get("event_type"),
            content: row.get("content"),
            context: row.get("context"),
            outcome: row.get("outcome"),
            importance_score: row.get("importance_score"),
            access_count: row.get("access_count"),
            created_at: row.get("created_at"),
            last_accessed_at: row.get("last_accessed_at"),
            expires_at: row.get("expires_at"),
        }
    }
}

// ============================================================================
// L3 Knowledge Memory
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeMemory {
    pub id: Uuid,
    pub agent_id: Option<String>,
    pub category: String,
    pub title: String,
    pub content: String,
    pub source_type: Option<String>,
    pub source_id: Option<Uuid>,
    pub confidence_score: f64,
    pub verification_count: i32,
    pub is_validated: bool,
    pub importance_score: f64,
    pub access_count: i32,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct KnowledgeMemoryStore {
    db_pool: PgPool,
}

impl KnowledgeMemoryStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn store(
        &self,
        agent_id: Option<&str>,
        category: &str,
        title: &str,
        content: &str,
        source_type: Option<&str>,
        source_id: Option<Uuid>,
        confidence_score: f64,
    ) -> AgentResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO knowledge_memory
                (id, agent_id, category, title, content, source_type, source_id, confidence_score)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(agent_id)
        .bind(category)
        .bind(title)
        .bind(content)
        .bind(source_type)
        .bind(source_id)
        .bind(confidence_score)
        .execute(&self.db_pool)
        .await
        .map_err(|e| AgentError::DatabaseError(format!("Failed to store knowledge: {}", e)))?;

        Ok(id)
    }

    pub async fn get(&self, id: Uuid) -> AgentResult<Option<KnowledgeMemory>> {
        let row = sqlx::query(
            r#"
            UPDATE knowledge_memory
            SET access_count = access_count + 1, last_accessed_at = NOW()
            WHERE id = $1
            RETURNING id, agent_id, category, title, content, source_type, source_id,
                      confidence_score, verification_count, is_validated, importance_score,
                      access_count, created_at, last_accessed_at, updated_at
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db_pool)
        .await?;

        Ok(row.map(|r| self.row_to_knowledge(&r)))
    }

    pub async fn list(
        &self,
        category: Option<&str>,
        validated_only: bool,
        limit: i64,
        offset: i64,
    ) -> AgentResult<Vec<KnowledgeMemory>> {
        let rows = if let Some(cat) = category {
            if validated_only {
                sqlx::query(
                    r#"SELECT id, agent_id, category, title, content, source_type, source_id,
                              confidence_score, verification_count, is_validated, importance_score,
                              access_count, created_at, last_accessed_at, updated_at
                       FROM knowledge_memory
                       WHERE category = $1 AND is_validated = TRUE
                       ORDER BY confidence_score DESC, created_at DESC LIMIT $2 OFFSET $3"#,
                )
                .bind(cat)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await?
            } else {
                sqlx::query(
                    r#"SELECT id, agent_id, category, title, content, source_type, source_id,
                              confidence_score, verification_count, is_validated, importance_score,
                              access_count, created_at, last_accessed_at, updated_at
                       FROM knowledge_memory
                       WHERE category = $1
                       ORDER BY confidence_score DESC, created_at DESC LIMIT $2 OFFSET $3"#,
                )
                .bind(cat)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.db_pool)
                .await?
            }
        } else {
            sqlx::query(
                r#"SELECT id, agent_id, category, title, content, source_type, source_id,
                          confidence_score, verification_count, is_validated, importance_score,
                          access_count, created_at, last_accessed_at, updated_at
                   FROM knowledge_memory
                   ORDER BY confidence_score DESC, created_at DESC LIMIT $1 OFFSET $2"#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db_pool)
            .await?
        };

        Ok(rows.iter().map(|r| self.row_to_knowledge(r)).collect())
    }

    pub async fn search_by_embedding(
        &self,
        embedding: &[f32],
        category: Option<&str>,
        limit: i64,
    ) -> AgentResult<Vec<KnowledgeMemory>> {
        let embedding_str = format!(
            "[{}]",
            embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let rows = if let Some(cat) = category {
            sqlx::query(
                r#"SELECT id, agent_id, category, title, content, source_type, source_id,
                          confidence_score, verification_count, is_validated, importance_score,
                          access_count, created_at, last_accessed_at, updated_at
                   FROM knowledge_memory
                   WHERE category = $1 AND embedding IS NOT NULL AND is_validated = TRUE
                   ORDER BY embedding <=> $2::vector
                   LIMIT $3"#,
            )
            .bind(cat)
            .bind(&embedding_str)
            .bind(limit)
            .fetch_all(&self.db_pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT id, agent_id, category, title, content, source_type, source_id,
                          confidence_score, verification_count, is_validated, importance_score,
                          access_count, created_at, last_accessed_at, updated_at
                   FROM knowledge_memory
                   WHERE embedding IS NOT NULL AND is_validated = TRUE
                   ORDER BY embedding <=> $1::vector
                   LIMIT $2"#,
            )
            .bind(&embedding_str)
            .bind(limit)
            .fetch_all(&self.db_pool)
            .await?
        };

        Ok(rows.iter().map(|r| self.row_to_knowledge(r)).collect())
    }

    /// Validate a knowledge item by incrementing verification count.
    pub async fn validate(&self, id: Uuid) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE knowledge_memory
               SET verification_count = verification_count + 1,
                   is_validated = TRUE,
                   confidence_score = LEAST(1.0, confidence_score + 0.1),
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .execute(&self.db_pool)
        .await?;
        Ok(())
    }

    /// Invalidate a knowledge item (failed verification).
    pub async fn invalidate(&self, id: Uuid) -> AgentResult<()> {
        sqlx::query(
            r#"UPDATE knowledge_memory
               SET confidence_score = GREATEST(0.0, confidence_score - 0.2),
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .execute(&self.db_pool)
        .await?;
        Ok(())
    }

    fn row_to_knowledge(&self, row: &sqlx::postgres::PgRow) -> KnowledgeMemory {
        KnowledgeMemory {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            category: row.get("category"),
            title: row.get("title"),
            content: row.get("content"),
            source_type: row.get("source_type"),
            source_id: row.get("source_id"),
            confidence_score: row.get("confidence_score"),
            verification_count: row.get("verification_count"),
            is_validated: row.get("is_validated"),
            importance_score: row.get("importance_score"),
            access_count: row.get("access_count"),
            created_at: row.get("created_at"),
            last_accessed_at: row.get("last_accessed_at"),
            updated_at: row.get("updated_at"),
        }
    }
}

// ============================================================================
// Agent Calibration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCalibration {
    pub id: Uuid,
    pub agent_id: String,
    pub calibration_date: chrono::NaiveDate,
    pub calibration_factor: f64,
    pub accuracy_score: f64,
    pub bias_score: f64,
    pub confidence_correlation: f64,
    pub sample_size: i32,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct CalibrationStore {
    db_pool: PgPool,
}

impl CalibrationStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn record(
        &self,
        agent_id: &str,
        calibration_factor: f64,
        accuracy_score: f64,
        bias_score: f64,
        confidence_correlation: f64,
        sample_size: i32,
        notes: Option<&str>,
    ) -> AgentResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_calibration
                (id, agent_id, calibration_factor, accuracy_score, bias_score,
                 confidence_correlation, sample_size, notes)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (agent_id, calibration_date)
            DO UPDATE SET
                calibration_factor = EXCLUDED.calibration_factor,
                accuracy_score = EXCLUDED.accuracy_score,
                bias_score = EXCLUDED.bias_score,
                confidence_correlation = EXCLUDED.confidence_correlation,
                sample_size = EXCLUDED.sample_size,
                notes = EXCLUDED.notes
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(agent_id)
        .bind(calibration_factor)
        .bind(accuracy_score)
        .bind(bias_score)
        .bind(confidence_correlation)
        .bind(sample_size)
        .bind(notes)
        .fetch_one(&self.db_pool)
        .await?;

        Ok(id)
    }

    pub async fn get_latest(&self, agent_id: &str) -> AgentResult<Option<AgentCalibration>> {
        let row = sqlx::query(
            r#"SELECT id, agent_id, calibration_date, calibration_factor, accuracy_score,
                      bias_score, confidence_correlation, sample_size, notes, created_at
               FROM agent_calibration
               WHERE agent_id = $1
               ORDER BY calibration_date DESC
               LIMIT 1"#,
        )
        .bind(agent_id)
        .fetch_optional(&self.db_pool)
        .await?;

        Ok(row.map(|r| AgentCalibration {
            id: r.get("id"),
            agent_id: r.get("agent_id"),
            calibration_date: r.get("calibration_date"),
            calibration_factor: r.get("calibration_factor"),
            accuracy_score: r.get("accuracy_score"),
            bias_score: r.get("bias_score"),
            confidence_correlation: r.get("confidence_correlation"),
            sample_size: r.get("sample_size"),
            notes: r.get("notes"),
            created_at: r.get("created_at"),
        }))
    }

    pub async fn list(&self, limit: i64) -> AgentResult<Vec<AgentCalibration>> {
        let rows = sqlx::query(
            r#"SELECT id, agent_id, calibration_date, calibration_factor, accuracy_score,
                      bias_score, confidence_correlation, sample_size, notes, created_at
               FROM agent_calibration
               ORDER BY calibration_date DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| AgentCalibration {
                id: r.get("id"),
                agent_id: r.get("agent_id"),
                calibration_date: r.get("calibration_date"),
                calibration_factor: r.get("calibration_factor"),
                accuracy_score: r.get("accuracy_score"),
                bias_score: r.get("bias_score"),
                confidence_correlation: r.get("confidence_correlation"),
                sample_size: r.get("sample_size"),
                notes: r.get("notes"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}

// ============================================================================
// Memory Reflection Log
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReflectionLog {
    pub id: Uuid,
    pub reflection_type: String,
    pub agent_id: Option<String>,
    pub trigger_event: Option<String>,
    pub insights: serde_json::Value,
    pub patterns_detected: serde_json::Value,
    pub knowledge_validated: i32,
    pub knowledge_invalidated: i32,
    pub agents_calibrated: i32,
    pub memory_items_cleaned: i32,
    pub duration_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
}

pub struct ReflectionLogStore {
    db_pool: PgPool,
}

impl ReflectionLogStore {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn record(
        &self,
        reflection_type: &str,
        agent_id: Option<&str>,
        trigger_event: Option<&str>,
        insights: serde_json::Value,
        patterns_detected: serde_json::Value,
        knowledge_validated: i32,
        knowledge_invalidated: i32,
        agents_calibrated: i32,
        memory_items_cleaned: i32,
        duration_ms: Option<i64>,
    ) -> AgentResult<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO memory_reflection_log
               (id, reflection_type, agent_id, trigger_event, insights, patterns_detected,
                knowledge_validated, knowledge_invalidated, agents_calibrated,
                memory_items_cleaned, duration_ms)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        )
        .bind(id)
        .bind(reflection_type)
        .bind(agent_id)
        .bind(trigger_event)
        .bind(insights)
        .bind(patterns_detected)
        .bind(knowledge_validated)
        .bind(knowledge_invalidated)
        .bind(agents_calibrated)
        .bind(memory_items_cleaned)
        .bind(duration_ms)
        .execute(&self.db_pool)
        .await?;
        Ok(id)
    }

    pub async fn list(&self, limit: i64) -> AgentResult<Vec<MemoryReflectionLog>> {
        let rows = sqlx::query(
            r#"SELECT id, reflection_type, agent_id, trigger_event, insights, patterns_detected,
                      knowledge_validated, knowledge_invalidated, agents_calibrated,
                      memory_items_cleaned, duration_ms, created_at
               FROM memory_reflection_log
               ORDER BY created_at DESC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&self.db_pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| MemoryReflectionLog {
                id: r.get("id"),
                reflection_type: r.get("reflection_type"),
                agent_id: r.get("agent_id"),
                trigger_event: r.get("trigger_event"),
                insights: r.get("insights"),
                patterns_detected: r.get("patterns_detected"),
                knowledge_validated: r.get("knowledge_validated"),
                knowledge_invalidated: r.get("knowledge_invalidated"),
                agents_calibrated: r.get("agents_calibrated"),
                memory_items_cleaned: r.get("memory_items_cleaned"),
                duration_ms: r.get("duration_ms"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}

// ============================================================================
// Memory Manager - Orchestrates all three tiers
// ============================================================================

pub struct MemoryManager {
    pub l1: Arc<ShortTermMemoryStore>,
    pub l2: Arc<EpisodicMemoryStore>,
    pub l3: Arc<KnowledgeMemoryStore>,
    pub calibration: Arc<CalibrationStore>,
    pub reflection_log: Arc<ReflectionLogStore>,
}

impl MemoryManager {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            l1: Arc::new(ShortTermMemoryStore::new()),
            l2: Arc::new(EpisodicMemoryStore::new(db_pool.clone())),
            l3: Arc::new(KnowledgeMemoryStore::new(db_pool.clone())),
            calibration: Arc::new(CalibrationStore::new(db_pool.clone())),
            reflection_log: Arc::new(ReflectionLogStore::new(db_pool)),
        }
    }

    /// Get memory statistics for dashboard.
    pub async fn get_stats(&self) -> AgentResult<serde_json::Value> {
        let l1_count = self.l1.len() as i64;

        let l2_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM episodic_memory")
            .fetch_one(self.l2.db_pool_ref())
            .await
            .unwrap_or(0);

        let l3_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_memory")
            .fetch_one(self.l3.db_pool_ref())
            .await
            .unwrap_or(0);

        let l3_validated: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_memory WHERE is_validated = TRUE")
                .fetch_one(self.l3.db_pool_ref())
                .await
                .unwrap_or(0);

        let calibration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_calibration")
                .fetch_one(self.calibration.db_pool_ref())
                .await
                .unwrap_or(0);

        let reflection_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memory_reflection_log")
                .fetch_one(self.reflection_log.db_pool_ref())
                .await
                .unwrap_or(0);

        Ok(serde_json::json!({
            "l1_short_term_count": l1_count,
            "l2_episodic_count": l2_count,
            "l3_knowledge_count": l3_count,
            "l3_validated_count": l3_validated,
            "calibration_count": calibration_count,
            "reflection_count": reflection_count,
        }))
    }

    /// Daily reflection cycle: result backfill → pattern discovery →
    /// knowledge validation → knowledge write → agent calibration → memory cleanup.
    pub async fn run_reflection_cycle(&self) -> AgentResult<MemoryReflectionLog> {
        let start = std::time::Instant::now();
        info!("Starting memory reflection cycle");

        // Step 1: Result backfill (already done in simulation.rs::record_trade_outcome)
        // Step 2: Pattern discovery - find agents with consistently wrong predictions
        let patterns = self.discover_patterns().await?;

        // Step 3: Knowledge validation - validate/invalidate knowledge based on recent outcomes
        let (validated, invalidated) = self.validate_knowledge().await?;

        // Step 4: Knowledge write - promote high-confidence episodic memories to L3
        let promoted = self.promote_episodic_to_knowledge().await?;

        // Step 5: Agent calibration - update calibration factors
        let calibrated = self.calibrate_agents().await?;

        // Step 6: Memory cleanup - apply forgetting curve
        let cleaned = self.l2.apply_forgetting_curve().await?;
        self.l1.cleanup_expired();

        let duration_ms = start.elapsed().as_millis() as i64;
        let insights = serde_json::json!({
            "patterns_found": patterns.len(),
            "knowledge_promoted": promoted,
            "agents_calibrated": calibrated,
        });

        let log = self
            .reflection_log
            .record(
                "daily",
                None,
                Some("scheduled"),
                insights,
                serde_json::json!(patterns),
                validated,
                invalidated,
                calibrated,
                cleaned,
                Some(duration_ms),
            )
            .await?;

        info!(
            validated,
            invalidated, promoted, calibrated, cleaned, "Reflection cycle completed"
        );
        Ok(MemoryReflectionLog {
            id: log,
            reflection_type: "daily".to_string(),
            agent_id: None,
            trigger_event: Some("scheduled".to_string()),
            insights: serde_json::json!({
                "patterns_found": patterns.len(),
                "knowledge_promoted": promoted,
                "agents_calibrated": calibrated,
            }),
            patterns_detected: serde_json::json!(patterns),
            knowledge_validated: validated,
            knowledge_invalidated: invalidated,
            agents_calibrated: calibrated,
            memory_items_cleaned: cleaned,
            duration_ms: Some(duration_ms),
            created_at: Utc::now(),
        })
    }

    async fn discover_patterns(&self) -> AgentResult<Vec<serde_json::Value>> {
        // Find agents with win rate significantly below 50% in last 30 days
        let rows = sqlx::query(
            r#"SELECT agent_id,
                      COUNT(*) as total,
                      SUM(CASE WHEN was_correct THEN 1 ELSE 0 END) as correct,
                      AVG(confidence) as avg_confidence
               FROM decision_memory
               WHERE created_at > NOW() - INTERVAL '30 days'
               GROUP BY agent_id
               HAVING COUNT(*) > 5
               ORDER BY (SUM(CASE WHEN was_correct THEN 1 ELSE 0 END)::FLOAT / COUNT(*)) ASC"#,
        )
        .fetch_all(self.l2.db_pool_ref())
        .await?;

        let patterns: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let total: i64 = r.get("total");
                let correct: i64 = r.get("correct");
                let win_rate = if total > 0 {
                    correct as f64 / total as f64
                } else {
                    0.0
                };
                serde_json::json!({
                    "agent_id": r.get::<String, _>("agent_id"),
                    "total_predictions": total,
                    "correct_predictions": correct,
                    "win_rate": win_rate,
                    "avg_confidence": r.get::<Option<f64>, _>("avg_confidence"),
                    "pattern": if win_rate < 0.4 { "low_accuracy" } else { "normal" },
                })
            })
            .collect();

        Ok(patterns)
    }

    async fn validate_knowledge(&self) -> AgentResult<(i32, i32)> {
        // Validate knowledge that aligns with recent correct decisions
        let validated = sqlx::query(
            r#"UPDATE knowledge_memory
               SET verification_count = verification_count + 1,
                   is_validated = TRUE,
                   confidence_score = LEAST(1.0, confidence_score + 0.05)
               FROM decision_memory
               WHERE knowledge_memory.source_id = decision_memory.id
                 AND decision_memory.was_correct = TRUE
                 AND decision_memory.created_at > NOW() - INTERVAL '7 days'"#,
        )
        .execute(self.l3.db_pool_ref())
        .await?;
        let v = validated.rows_affected() as i32;

        // Invalidate knowledge that contradicts recent wrong decisions
        let invalidated = sqlx::query(
            r#"UPDATE knowledge_memory
               SET confidence_score = GREATEST(0.0, confidence_score - 0.1)
               FROM decision_memory
               WHERE knowledge_memory.source_id = decision_memory.id
                 AND decision_memory.was_correct = FALSE
                 AND decision_memory.created_at > NOW() - INTERVAL '7 days'"#,
        )
        .execute(self.l3.db_pool_ref())
        .await?;
        let inv = invalidated.rows_affected() as i32;

        Ok((v, inv))
    }

    async fn promote_episodic_to_knowledge(&self) -> AgentResult<i32> {
        // Promote high-importance episodic memories with positive outcomes to L3
        let result = sqlx::query(
            r#"INSERT INTO knowledge_memory (id, agent_id, category, title, content, source_type, source_id, confidence_score)
               SELECT
                   gen_random_uuid(),
                   agent_id,
                   'promoted_pattern',
                   LEFT(content, 100),
                   content,
                   'episodic_memory',
                   id,
                   importance_score
               FROM episodic_memory
               WHERE importance_score > 0.7
                 AND outcome IS NOT NULL
                 AND created_at > NOW() - INTERVAL '7 days'
                 AND id NOT IN (SELECT source_id FROM knowledge_memory WHERE source_type = 'episodic_memory')
               LIMIT 10"#,
        )
        .execute(self.l3.db_pool_ref())
        .await?;

        Ok(result.rows_affected() as i32)
    }

    async fn calibrate_agents(&self) -> AgentResult<i32> {
        // Compute calibration factor for each agent based on recent accuracy
        let rows = sqlx::query(
            r#"SELECT agent_id,
                      COUNT(*) as total,
                      SUM(CASE WHEN was_correct THEN 1 ELSE 0 END) as correct,
                      AVG(confidence) as avg_confidence
               FROM decision_memory
               WHERE created_at > NOW() - INTERVAL '30 days'
               GROUP BY agent_id
               HAVING COUNT(*) > 3"#,
        )
        .fetch_all(self.calibration.db_pool_ref())
        .await?;

        let mut count = 0;
        for row in rows {
            let agent_id: String = row.get("agent_id");
            let total: i64 = row.get("total");
            let correct: i64 = row.get("correct");
            let avg_confidence: Option<f64> = row.get("avg_confidence");

            let accuracy = if total > 0 {
                correct as f64 / total as f64
            } else {
                0.5
            };

            // Calibration factor: ratio of actual accuracy to average confidence
            let calibration_factor = if let Some(conf) = avg_confidence {
                if conf > 0.0 {
                    (accuracy / conf).min(2.0).max(0.1)
                } else {
                    1.0
                }
            } else {
                1.0
            };

            let bias_score = accuracy - 0.5; // positive = overconfident, negative = underconfident
            let confidence_correlation = if total > 5 { 0.5 } else { 0.0 };

            self.calibration
                .record(
                    &agent_id,
                    calibration_factor,
                    accuracy,
                    bias_score,
                    confidence_correlation,
                    total as i32,
                    Some("auto-calibrated by reflection cycle"),
                )
                .await?;
            count += 1;
        }

        Ok(count)
    }
}

// Helper trait to access db_pool from stores
pub trait DbPoolRef {
    fn db_pool_ref(&self) -> &PgPool;
}

impl DbPoolRef for EpisodicMemoryStore {
    fn db_pool_ref(&self) -> &PgPool {
        &self.db_pool
    }
}

impl DbPoolRef for KnowledgeMemoryStore {
    fn db_pool_ref(&self) -> &PgPool {
        &self.db_pool
    }
}

impl DbPoolRef for CalibrationStore {
    fn db_pool_ref(&self) -> &PgPool {
        &self.db_pool
    }
}

impl DbPoolRef for ReflectionLogStore {
    fn db_pool_ref(&self) -> &PgPool {
        &self.db_pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_term_memory_basic() {
        let store = ShortTermMemoryStore::new();
        store.set("key1", serde_json::json!({"value": 42}));
        assert_eq!(
            store.get("key1"),
            Some(serde_json::json!({"value": 42}))
        );
        store.remove("key1");
        assert_eq!(store.get("key1"), None);
    }

    #[test]
    fn test_short_term_memory_ttl() {
        let store = ShortTermMemoryStore::new();
        store.set_with_ttl("key1", serde_json::json!("short"), -1); // already expired
        assert_eq!(store.get("key1"), None);
    }

    #[test]
    fn test_short_term_memory_cleanup() {
        let store = ShortTermMemoryStore::new();
        store.set_with_ttl("expired1", serde_json::json!(1), -1);
        store.set_with_ttl("expired2", serde_json::json!(2), -1);
        store.set("valid", serde_json::json!(3));
        let cleaned = store.cleanup_expired();
        assert_eq!(cleaned, 2);
        assert_eq!(store.len(), 1);
    }
}
