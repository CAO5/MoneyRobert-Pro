//! Backtest Trust Assessment Engine
//! 回测可信等级评估引擎
//!
//! 依据《系统评估与演进规划》第五节"测试与发布门禁"：
//! - 三级可信等级：display_only（仅展示）/ comparable（可比较）/ promotion_eligible（可晋级）
//! - 未达到"可用于比较"以上的回测结果，不允许作为自动晋级实盘的依据
//! - 评估维度：测试覆盖、资金守恒、滑点入账、数据质量、样本量、Walk-forward、概率校准

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// 回测可信等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// 仅展示：回测结果仅供参考，不可用于比较或晋级
    DisplayOnly,
    /// 可比较：回测结果可用于策略比较，但不允许自动晋级实盘
    Comparable,
    /// 可晋级：回测结果通过所有门禁，允许进入影子盘或小资金实盘
    PromotionEligible,
}

impl TrustLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DisplayOnly => "display_only",
            Self::Comparable => "comparable",
            Self::PromotionEligible => "promotion_eligible",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "display_only" => Some(Self::DisplayOnly),
            "comparable" => Some(Self::Comparable),
            "promotion_eligible" => Some(Self::PromotionEligible),
            _ => None,
        }
    }
}

/// 回测可信等级评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustAssessment {
    pub assessment_id: Uuid,
    pub job_id: Uuid,
    pub trust_level: TrustLevel,

    /// 测试覆盖是否通过
    pub test_coverage_passed: bool,
    /// 资金守恒测试是否通过
    pub capital_conservation_passed: bool,
    /// 滑点成本是否已入账
    pub slippage_accounted: bool,
    /// 数据质量等级
    pub data_quality_grade: String,
    /// 样本量是否充足
    pub sample_size_sufficient: bool,
    /// 是否通过 Walk-forward 验证
    pub walk_forward_validated: bool,
    /// 概率校准是否健康
    pub calibration_healthy: bool,

    /// 评估详情
    pub total_trades: i32,
    pub test_pass_rate: f64,
    pub data_coverage_ratio: f64,
    pub issues: serde_json::Value,
    pub recommendations: serde_json::Value,

    /// 是否允许晋级实盘
    pub promotion_eligible: bool,
    /// 晋级阻断项
    pub promotion_blockers: serde_json::Value,

    pub assessed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// 评估输入参数
#[derive(Debug, Clone)]
pub struct TrustAssessmentInput {
    pub job_id: Uuid,
    pub total_trades: i32,
    pub total_slippage_cost: f64,
    pub total_fee: f64,
    /// 测试通过率（0-1）
    pub test_pass_rate: Option<f64>,
    /// 数据覆盖率（0-1）
    pub data_coverage_ratio: Option<f64>,
    /// 数据质量等级
    pub data_quality_grade: Option<String>,
    /// 是否通过 Walk-forward 验证
    pub walk_forward_validated: bool,
    /// 概率校准是否健康
    pub calibration_healthy: bool,
}

impl Default for TrustAssessmentInput {
    fn default() -> Self {
        Self {
            job_id: Uuid::new_v4(),
            total_trades: 0,
            total_slippage_cost: 0.0,
            total_fee: 0.0,
            test_pass_rate: None,
            data_coverage_ratio: None,
            data_quality_grade: None,
            walk_forward_validated: false,
            calibration_healthy: false,
        }
    }
}

/// 评估回测可信等级
///
/// 评估规则：
/// - DisplayOnly（仅展示）：默认等级，任何回测都至少是这个等级
/// - Comparable（可比较）：需满足以下条件：
///   1. 滑点成本已入账（total_slippage_cost > 0 或明确为 0）
///   2. 样本量充足（total_trades >= 30）
///   3. 数据覆盖率 >= 80%
/// - PromotionEligible（可晋级）：需满足 Comparable 的所有条件，外加：
///   1. 测试通过率 >= 90%
///   2. Walk-forward 验证通过
///   3. 概率校准健康
///   4. 数据质量等级 >= good
pub fn assess_trust(input: &TrustAssessmentInput) -> TrustAssessment {
    let now = Utc::now();
    let mut issues: Vec<String> = Vec::new();
    let mut recommendations: Vec<String> = Vec::new();
    let mut blockers: Vec<String> = Vec::new();

    // 1. 滑点成本是否已入账
    // 当 total_trades > 0 但 total_slippage_cost == 0 且 total_fee == 0 时，可能未入账
    let slippage_accounted = input.total_trades == 0 || input.total_slippage_cost > 0.0 || input.total_fee > 0.0;
    if !slippage_accounted {
        issues.push("滑点成本未入账，绩效报告可能不准确".into());
        recommendations.push("在撮合引擎中保存每笔成交的 slippage_cost".into());
        blockers.push("滑点成本未入账".into());
    }

    // 2. 样本量是否充足（至少 30 笔交易）
    let sample_size_sufficient = input.total_trades >= 30;
    if !sample_size_sufficient {
        issues.push(format!(
            "样本量不足：{} 笔交易，建议至少 30 笔",
            input.total_trades
        ));
        recommendations.push("延长回测时间范围或增加交易频率以获取更多样本".into());
        blockers.push("样本量不足".into());
    }

    // 3. 数据覆盖率
    let data_coverage_ratio = input.data_coverage_ratio.unwrap_or(0.0);
    if data_coverage_ratio < 0.80 {
        issues.push(format!(
            "数据覆盖率不足：{:.1}%，建议至少 80%",
            data_coverage_ratio * 100.0
        ));
        recommendations.push("回填历史数据以提高覆盖率".into());
        blockers.push("数据覆盖率不足".into());
    }

    // 4. 数据质量等级
    let data_quality_grade = input
        .data_quality_grade
        .clone()
        .unwrap_or_else(|| "unknown".into());

    // 5. 测试通过率
    let test_pass_rate = input.test_pass_rate.unwrap_or(0.0);
    let test_coverage_passed = test_pass_rate >= 0.9;
    if !test_coverage_passed {
        issues.push(format!(
            "测试通过率不足：{:.1}%，建议至少 90%",
            test_pass_rate * 100.0
        ));
        recommendations.push("修复失败的测试并接入 CI 门禁".into());
        blockers.push("测试通过率不足".into());
    }

    // 6. Walk-forward 验证
    if !input.walk_forward_validated {
        issues.push("未通过 Walk-forward 验证".into());
        recommendations.push("使用 Walk-forward 或 expanding window 验证策略".into());
        blockers.push("未通过 Walk-forward 验证".into());
    }

    // 7. 概率校准
    if !input.calibration_healthy {
        issues.push("概率校准不健康".into());
        recommendations.push("检查 Brier Score 和校准曲线，必要时重新训练模型".into());
        blockers.push("概率校准不健康".into());
    }

    // 资金守恒测试（简化判断：有交易且费用/滑点已记录则视为通过）
    let capital_conservation_passed = input.total_trades == 0 || slippage_accounted;

    // 确定可信等级
    let trust_level = if slippage_accounted
        && sample_size_sufficient
        && data_coverage_ratio >= 0.80
        && test_coverage_passed
        && input.walk_forward_validated
        && input.calibration_healthy
        && (data_quality_grade == "excellent"
            || data_quality_grade == "good")
    {
        TrustLevel::PromotionEligible
    } else if slippage_accounted && sample_size_sufficient && data_coverage_ratio >= 0.80 {
        TrustLevel::Comparable
    } else {
        TrustLevel::DisplayOnly
    };

    let promotion_eligible = trust_level == TrustLevel::PromotionEligible;

    TrustAssessment {
        assessment_id: Uuid::new_v4(),
        job_id: input.job_id,
        trust_level,
        test_coverage_passed,
        capital_conservation_passed,
        slippage_accounted,
        data_quality_grade,
        sample_size_sufficient,
        walk_forward_validated: input.walk_forward_validated,
        calibration_healthy: input.calibration_healthy,
        total_trades: input.total_trades,
        test_pass_rate,
        data_coverage_ratio,
        issues: serde_json::to_value(&issues).unwrap_or(serde_json::json!([])),
        recommendations: serde_json::to_value(&recommendations).unwrap_or(serde_json::json!([])),
        promotion_eligible,
        promotion_blockers: serde_json::to_value(&blockers).unwrap_or(serde_json::json!([])),
        assessed_at: now,
        created_at: now,
    }
}

/// 保存可信等级评估到数据库
pub async fn save_assessment(
    pool: &PgPool,
    assessment: &TrustAssessment,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO backtest_trust_assessments
           (assessment_id, job_id, trust_level,
            test_coverage_passed, capital_conservation_passed, slippage_accounted,
            data_quality_grade, sample_size_sufficient, walk_forward_validated, calibration_healthy,
            total_trades, test_pass_rate, data_coverage_ratio,
            issues, recommendations,
            promotion_eligible, promotion_blockers,
            assessed_at, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
           ON CONFLICT (job_id) DO UPDATE SET
            trust_level = EXCLUDED.trust_level,
            test_coverage_passed = EXCLUDED.test_coverage_passed,
            capital_conservation_passed = EXCLUDED.capital_conservation_passed,
            slippage_accounted = EXCLUDED.slippage_accounted,
            data_quality_grade = EXCLUDED.data_quality_grade,
            sample_size_sufficient = EXCLUDED.sample_size_sufficient,
            walk_forward_validated = EXCLUDED.walk_forward_validated,
            calibration_healthy = EXCLUDED.calibration_healthy,
            total_trades = EXCLUDED.total_trades,
            test_pass_rate = EXCLUDED.test_pass_rate,
            data_coverage_ratio = EXCLUDED.data_coverage_ratio,
            issues = EXCLUDED.issues,
            recommendations = EXCLUDED.recommendations,
            promotion_eligible = EXCLUDED.promotion_eligible,
            promotion_blockers = EXCLUDED.promotion_blockers,
            assessed_at = EXCLUDED.assessed_at"#,
    )
    .bind(assessment.assessment_id)
    .bind(assessment.job_id)
    .bind(assessment.trust_level.as_str())
    .bind(assessment.test_coverage_passed)
    .bind(assessment.capital_conservation_passed)
    .bind(assessment.slippage_accounted)
    .bind(&assessment.data_quality_grade)
    .bind(assessment.sample_size_sufficient)
    .bind(assessment.walk_forward_validated)
    .bind(assessment.calibration_healthy)
    .bind(assessment.total_trades)
    .bind(assessment.test_pass_rate)
    .bind(assessment.data_coverage_ratio)
    .bind(&assessment.issues)
    .bind(&assessment.recommendations)
    .bind(assessment.promotion_eligible)
    .bind(&assessment.promotion_blockers)
    .bind(assessment.assessed_at)
    .bind(assessment.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查询回测可信等级
pub async fn get_assessment(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<Option<TrustAssessment>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT assessment_id, job_id, trust_level,
                  test_coverage_passed, capital_conservation_passed, slippage_accounted,
                  data_quality_grade, sample_size_sufficient, walk_forward_validated, calibration_healthy,
                  total_trades, test_pass_rate, data_coverage_ratio,
                  issues, recommendations,
                  promotion_eligible, promotion_blockers,
                  assessed_at, created_at
           FROM backtest_trust_assessments
           WHERE job_id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| TrustAssessment {
        assessment_id: row.get("assessment_id"),
        job_id: row.get("job_id"),
        trust_level: TrustLevel::from_str(row.get::<String, _>("trust_level").as_str())
            .unwrap_or(TrustLevel::DisplayOnly),
        test_coverage_passed: row.get("test_coverage_passed"),
        capital_conservation_passed: row.get("capital_conservation_passed"),
        slippage_accounted: row.get("slippage_accounted"),
        data_quality_grade: row.get("data_quality_grade"),
        sample_size_sufficient: row.get("sample_size_sufficient"),
        walk_forward_validated: row.get("walk_forward_validated"),
        calibration_healthy: row.get("calibration_healthy"),
        total_trades: row.get("total_trades"),
        test_pass_rate: row.get("test_pass_rate"),
        data_coverage_ratio: row.get("data_coverage_ratio"),
        issues: row.get("issues"),
        recommendations: row.get("recommendations"),
        promotion_eligible: row.get("promotion_eligible"),
        promotion_blockers: row.get("promotion_blockers"),
        assessed_at: row.get("assessed_at"),
        created_at: row.get("created_at"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assess_trust_display_only_for_insufficient_samples() {
        // 样本量不足，应为 DisplayOnly
        let input = TrustAssessmentInput {
            job_id: Uuid::new_v4(),
            total_trades: 5,
            total_slippage_cost: 10.0,
            total_fee: 5.0,
            test_pass_rate: Some(0.95),
            data_coverage_ratio: Some(0.95),
            data_quality_grade: Some("good".into()),
            walk_forward_validated: true,
            calibration_healthy: true,
        };
        let result = assess_trust(&input);
        assert_eq!(result.trust_level, TrustLevel::DisplayOnly);
        assert!(!result.sample_size_sufficient);
        assert!(!result.promotion_eligible);
    }

    #[test]
    fn test_assess_trust_comparable_for_sufficient_samples() {
        // 样本量充足、滑点入账、覆盖率达标，但未通过 Walk-forward，应为 Comparable
        let input = TrustAssessmentInput {
            job_id: Uuid::new_v4(),
            total_trades: 50,
            total_slippage_cost: 100.0,
            total_fee: 50.0,
            test_pass_rate: Some(0.95),
            data_coverage_ratio: Some(0.95),
            data_quality_grade: Some("good".into()),
            walk_forward_validated: false,
            calibration_healthy: false,
        };
        let result = assess_trust(&input);
        assert_eq!(result.trust_level, TrustLevel::Comparable);
        assert!(result.sample_size_sufficient);
        assert!(result.slippage_accounted);
        assert!(!result.promotion_eligible);
    }

    #[test]
    fn test_assess_trust_promotion_eligible_when_all_pass() {
        // 所有门禁通过，应为 PromotionEligible
        let input = TrustAssessmentInput {
            job_id: Uuid::new_v4(),
            total_trades: 100,
            total_slippage_cost: 200.0,
            total_fee: 100.0,
            test_pass_rate: Some(0.95),
            data_coverage_ratio: Some(0.98),
            data_quality_grade: Some("excellent".into()),
            walk_forward_validated: true,
            calibration_healthy: true,
        };
        let result = assess_trust(&input);
        assert_eq!(result.trust_level, TrustLevel::PromotionEligible);
        assert!(result.promotion_eligible);
    }

    #[test]
    fn test_assess_trust_display_only_when_slippage_not_accounted() {
        // 滑点未入账，应为 DisplayOnly
        let input = TrustAssessmentInput {
            job_id: Uuid::new_v4(),
            total_trades: 50,
            total_slippage_cost: 0.0,
            total_fee: 0.0,
            test_pass_rate: Some(0.95),
            data_coverage_ratio: Some(0.95),
            data_quality_grade: Some("good".into()),
            walk_forward_validated: true,
            calibration_healthy: true,
        };
        let result = assess_trust(&input);
        assert_eq!(result.trust_level, TrustLevel::DisplayOnly);
        assert!(!result.slippage_accounted);
    }

    #[test]
    fn test_assess_trust_issues_and_blockers_populated() {
        let input = TrustAssessmentInput {
            job_id: Uuid::new_v4(),
            total_trades: 10,
            total_slippage_cost: 0.0,
            total_fee: 0.0,
            test_pass_rate: Some(0.5),
            data_coverage_ratio: Some(0.5),
            data_quality_grade: Some("poor".into()),
            walk_forward_validated: false,
            calibration_healthy: false,
        };
        let result = assess_trust(&input);
        let issues = result.issues.as_array().unwrap();
        let blockers = result.promotion_blockers.as_array().unwrap();
        assert!(!issues.is_empty(), "应有问题");
        assert!(!blockers.is_empty(), "应有阻断项");
    }

    #[test]
    fn test_trust_level_serialization() {
        assert_eq!(TrustLevel::DisplayOnly.as_str(), "display_only");
        assert_eq!(TrustLevel::Comparable.as_str(), "comparable");
        assert_eq!(TrustLevel::PromotionEligible.as_str(), "promotion_eligible");
        assert_eq!(
            TrustLevel::from_str("comparable"),
            Some(TrustLevel::Comparable)
        );
        assert_eq!(TrustLevel::from_str("invalid"), None);
    }
}
