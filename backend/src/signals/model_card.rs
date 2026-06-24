//! Model Card Module
//! 模型卡：模型发布治理与可信证据聚合
//!
//! 依据《系统评估与演进规划》第四阶段任务3：
//!   "模型卡、校准曲线、反事实解释"
//!
//! ModelCard 是模型发布治理的核心产物，聚合：
//! - 校准报告（Brier Score、LogLoss、校准曲线）
//! - 信任评估（三级门禁：display_only/comparable/promotion_eligible）
//! - 预测统计（样本量、准确率）
//! - 失效条件与已知限制
//!
//! 参考 Google ModelCard 规范 + 系统现有 decision_cards.invalidation_conditions 设计

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// 模型卡状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCardStatus {
    /// 草稿：刚创建，尚未验证
    Draft,
    /// 影子盘：在影子环境中运行，未影响实盘
    Shadow,
    /// 活跃：已通过门禁，用于实盘决策
    Active,
    /// 已弃用：不再使用，但保留历史记录
    Deprecated,
    /// 已回滚：从 Active 回滚到之前的版本
    RolledBack,
}

impl ModelCardStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Shadow => "shadow",
            Self::Active => "active",
            Self::Deprecated => "deprecated",
            Self::RolledBack => "rolled_back",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "shadow" => Some(Self::Shadow),
            "active" => Some(Self::Active),
            "deprecated" => Some(Self::Deprecated),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }
}

/// 模型卡
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCard {
    pub card_id: Uuid,
    pub model_version: String,
    pub model_type: String,
    pub model_name: String,

    // 模型描述
    pub description: Option<String>,
    pub intended_use: Option<String>,
    pub out_of_scope: Option<String>,
    pub training_data_summary: Option<serde_json::Value>,
    pub feature_version: Option<String>,
    pub features_used: Option<serde_json::Value>,

    // 质量证据
    pub calibration_report_id: Option<Uuid>,
    pub trust_assessment_id: Option<Uuid>,
    pub brier_score: Option<f64>,
    pub log_loss: Option<f64>,
    pub accuracy: Option<f64>,
    pub calibration_curve: Option<serde_json::Value>,

    // 失效条件与风险
    pub invalidation_conditions: serde_json::Value,
    pub known_limitations: serde_json::Value,
    pub ethical_considerations: Option<String>,

    // 版本与发布治理
    pub status: ModelCardStatus,
    pub shadow_period_start: Option<DateTime<Utc>>,
    pub shadow_period_end: Option<DateTime<Utc>>,
    pub promotion_eligible: bool,
    pub previous_version: Option<String>,

    // 审计
    pub created_by: Option<i64>,
    pub approved_by: Option<i64>,
    pub approved_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 模型卡构建器输入
#[derive(Debug, Clone)]
pub struct ModelCardInput {
    pub model_version: String,
    pub model_type: String,
    pub model_name: String,
    pub description: Option<String>,
    pub intended_use: Option<String>,
    pub out_of_scope: Option<String>,
    pub training_data_summary: Option<serde_json::Value>,
    pub feature_version: Option<String>,
    pub features_used: Option<serde_json::Value>,
    pub invalidation_conditions: Option<serde_json::Value>,
    pub known_limitations: Option<serde_json::Value>,
    pub ethical_considerations: Option<String>,
    pub created_by: Option<i64>,
}

/// 从已有数据聚合模型卡
///
/// 从 signal_calibration_reports、backtest_trust_assessments、signal_predictions
/// 聚合校准证据和预测统计，生成完整的模型卡
pub async fn aggregate_from_existing(
    pool: &PgPool,
    input: &ModelCardInput,
) -> Result<ModelCard, sqlx::Error> {
    let now = Utc::now();

    // 1. 查询最新的校准报告
    let cal_row = sqlx::query(
        r#"SELECT report_id, brier_score, log_loss, accuracy, calibration_curve
           FROM signal_calibration_reports
           WHERE model_version = $1
           ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(&input.model_version)
    .fetch_optional(pool)
    .await?;

    let (calibration_report_id, brier_score, log_loss, accuracy, calibration_curve) = cal_row
        .map(|r| {
            (
                Some(r.get::<Uuid, _>("report_id")),
                Some(r.get::<f64, _>("brier_score")),
                Some(r.get::<f64, _>("log_loss")),
                Some(r.get::<f64, _>("accuracy")),
                Some(r.get::<serde_json::Value, _>("calibration_curve")),
            )
        })
        .unwrap_or((None, None, None, None, None));

    // 2. 查询最新的信任评估（通过 model_version 关联 job 再关联 assessment）
    let trust_row = sqlx::query(
        r#"SELECT bta.assessment_id
           FROM backtest_trust_assessments bta
           JOIN backtest_jobs bj ON bta.job_id = bj.job_id
           WHERE bj.strategy_id = $1
           ORDER BY bta.assessed_at DESC LIMIT 1"#,
    )
    .bind(&input.model_version)
    .fetch_optional(pool)
    .await?;

    let trust_assessment_id = trust_row.map(|r| r.get::<Uuid, _>("assessment_id"));

    // 3. 检查是否可晋级（信任等级为 promotion_eligible）
    let promotion_eligible = if trust_assessment_id.is_some() {
        sqlx::query(
            r#"SELECT promotion_eligible FROM backtest_trust_assessments
               WHERE assessment_id = $1"#,
        )
        .bind(trust_assessment_id.unwrap())
        .fetch_optional(pool)
        .await?
        .map(|r| r.get::<bool, _>("promotion_eligible"))
        .unwrap_or(false)
    } else {
        false
    };

    // 4. 如果没有提供失效条件，生成默认的
    let invalidation_conditions = input.invalidation_conditions.clone().unwrap_or_else(|| {
        serde_json::json!([
            {"condition": "brier_score > 0.33", "description": "概率校准严重退化"},
            {"condition": "data_freshness > 300s", "description": "数据延迟超过5分钟"},
            {"condition": "market_regime_shift", "description": "市场状态发生显著变化"},
            {"condition": "sample_size < 30", "description": "样本量不足"}
        ])
    });

    let known_limitations = input.known_limitations.clone().unwrap_or_else(|| {
        serde_json::json!([
            "模型基于历史数据训练，无法预测黑天鹅事件",
            "极端市场条件下可能失效",
            "需要定期重新校准"
        ])
    });

    Ok(ModelCard {
        card_id: Uuid::new_v4(),
        model_version: input.model_version.clone(),
        model_type: input.model_type.clone(),
        model_name: input.model_name.clone(),
        description: input.description.clone(),
        intended_use: input.intended_use.clone(),
        out_of_scope: input.out_of_scope.clone(),
        training_data_summary: input.training_data_summary.clone(),
        feature_version: input.feature_version.clone(),
        features_used: input.features_used.clone(),
        calibration_report_id,
        trust_assessment_id,
        brier_score,
        log_loss,
        accuracy,
        calibration_curve,
        invalidation_conditions,
        known_limitations,
        ethical_considerations: input.ethical_considerations.clone(),
        status: ModelCardStatus::Draft,
        shadow_period_start: None,
        shadow_period_end: None,
        promotion_eligible,
        previous_version: None,
        created_by: input.created_by,
        approved_by: None,
        approved_at: None,
        created_at: now,
        updated_at: now,
    })
}

/// 保存模型卡到数据库
pub async fn save_card(pool: &PgPool, card: &ModelCard) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO model_cards
           (card_id, model_version, model_type, model_name,
            description, intended_use, out_of_scope,
            training_data_summary, feature_version, features_used,
            calibration_report_id, trust_assessment_id,
            brier_score, log_loss, accuracy, calibration_curve,
            invalidation_conditions, known_limitations, ethical_considerations,
            status, shadow_period_start, shadow_period_end,
            promotion_eligible, previous_version,
            created_by, approved_by, approved_at,
            created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                   $11, $12, $13, $14, $15, $16, $17, $18, $19,
                   $20, $21, $22, $23, $24, $25, $26, $27, $28, $29)
           ON CONFLICT (model_version) DO UPDATE SET
            model_type = EXCLUDED.model_type,
            model_name = EXCLUDED.model_name,
            description = EXCLUDED.description,
            intended_use = EXCLUDED.intended_use,
            out_of_scope = EXCLUDED.out_of_scope,
            training_data_summary = EXCLUDED.training_data_summary,
            feature_version = EXCLUDED.feature_version,
            features_used = EXCLUDED.features_used,
            calibration_report_id = EXCLUDED.calibration_report_id,
            trust_assessment_id = EXCLUDED.trust_assessment_id,
            brier_score = EXCLUDED.brier_score,
            log_loss = EXCLUDED.log_loss,
            accuracy = EXCLUDED.accuracy,
            calibration_curve = EXCLUDED.calibration_curve,
            invalidation_conditions = EXCLUDED.invalidation_conditions,
            known_limitations = EXCLUDED.known_limitations,
            ethical_considerations = EXCLUDED.ethical_considerations,
            promotion_eligible = EXCLUDED.promotion_eligible,
            updated_at = NOW()"#,
    )
    .bind(card.card_id)
    .bind(&card.model_version)
    .bind(&card.model_type)
    .bind(&card.model_name)
    .bind(&card.description)
    .bind(&card.intended_use)
    .bind(&card.out_of_scope)
    .bind(&card.training_data_summary)
    .bind(&card.feature_version)
    .bind(&card.features_used)
    .bind(card.calibration_report_id)
    .bind(card.trust_assessment_id)
    .bind(card.brier_score)
    .bind(card.log_loss)
    .bind(card.accuracy)
    .bind(&card.calibration_curve)
    .bind(&card.invalidation_conditions)
    .bind(&card.known_limitations)
    .bind(&card.ethical_considerations)
    .bind(card.status.as_str())
    .bind(card.shadow_period_start)
    .bind(card.shadow_period_end)
    .bind(card.promotion_eligible)
    .bind(&card.previous_version)
    .bind(card.created_by)
    .bind(card.approved_by)
    .bind(card.approved_at)
    .bind(card.created_at)
    .bind(card.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查询单个模型卡
pub async fn get_card(
    pool: &PgPool,
    model_version: &str,
) -> Result<Option<ModelCard>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT card_id, model_version, model_type, model_name,
                  description, intended_use, out_of_scope,
                  training_data_summary, feature_version, features_used,
                  calibration_report_id, trust_assessment_id,
                  brier_score, log_loss, accuracy, calibration_curve,
                  invalidation_conditions, known_limitations, ethical_considerations,
                  status, shadow_period_start, shadow_period_end,
                  promotion_eligible, previous_version,
                  created_by, approved_by, approved_at,
                  created_at, updated_at
           FROM model_cards WHERE model_version = $1"#,
    )
    .bind(model_version)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| row_to_card(&r)))
}

/// 查询模型卡列表
pub async fn list_cards(
    pool: &PgPool,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<ModelCard>, sqlx::Error> {
    let rows = if let Some(st) = status {
        sqlx::query(
            r#"SELECT card_id, model_version, model_type, model_name,
                      description, intended_use, out_of_scope,
                      training_data_summary, feature_version, features_used,
                      calibration_report_id, trust_assessment_id,
                      brier_score, log_loss, accuracy, calibration_curve,
                      invalidation_conditions, known_limitations, ethical_considerations,
                      status, shadow_period_start, shadow_period_end,
                      promotion_eligible, previous_version,
                      created_by, approved_by, approved_at,
                      created_at, updated_at
               FROM model_cards WHERE status = $1
               ORDER BY updated_at DESC LIMIT $2"#,
        )
        .bind(st)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT card_id, model_version, model_type, model_name,
                      description, intended_use, out_of_scope,
                      training_data_summary, feature_version, features_used,
                      calibration_report_id, trust_assessment_id,
                      brier_score, log_loss, accuracy, calibration_curve,
                      invalidation_conditions, known_limitations, ethical_considerations,
                      status, shadow_period_start, shadow_period_end,
                      promotion_eligible, previous_version,
                      created_by, approved_by, approved_at,
                      created_at, updated_at
               FROM model_cards
               ORDER BY updated_at DESC LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.iter().map(|r| row_to_card(r)).collect())
}

/// 发布门禁：变更模型卡状态
///
/// 状态转换规则：
/// - draft -> shadow: 需要 promotion_eligible = true
/// - shadow -> active: 需要设置 shadow_period_end
/// - active -> deprecated: 直接允许
/// - active -> rolled_back: 需要 previous_version 存在
pub async fn promote_card(
    pool: &PgPool,
    model_version: &str,
    new_status: ModelCardStatus,
    approved_by: Option<i64>,
) -> Result<ModelCard, String> {
    let current = get_card(pool, model_version)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| format!("Model card not found: {}", model_version))?;

    // 验证状态转换
    match (current.status, new_status) {
        (ModelCardStatus::Draft, ModelCardStatus::Shadow) => {
            if !current.promotion_eligible {
                return Err("Cannot promote to shadow: model is not promotion eligible".into());
            }
        }
        (ModelCardStatus::Shadow, ModelCardStatus::Active) => {
            // 影子期需要结束
            if current.shadow_period_end.is_none() {
                return Err("Cannot promote to active: shadow period end not set".into());
            }
        }
        (ModelCardStatus::Active, ModelCardStatus::Deprecated) => {}
        (ModelCardStatus::Active, ModelCardStatus::RolledBack) => {
            if current.previous_version.is_none() {
                return Err("Cannot rollback: no previous version".into());
            }
        }
        _ => {
            return Err(format!(
                "Invalid status transition: {:?} -> {:?}",
                current.status, new_status
            ));
        }
    }

    let now = Utc::now();
    sqlx::query(
        r#"UPDATE model_cards
           SET status = $1, approved_by = $2, approved_at = $3, updated_at = NOW()
           WHERE model_version = $4"#,
    )
    .bind(new_status.as_str())
    .bind(approved_by)
    .bind(now)
    .bind(model_version)
    .execute(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    get_card(pool, model_version)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Card not found after update".into())
}

/// 从数据库行构建 ModelCard
fn row_to_card(r: &sqlx::postgres::PgRow) -> ModelCard {
    ModelCard {
        card_id: r.get("card_id"),
        model_version: r.get("model_version"),
        model_type: r.get("model_type"),
        model_name: r.get("model_name"),
        description: r.get("description"),
        intended_use: r.get("intended_use"),
        out_of_scope: r.get("out_of_scope"),
        training_data_summary: r.get("training_data_summary"),
        feature_version: r.get("feature_version"),
        features_used: r.get("features_used"),
        calibration_report_id: r.get("calibration_report_id"),
        trust_assessment_id: r.get("trust_assessment_id"),
        brier_score: r.get("brier_score"),
        log_loss: r.get("log_loss"),
        accuracy: r.get("accuracy"),
        calibration_curve: r.get("calibration_curve"),
        invalidation_conditions: r.get("invalidation_conditions"),
        known_limitations: r.get("known_limitations"),
        ethical_considerations: r.get("ethical_considerations"),
        status: ModelCardStatus::from_str(r.get::<String, _>("status").as_str())
            .unwrap_or(ModelCardStatus::Draft),
        shadow_period_start: r.get("shadow_period_start"),
        shadow_period_end: r.get("shadow_period_end"),
        promotion_eligible: r.get("promotion_eligible"),
        previous_version: r.get("previous_version"),
        created_by: r.get("created_by"),
        approved_by: r.get("approved_by"),
        approved_at: r.get("approved_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_card_status_serialization() {
        assert_eq!(ModelCardStatus::Draft.as_str(), "draft");
        assert_eq!(ModelCardStatus::Shadow.as_str(), "shadow");
        assert_eq!(ModelCardStatus::Active.as_str(), "active");
        assert_eq!(ModelCardStatus::Deprecated.as_str(), "deprecated");
        assert_eq!(ModelCardStatus::RolledBack.as_str(), "rolled_back");

        assert_eq!(
            ModelCardStatus::from_str("draft"),
            Some(ModelCardStatus::Draft)
        );
        assert_eq!(
            ModelCardStatus::from_str("active"),
            Some(ModelCardStatus::Active)
        );
        assert_eq!(ModelCardStatus::from_str("invalid"), None);
    }

    #[test]
    fn test_model_card_status_all_variants() {
        let statuses = vec![
            ModelCardStatus::Draft,
            ModelCardStatus::Shadow,
            ModelCardStatus::Active,
            ModelCardStatus::Deprecated,
            ModelCardStatus::RolledBack,
        ];
        for s in &statuses {
            let str_val = s.as_str();
            let parsed = ModelCardStatus::from_str(str_val);
            assert_eq!(parsed, Some(*s));
        }
    }

    #[test]
    fn test_model_card_input_default_invalidation() {
        let input = ModelCardInput {
            model_version: "test_v1".into(),
            model_type: "classifier".into(),
            model_name: "Test Model".into(),
            description: None,
            intended_use: None,
            out_of_scope: None,
            training_data_summary: None,
            feature_version: None,
            features_used: None,
            invalidation_conditions: None,
            known_limitations: None,
            ethical_considerations: None,
            created_by: None,
        };
        // 验证默认失效条件会在 aggregate 时生成（这里只验证输入结构）
        assert_eq!(input.model_version, "test_v1");
        assert!(input.invalidation_conditions.is_none());
    }
}
