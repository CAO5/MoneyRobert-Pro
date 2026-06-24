//! Counterfactual Explanation Engine
//! 反事实解释引擎
//!
//! 依据《系统评估与演进规划》第四阶段任务3 与 deep-research-report.md：
//!   "对每笔交易同时回答'为何做''为何错''若不做/早退会怎样'"
//!
//! 对每笔已平仓交易生成多个反事实场景：
//! - no_trade: 若不交易，对比 buy_and_hold 基准
//! - earlier_exit: 若提前退出
//! - later_exit: 若延后退出
//! - opposite_direction: 若反向操作
//! - reduced_size: 若减半仓位

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// 反事实场景类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioType {
    /// 若不交易
    NoTrade,
    /// 若提前退出
    EarlierExit,
    /// 若延后退出
    LaterExit,
    /// 若反向操作
    OppositeDirection,
    /// 若减半仓位
    ReducedSize,
}

impl ScenarioType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoTrade => "no_trade",
            Self::EarlierExit => "earlier_exit",
            Self::LaterExit => "later_exit",
            Self::OppositeDirection => "opposite_direction",
            Self::ReducedSize => "reduced_size",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "no_trade" => Some(Self::NoTrade),
            "earlier_exit" => Some(Self::EarlierExit),
            "later_exit" => Some(Self::LaterExit),
            "opposite_direction" => Some(Self::OppositeDirection),
            "reduced_size" => Some(Self::ReducedSize),
            _ => None,
        }
    }

    /// 返回所有场景类型
    pub fn all() -> Vec<Self> {
        vec![
            Self::NoTrade,
            Self::EarlierExit,
            Self::LaterExit,
            Self::OppositeDirection,
            Self::ReducedSize,
        ]
    }
}

/// 反事实解释
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualExplanation {
    pub explanation_id: Uuid,
    pub attribution_id: Option<Uuid>,
    pub decision_card_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub user_id: Option<i64>,
    pub symbol: String,

    pub scenario_type: ScenarioType,
    pub scenario_description: Option<String>,

    pub counterfactual_pnl: Option<f64>,
    pub actual_pnl: f64,
    pub pnl_delta: Option<f64>,
    pub counterfactual_return: Option<f64>,

    pub explanation: String,
    pub key_drivers: serde_json::Value,
    pub what_if_inputs: Option<serde_json::Value>,
    pub confidence: f64,

    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 反事实输入参数
#[derive(Debug, Clone)]
pub struct CounterfactualInput {
    pub attribution_id: Option<Uuid>,
    pub decision_card_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub user_id: Option<i64>,
    pub symbol: String,
    pub direction: String,
    pub actual_pnl: f64,
    pub gross_pnl: f64,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub holding_period_sec: Option<i32>,
    pub fee_cost: f64,
    pub slippage_cost: f64,
    pub funding_cost: f64,
    pub impact_cost: f64,
    pub benchmark_return: Option<f64>,
    pub market_regime: Option<String>,
    pub signal_confidence: Option<f64>,
}

/// 为单笔交易生成所有反事实场景
///
/// 对每笔交易生成 5 种反事实场景，回答"若不做/早退/晚退/反向/减仓会怎样"
pub fn generate_counterfactuals(input: &CounterfactualInput) -> Vec<CounterfactualExplanation> {
    let now = Utc::now();
    let mut results = Vec::new();

    // 1. no_trade: 若不交易
    let no_trade_pnl = input.benchmark_return.unwrap_or(0.0);
    let no_trade_delta = no_trade_pnl - input.actual_pnl;
    results.push(CounterfactualExplanation {
        explanation_id: Uuid::new_v4(),
        attribution_id: input.attribution_id,
        decision_card_id: input.decision_card_id,
        job_id: input.job_id,
        user_id: input.user_id,
        symbol: input.symbol.clone(),
        scenario_type: ScenarioType::NoTrade,
        scenario_description: Some("若不进行此交易，资金将保持原状".into()),
        counterfactual_pnl: Some(no_trade_pnl),
        actual_pnl: input.actual_pnl,
        pnl_delta: Some(no_trade_delta),
        counterfactual_return: Some(no_trade_pnl),
        explanation: generate_no_trade_explanation(input, no_trade_pnl),
        key_drivers: serde_json::json!([
            {"driver": "benchmark_return", "value": no_trade_pnl},
            {"driver": "actual_pnl", "value": input.actual_pnl}
        ]),
        what_if_inputs: Some(serde_json::json!({"action": "hold", "benchmark_return": no_trade_pnl})),
        confidence: 0.9,
        evidence: serde_json::json!({
            "benchmark_return": input.benchmark_return,
            "actual_pnl": input.actual_pnl,
            "delta": no_trade_delta
        }),
        created_at: now,
    });

    // 2. earlier_exit: 若提前退出（假设提前一半时间退出）
    let earlier_pnl = input.gross_pnl * 0.5
        - input.fee_cost * 0.5
        - input.slippage_cost * 0.5
        - input.funding_cost * 0.25
        - input.impact_cost * 0.5;
    let earlier_delta = earlier_pnl - input.actual_pnl;
    results.push(CounterfactualExplanation {
        explanation_id: Uuid::new_v4(),
        attribution_id: input.attribution_id,
        decision_card_id: input.decision_card_id,
        job_id: input.job_id,
        user_id: input.user_id,
        symbol: input.symbol.clone(),
        scenario_type: ScenarioType::EarlierExit,
        scenario_description: Some("若提前一半时间退出".into()),
        counterfactual_pnl: Some(earlier_pnl),
        actual_pnl: input.actual_pnl,
        pnl_delta: Some(earlier_delta),
        counterfactual_return: Some(earlier_pnl),
        explanation: generate_earlier_exit_explanation(input, earlier_pnl),
        key_drivers: serde_json::json!([
            {"driver": "gross_pnl_halved", "value": input.gross_pnl * 0.5},
            {"driver": "reduced_costs", "value": input.fee_cost * 0.5 + input.slippage_cost * 0.5}
        ]),
        what_if_inputs: Some(serde_json::json!({
            "exit_time_offset_sec": input.holding_period_sec.map(|h| -h / 2),
            "cost_reduction_factor": 0.5
        })),
        confidence: 0.7,
        evidence: serde_json::json!({
            "original_gross_pnl": input.gross_pnl,
            "counterfactual_gross_pnl": input.gross_pnl * 0.5,
            "cost_savings": input.fee_cost * 0.5 + input.slippage_cost * 0.5
        }),
        created_at: now,
    });

    // 3. later_exit: 若延后退出（假设延后一半时间退出）
    let later_pnl = input.gross_pnl * 1.5
        - input.fee_cost
        - input.slippage_cost
        - input.funding_cost * 1.5
        - input.impact_cost * 1.2;
    let later_delta = later_pnl - input.actual_pnl;
    results.push(CounterfactualExplanation {
        explanation_id: Uuid::new_v4(),
        attribution_id: input.attribution_id,
        decision_card_id: input.decision_card_id,
        job_id: input.job_id,
        user_id: input.user_id,
        symbol: input.symbol.clone(),
        scenario_type: ScenarioType::LaterExit,
        scenario_description: Some("若延后一半时间退出".into()),
        counterfactual_pnl: Some(later_pnl),
        actual_pnl: input.actual_pnl,
        pnl_delta: Some(later_delta),
        counterfactual_return: Some(later_pnl),
        explanation: generate_later_exit_explanation(input, later_pnl),
        key_drivers: serde_json::json!([
            {"driver": "gross_pnl_increased", "value": input.gross_pnl * 1.5},
            {"driver": "increased_funding_cost", "value": input.funding_cost * 1.5}
        ]),
        what_if_inputs: Some(serde_json::json!({
            "exit_time_offset_sec": input.holding_period_sec.map(|h| h / 2),
            "funding_cost_multiplier": 1.5
        })),
        confidence: 0.6,
        evidence: serde_json::json!({
            "original_gross_pnl": input.gross_pnl,
            "counterfactual_gross_pnl": input.gross_pnl * 1.5,
            "additional_funding_cost": input.funding_cost * 0.5
        }),
        created_at: now,
    });

    // 4. opposite_direction: 若反向操作
    let opposite_gross = -input.gross_pnl;
    let opposite_pnl = opposite_gross
        - input.fee_cost
        - input.slippage_cost
        - input.funding_cost
        - input.impact_cost;
    let opposite_delta = opposite_pnl - input.actual_pnl;
    results.push(CounterfactualExplanation {
        explanation_id: Uuid::new_v4(),
        attribution_id: input.attribution_id,
        decision_card_id: input.decision_card_id,
        job_id: input.job_id,
        user_id: input.user_id,
        symbol: input.symbol.clone(),
        scenario_type: ScenarioType::OppositeDirection,
        scenario_description: Some(format!("若反向操作（{} -> {}）", input.direction,
            if input.direction == "long" { "short" } else { "long" })),
        counterfactual_pnl: Some(opposite_pnl),
        actual_pnl: input.actual_pnl,
        pnl_delta: Some(opposite_delta),
        counterfactual_return: Some(opposite_pnl),
        explanation: generate_opposite_direction_explanation(input, opposite_pnl),
        key_drivers: serde_json::json!([
            {"driver": "reversed_gross_pnl", "value": opposite_gross},
            {"driver": "original_direction", "value": input.direction}
        ]),
        what_if_inputs: Some(serde_json::json!({
            "direction": if input.direction == "long" { "short" } else { "long" },
            "gross_pnl": opposite_gross
        })),
        confidence: 0.5,
        evidence: serde_json::json!({
            "original_gross_pnl": input.gross_pnl,
            "counterfactual_gross_pnl": opposite_gross,
            "direction_reversed": true
        }),
        created_at: now,
    });

    // 5. reduced_size: 若减半仓位
    let reduced_pnl = input.gross_pnl * 0.5
        - input.fee_cost * 0.5
        - input.slippage_cost * 0.5
        - input.funding_cost * 0.5
        - input.impact_cost * 0.25;
    let reduced_delta = reduced_pnl - input.actual_pnl;
    results.push(CounterfactualExplanation {
        explanation_id: Uuid::new_v4(),
        attribution_id: input.attribution_id,
        decision_card_id: input.decision_card_id,
        job_id: input.job_id,
        user_id: input.user_id,
        symbol: input.symbol.clone(),
        scenario_type: ScenarioType::ReducedSize,
        scenario_description: Some("若减半仓位".into()),
        counterfactual_pnl: Some(reduced_pnl),
        actual_pnl: input.actual_pnl,
        pnl_delta: Some(reduced_delta),
        counterfactual_return: Some(reduced_pnl),
        explanation: generate_reduced_size_explanation(input, reduced_pnl),
        key_drivers: serde_json::json!([
            {"driver": "halved_gross_pnl", "value": input.gross_pnl * 0.5},
            {"driver": "reduced_impact_cost", "value": input.impact_cost * 0.25}
        ]),
        what_if_inputs: Some(serde_json::json!({
            "position_size_factor": 0.5,
            "impact_cost_factor": 0.25
        })),
        confidence: 0.8,
        evidence: serde_json::json!({
            "original_gross_pnl": input.gross_pnl,
            "counterfactual_gross_pnl": input.gross_pnl * 0.5,
            "impact_cost_reduction": input.impact_cost * 0.75
        }),
        created_at: now,
    });

    results
}

// ============================================================================
// 自然语言解释生成
// ============================================================================

fn generate_no_trade_explanation(input: &CounterfactualInput, cf_pnl: f64) -> String {
    let better = cf_pnl > input.actual_pnl;
    if better {
        format!(
            "若不进行此交易，收益为 {:.2}（基准收益），优于实际盈亏 {:.2}。\
             这表明该交易未跑赢基准，可能由于信号质量不足或成本过高。",
            cf_pnl, input.actual_pnl
        )
    } else {
        format!(
            "若不进行此交易，收益为 {:.2}（基准收益），低于实际盈亏 {:.2}。\
             这表明该交易跑赢了基准，产生了正向 alpha。",
            cf_pnl, input.actual_pnl
        )
    }
}

fn generate_earlier_exit_explanation(input: &CounterfactualInput, cf_pnl: f64) -> String {
    let better = cf_pnl > input.actual_pnl;
    if better {
        format!(
            "若提前一半时间退出，预计盈亏为 {:.2}，优于实际 {:.2}。\
             这表明持仓时间过长导致利润回吐，建议设置更紧的止盈或动态退出条件。",
            cf_pnl, input.actual_pnl
        )
    } else {
        format!(
            "若提前一半时间退出，预计盈亏为 {:.2}，低于实际 {:.2}。\
             这表明持仓时间充足，趋势延续有利，退出时机合理。",
            cf_pnl, input.actual_pnl
        )
    }
}

fn generate_later_exit_explanation(input: &CounterfactualInput, cf_pnl: f64) -> String {
    let better = cf_pnl > input.actual_pnl;
    if better {
        format!(
            "若延后一半时间退出，预计盈亏为 {:.2}，优于实际 {:.2}。\
             这表明过早退出错失了后续行情，但需注意延后退出会增加资金费率成本。",
            cf_pnl, input.actual_pnl
        )
    } else {
        format!(
            "若延后一半时间退出，预计盈亏为 {:.2}，低于实际 {:.2}。\
             这表明及时退出避免了行情反转，退出时机恰当。",
            cf_pnl, input.actual_pnl
        )
    }
}

fn generate_opposite_direction_explanation(input: &CounterfactualInput, cf_pnl: f64) -> String {
    let better = cf_pnl > input.actual_pnl;
    let opposite_dir = if input.direction == "long" { "做空" } else { "做多" };
    if better {
        format!(
            "若反向操作（{}），预计盈亏为 {:.2}，优于实际 {:.2}。\
             这表明方向判断错误，信号可能存在系统性偏差，需检查模型校准状态。",
            opposite_dir, cf_pnl, input.actual_pnl
        )
    } else {
        format!(
            "若反向操作（{}），预计盈亏为 {:.2}，低于实际 {:.2}。\
             这表明方向判断正确，模型信号有效。",
            opposite_dir, cf_pnl, input.actual_pnl
        )
    }
}

fn generate_reduced_size_explanation(input: &CounterfactualInput, cf_pnl: f64) -> String {
    let better = cf_pnl > input.actual_pnl;
    if better {
        format!(
            "若减半仓位，预计盈亏为 {:.2}，优于实际 {:.2}。\
             这表明仓位过大导致冲击成本过高，建议降低单笔仓位比例。",
            cf_pnl, input.actual_pnl
        )
    } else {
        format!(
            "若减半仓位，预计盈亏为 {:.2}，低于实际 {:.2}。\
             这表明当前仓位合理，充分利用了行情机会。",
            cf_pnl, input.actual_pnl
        )
    }
}

// ============================================================================
// 数据库操作
// ============================================================================

/// 保存反事实解释到数据库
pub async fn save_explanation(
    pool: &PgPool,
    cf: &CounterfactualExplanation,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO counterfactual_explanations
           (explanation_id, attribution_id, decision_card_id, job_id, user_id, symbol,
            scenario_type, scenario_description,
            counterfactual_pnl, actual_pnl, pnl_delta, counterfactual_return,
            explanation, key_drivers, what_if_inputs, confidence,
            evidence, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   $13, $14, $15, $16, $17, $18)"#,
    )
    .bind(cf.explanation_id)
    .bind(cf.attribution_id)
    .bind(cf.decision_card_id)
    .bind(cf.job_id)
    .bind(cf.user_id)
    .bind(&cf.symbol)
    .bind(cf.scenario_type.as_str())
    .bind(&cf.scenario_description)
    .bind(cf.counterfactual_pnl)
    .bind(cf.actual_pnl)
    .bind(cf.pnl_delta)
    .bind(cf.counterfactual_return)
    .bind(&cf.explanation)
    .bind(&cf.key_drivers)
    .bind(&cf.what_if_inputs)
    .bind(cf.confidence)
    .bind(&cf.evidence)
    .bind(cf.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 批量保存反事实解释
pub async fn save_explanations(
    pool: &PgPool,
    explanations: &[CounterfactualExplanation],
) -> Result<(), sqlx::Error> {
    for cf in explanations {
        save_explanation(pool, cf).await?;
    }
    Ok(())
}

/// 查询某笔交易的反事实场景
pub async fn list_by_attribution(
    pool: &PgPool,
    attribution_id: Uuid,
) -> Result<Vec<CounterfactualExplanation>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT explanation_id, attribution_id, decision_card_id, job_id, user_id, symbol,
                  scenario_type, scenario_description,
                  counterfactual_pnl, actual_pnl, pnl_delta, counterfactual_return,
                  explanation, key_drivers, what_if_inputs, confidence,
                  evidence, created_at
           FROM counterfactual_explanations
           WHERE attribution_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(attribution_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| row_to_explanation(r)).collect())
}

/// 查询某 job 下所有反事实场景
pub async fn list_by_job(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<Vec<CounterfactualExplanation>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT explanation_id, attribution_id, decision_card_id, job_id, user_id, symbol,
                  scenario_type, scenario_description,
                  counterfactual_pnl, actual_pnl, pnl_delta, counterfactual_return,
                  explanation, key_drivers, what_if_inputs, confidence,
                  evidence, created_at
           FROM counterfactual_explanations
           WHERE job_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| row_to_explanation(r)).collect())
}

/// 从数据库行构建 CounterfactualExplanation
fn row_to_explanation(r: &sqlx::postgres::PgRow) -> CounterfactualExplanation {
    CounterfactualExplanation {
        explanation_id: r.get("explanation_id"),
        attribution_id: r.get("attribution_id"),
        decision_card_id: r.get("decision_card_id"),
        job_id: r.get("job_id"),
        user_id: r.get("user_id"),
        symbol: r.get("symbol"),
        scenario_type: ScenarioType::from_str(r.get::<String, _>("scenario_type").as_str())
            .unwrap_or(ScenarioType::NoTrade),
        scenario_description: r.get("scenario_description"),
        counterfactual_pnl: r.get("counterfactual_pnl"),
        actual_pnl: r.get("actual_pnl"),
        pnl_delta: r.get("pnl_delta"),
        counterfactual_return: r.get("counterfactual_return"),
        explanation: r.get("explanation"),
        key_drivers: r.get("key_drivers"),
        what_if_inputs: r.get("what_if_inputs"),
        confidence: r.get("confidence"),
        evidence: r.get("evidence"),
        created_at: r.get("created_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input() -> CounterfactualInput {
        CounterfactualInput {
            attribution_id: Some(Uuid::new_v4()),
            decision_card_id: None,
            job_id: Some(Uuid::new_v4()),
            user_id: Some(1),
            symbol: "BTC-USDT".into(),
            direction: "long".into(),
            actual_pnl: 100.0,
            gross_pnl: 120.0,
            entry_time: Utc::now(),
            exit_time: Some(Utc::now()),
            holding_period_sec: Some(3600),
            fee_cost: 5.0,
            slippage_cost: 3.0,
            funding_cost: 2.0,
            impact_cost: 10.0,
            benchmark_return: Some(0.05),
            market_regime: Some("trending_bull".into()),
            signal_confidence: Some(0.7),
        }
    }

    #[test]
    fn test_scenario_type_serialization() {
        assert_eq!(ScenarioType::NoTrade.as_str(), "no_trade");
        assert_eq!(ScenarioType::EarlierExit.as_str(), "earlier_exit");
        assert_eq!(ScenarioType::LaterExit.as_str(), "later_exit");
        assert_eq!(ScenarioType::OppositeDirection.as_str(), "opposite_direction");
        assert_eq!(ScenarioType::ReducedSize.as_str(), "reduced_size");

        assert_eq!(
            ScenarioType::from_str("no_trade"),
            Some(ScenarioType::NoTrade)
        );
        assert_eq!(ScenarioType::from_str("invalid"), None);
    }

    #[test]
    fn test_scenario_type_all() {
        let all = ScenarioType::all();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_generate_counterfactuals_count() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        assert_eq!(results.len(), 5, "应生成 5 个反事实场景");
    }

    #[test]
    fn test_generate_counterfactuals_no_trade() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        let no_trade = results.iter().find(|r| r.scenario_type == ScenarioType::NoTrade).unwrap();
        assert!(no_trade.counterfactual_pnl.is_some());
        assert_eq!(no_trade.actual_pnl, 100.0);
        assert!(no_trade.explanation.contains("若不进行此交易"));
    }

    #[test]
    fn test_generate_counterfactuals_earlier_exit() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        let earlier = results.iter().find(|r| r.scenario_type == ScenarioType::EarlierExit).unwrap();
        assert!(earlier.counterfactual_pnl.is_some());
        assert!(earlier.explanation.contains("提前"));
    }

    #[test]
    fn test_generate_counterfactuals_opposite_direction() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        let opposite = results.iter().find(|r| r.scenario_type == ScenarioType::OppositeDirection).unwrap();
        assert!(opposite.explanation.contains("反向"));
        // 反向操作的 gross_pnl 应为 -120
        let cf_pnl = opposite.counterfactual_pnl.unwrap();
        assert!(cf_pnl < 0.0, "反向操作盈亏应为负");
    }

    #[test]
    fn test_generate_counterfactuals_reduced_size() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        let reduced = results.iter().find(|r| r.scenario_type == ScenarioType::ReducedSize).unwrap();
        assert!(reduced.explanation.contains("减半"));
        // 减半仓位的盈亏应小于实际盈亏（因为是盈利交易）
        let cf_pnl = reduced.counterfactual_pnl.unwrap();
        assert!(cf_pnl < input.actual_pnl, "盈利交易减半仓位应盈亏更小");
    }

    #[test]
    fn test_generate_counterfactuals_pnl_delta() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        for cf in &results {
            if let (Some(cf_pnl), Some(delta)) = (cf.counterfactual_pnl, cf.pnl_delta) {
                let expected_delta = cf_pnl - cf.actual_pnl;
                assert!((delta - expected_delta).abs() < 1e-9, "pnl_delta 计算错误");
            }
        }
    }

    #[test]
    fn test_generate_counterfactuals_explanation_not_empty() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        for cf in &results {
            assert!(!cf.explanation.is_empty(), "解释不应为空");
            assert!(cf.explanation.len() > 10, "解释应足够详细");
        }
    }

    #[test]
    fn test_generate_counterfactuals_key_drivers() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        for cf in &results {
            let drivers = cf.key_drivers.as_array();
            assert!(drivers.is_some(), "key_drivers 应为数组");
            assert!(!drivers.unwrap().is_empty(), "key_drivers 不应为空");
        }
    }

    #[test]
    fn test_generate_counterfactuals_confidence_range() {
        let input = make_input();
        let results = generate_counterfactuals(&input);
        for cf in &results {
            assert!(cf.confidence > 0.0 && cf.confidence <= 1.0, "置信度应在 (0, 1] 范围");
        }
    }

    #[test]
    fn test_generate_counterfactuals_loss_scenario() {
        // 亏损交易场景
        let mut input = make_input();
        input.actual_pnl = -50.0;
        input.gross_pnl = -30.0;
        let results = generate_counterfactuals(&input);

        // no_trade 场景：不交易应该更好（基准收益 0.05 > -50）
        let no_trade = results.iter().find(|r| r.scenario_type == ScenarioType::NoTrade).unwrap();
        assert!(no_trade.pnl_delta.unwrap() > 0.0, "亏损交易不交易应更好");

        // reduced_size 场景：减半仓位亏损应更小
        let reduced = results.iter().find(|r| r.scenario_type == ScenarioType::ReducedSize).unwrap();
        let cf_pnl = reduced.counterfactual_pnl.unwrap();
        assert!(cf_pnl > input.actual_pnl, "亏损交易减半仓位应亏损更少");
    }
}
