//! Post-Trade Attribution Engine
//! 交易后归因分析引擎
//!
//! 依据《系统评估与演进规划》第四阶段任务4：
//!   "交易后自动归因和策略失效提醒"
//!
//! 对每笔已平仓交易做归因分析：
//! - 盈亏来源分解（毛盈亏 → 手续费/滑点/资金费率/冲击 → 净盈亏）
//! - 信号质量评估（校准概率 vs 实际结果）
//! - 基准对比（同期 Buy and Hold 的 alpha）
//! - 归因标签（市场状态、退出原因、信号来源）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 交易归因记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TradeAttribution {
    pub attribution_id: Uuid,
    pub job_id: Option<Uuid>,
    pub user_id: Option<i64>,
    pub symbol: String,
    pub order_id: Option<Uuid>,
    pub fill_id: Option<Uuid>,
    pub decision_card_id: Option<Uuid>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub holding_period_sec: Option<i32>,
    pub gross_pnl: f64,
    pub fee_cost: f64,
    pub slippage_cost: f64,
    pub funding_cost: f64,
    pub impact_cost: f64,
    pub net_pnl: f64,
    pub direction: String,
    pub market_regime: Option<String>,
    pub exit_regime: Option<String>,
    pub signal_source: Option<String>,
    pub signal_confidence: Option<f64>,
    pub calibrated_probability: Option<f64>,
    pub win_loss: Option<String>,
    pub exit_reason: Option<String>,
    pub attribution_tags: serde_json::Value,
    pub benchmark_return: Option<f64>,
    pub alpha: Option<f64>,
    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 归因输入参数
#[derive(Debug, Clone)]
pub struct AttributionInput {
    pub job_id: Option<Uuid>,
    pub user_id: Option<i64>,
    pub symbol: String,
    pub order_id: Option<Uuid>,
    pub fill_id: Option<Uuid>,
    pub decision_card_id: Option<Uuid>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub direction: String,
    pub gross_pnl: f64,
    pub fee_cost: f64,
    pub slippage_cost: f64,
    pub funding_cost: f64,
    pub impact_cost: f64,
    pub market_regime: Option<String>,
    pub exit_regime: Option<String>,
    pub signal_source: Option<String>,
    pub signal_confidence: Option<f64>,
    pub calibrated_probability: Option<f64>,
    pub exit_reason: Option<String>,
    pub benchmark_return: Option<f64>,
}

/// 执行归因分析
///
/// 输入交易数据，输出完整的归因记录
pub fn analyze_attribution(input: &AttributionInput) -> TradeAttribution {
    let net_pnl = input.gross_pnl
        - input.fee_cost
        - input.slippage_cost
        - input.funding_cost
        - input.impact_cost;

    let holding_period_sec = input
        .exit_time
        .map(|exit| (exit - input.entry_time).num_seconds() as i32);

    let win_loss = if net_pnl > 0.0 {
        Some("win".to_string())
    } else if net_pnl < 0.0 {
        Some("loss".to_string())
    } else {
        Some("breakeven".to_string())
    };

    let alpha = input.benchmark_return.map(|bm| {
        let signed_bm = if input.direction == "short" { -bm } else { bm };
        net_pnl - signed_bm
    });

    // 生成归因标签
    let mut tags: Vec<String> = Vec::new();

    // 成本占比标签
    let total_cost = input.fee_cost + input.slippage_cost + input.funding_cost + input.impact_cost;
    if total_cost > 0.0 && input.gross_pnl.abs() > 0.0 {
        let cost_ratio = total_cost / input.gross_pnl.abs();
        if cost_ratio > 0.5 {
            tags.push("cost_dominant".into());
        } else if cost_ratio > 0.2 {
            tags.push("cost_significant".into());
        }
    }

    // 信号质量标签
    if let Some(cal_p) = input.calibrated_probability {
        let was_correct = (net_pnl > 0.0 && input.direction == "long")
            || (net_pnl > 0.0 && input.direction == "short");
        if was_correct && cal_p < 0.5 {
            tags.push("low_confidence_winner".into());
        }
        if !was_correct && cal_p > 0.7 {
            tags.push("high_confidence_loser".into());
        }
    }

    // Alpha 标签
    if let Some(a) = alpha {
        if a > 0.0 {
            tags.push("positive_alpha".into());
        } else {
            tags.push("negative_alpha".into());
        }
    }

    // 持仓时间标签
    if let Some(secs) = holding_period_sec {
        if secs < 300 {
            tags.push("scalping".into());
        } else if secs < 3600 {
            tags.push("intraday".into());
        } else if secs < 86400 {
            tags.push("swing".into());
        } else {
            tags.push("position".into());
        }
    }

    // 市场状态标签
    if let Some(regime) = &input.market_regime {
        tags.push(format!("regime_{}", regime.to_lowercase()));
    }

    // 退出原因标签
    if let Some(reason) = &input.exit_reason {
        tags.push(format!("exit_{}", reason));
    }

    // 构建证据
    let evidence = serde_json::json!({
        "gross_pnl": input.gross_pnl,
        "total_cost": total_cost,
        "cost_breakdown": {
            "fee": input.fee_cost,
            "slippage": input.slippage_cost,
            "funding": input.funding_cost,
            "impact": input.impact_cost,
        },
        "cost_ratio": if input.gross_pnl.abs() > 0.0 {
            Some(total_cost / input.gross_pnl.abs())
        } else {
            None
        },
        "signal_confidence": input.signal_confidence,
        "calibrated_probability": input.calibrated_probability,
        "benchmark_return": input.benchmark_return,
        "alpha": alpha,
    });

    TradeAttribution {
        attribution_id: Uuid::new_v4(),
        job_id: input.job_id,
        user_id: input.user_id,
        symbol: input.symbol.clone(),
        order_id: input.order_id,
        fill_id: input.fill_id,
        decision_card_id: input.decision_card_id,
        entry_time: input.entry_time,
        exit_time: input.exit_time,
        holding_period_sec,
        gross_pnl: input.gross_pnl,
        fee_cost: input.fee_cost,
        slippage_cost: input.slippage_cost,
        funding_cost: input.funding_cost,
        impact_cost: input.impact_cost,
        net_pnl,
        direction: input.direction.clone(),
        market_regime: input.market_regime.clone(),
        exit_regime: input.exit_regime.clone(),
        signal_source: input.signal_source.clone(),
        signal_confidence: input.signal_confidence,
        calibrated_probability: input.calibrated_probability,
        win_loss,
        exit_reason: input.exit_reason.clone(),
        attribution_tags: serde_json::to_value(&tags).unwrap_or(serde_json::json!([])),
        benchmark_return: input.benchmark_return,
        alpha,
        evidence,
        created_at: Utc::now(),
    }
}

/// 保存归因记录到数据库
pub async fn save_attribution(
    pool: &sqlx::PgPool,
    attr: &TradeAttribution,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO trade_attributions
           (attribution_id, job_id, user_id, symbol, order_id, fill_id, decision_card_id,
            entry_time, exit_time, holding_period_sec,
            gross_pnl, fee_cost, slippage_cost, funding_cost, impact_cost, net_pnl,
            direction, market_regime, exit_regime,
            signal_source, signal_confidence, calibrated_probability,
            win_loss, exit_reason, attribution_tags,
            benchmark_return, alpha, evidence, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                   $11, $12, $13, $14, $15, $16,
                   $17, $18, $19, $20, $21, $22,
                   $23, $24, $25, $26, $27, $28, $29)"#,
    )
    .bind(attr.attribution_id)
    .bind(attr.job_id)
    .bind(attr.user_id)
    .bind(&attr.symbol)
    .bind(attr.order_id)
    .bind(attr.fill_id)
    .bind(attr.decision_card_id)
    .bind(attr.entry_time)
    .bind(attr.exit_time)
    .bind(attr.holding_period_sec)
    .bind(attr.gross_pnl)
    .bind(attr.fee_cost)
    .bind(attr.slippage_cost)
    .bind(attr.funding_cost)
    .bind(attr.impact_cost)
    .bind(attr.net_pnl)
    .bind(&attr.direction)
    .bind(&attr.market_regime)
    .bind(&attr.exit_regime)
    .bind(&attr.signal_source)
    .bind(attr.signal_confidence)
    .bind(attr.calibrated_probability)
    .bind(&attr.win_loss)
    .bind(&attr.exit_reason)
    .bind(&attr.attribution_tags)
    .bind(attr.benchmark_return)
    .bind(attr.alpha)
    .bind(&attr.evidence)
    .bind(attr.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查询归因记录
pub async fn list_attributions(
    pool: &sqlx::PgPool,
    job_id: Option<Uuid>,
    user_id: Option<i64>,
    symbol: Option<&str>,
    limit: i64,
) -> Result<Vec<TradeAttribution>, sqlx::Error> {
    let mut sql = String::from("SELECT * FROM trade_attributions WHERE 1=1");
    let mut binds: Vec<String> = Vec::new();

    if job_id.is_some() {
        sql.push_str(" AND job_id = $");
        sql.push_str(&(binds.len() + 1).to_string());
        binds.push(job_id.unwrap().to_string());
    }
    if user_id.is_some() {
        sql.push_str(" AND user_id = $");
        sql.push_str(&(binds.len() + 1).to_string());
        binds.push(user_id.unwrap().to_string());
    }
    if symbol.is_some() {
        sql.push_str(" AND symbol = $");
        sql.push_str(&(binds.len() + 1).to_string());
        binds.push(symbol.unwrap().to_string());
    }

    sql.push_str(" ORDER BY entry_time DESC LIMIT $");
    sql.push_str(&(binds.len() + 1).to_string());

    let mut query = sqlx::query_as::<_, TradeAttribution>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);

    query.fetch_all(pool).await
}

/// 归因汇总统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionSummary {
    pub total_trades: i32,
    pub win_count: i32,
    pub loss_count: i32,
    pub breakeven_count: i32,
    pub win_rate: f64,
    pub total_gross_pnl: f64,
    pub total_net_pnl: f64,
    pub total_fee: f64,
    pub total_slippage: f64,
    pub total_funding: f64,
    pub total_impact: f64,
    pub avg_alpha: f64,
    pub cost_ratio: f64,
    pub by_regime: serde_json::Value,
    pub by_exit_reason: serde_json::Value,
    pub by_signal_source: serde_json::Value,
}

/// 计算归因汇总
pub fn summarize_attributions(attributions: &[TradeAttribution]) -> AttributionSummary {
    let total_trades = attributions.len() as i32;
    if total_trades == 0 {
        return AttributionSummary {
            total_trades: 0,
            win_count: 0,
            loss_count: 0,
            breakeven_count: 0,
            win_rate: 0.0,
            total_gross_pnl: 0.0,
            total_net_pnl: 0.0,
            total_fee: 0.0,
            total_slippage: 0.0,
            total_funding: 0.0,
            total_impact: 0.0,
            avg_alpha: 0.0,
            cost_ratio: 0.0,
            by_regime: serde_json::json!({}),
            by_exit_reason: serde_json::json!({}),
            by_signal_source: serde_json::json!({}),
        };
    }

    let win_count = attributions
        .iter()
        .filter(|a| a.win_loss.as_deref() == Some("win"))
        .count() as i32;
    let loss_count = attributions
        .iter()
        .filter(|a| a.win_loss.as_deref() == Some("loss"))
        .count() as i32;
    let breakeven_count = attributions
        .iter()
        .filter(|a| a.win_loss.as_deref() == Some("breakeven"))
        .count() as i32;

    let total_gross_pnl: f64 = attributions.iter().map(|a| a.gross_pnl).sum();
    let total_net_pnl: f64 = attributions.iter().map(|a| a.net_pnl).sum();
    let total_fee: f64 = attributions.iter().map(|a| a.fee_cost).sum();
    let total_slippage: f64 = attributions.iter().map(|a| a.slippage_cost).sum();
    let total_funding: f64 = attributions.iter().map(|a| a.funding_cost).sum();
    let total_impact: f64 = attributions.iter().map(|a| a.impact_cost).sum();

    let alphas: Vec<f64> = attributions
        .iter()
        .filter_map(|a| a.alpha)
        .collect();
    let avg_alpha = if alphas.is_empty() {
        0.0
    } else {
        alphas.iter().sum::<f64>() / alphas.len() as f64
    };

    let total_cost = total_fee + total_slippage + total_funding + total_impact;
    let cost_ratio = if total_gross_pnl.abs() > 0.0 {
        total_cost / total_gross_pnl.abs()
    } else {
        0.0
    };

    // 按市场状态分组
    let mut regime_map: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for attr in attributions {
        let regime = attr.market_regime.clone().unwrap_or_else(|| "unknown".into());
        regime_map.entry(regime).or_default().push(attr.net_pnl);
    }
    let by_regime: serde_json::Value = serde_json::to_value(
        regime_map
            .iter()
            .map(|(k, v)| {
                let sum: f64 = v.iter().sum();
                let count = v.len();
                let avg = sum / count as f64;
                serde_json::json!({"count": count, "total_pnl": sum, "avg_pnl": avg})
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or(serde_json::json!([]));

    // 按退出原因分组
    let mut exit_map: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for attr in attributions {
        let reason = attr.exit_reason.clone().unwrap_or_else(|| "unknown".into());
        exit_map.entry(reason).or_default().push(attr.net_pnl);
    }
    let by_exit_reason: serde_json::Value = serde_json::to_value(
        exit_map
            .iter()
            .map(|(k, v)| {
                let sum: f64 = v.iter().sum();
                let count = v.len();
                serde_json::json!({"reason": k, "count": count, "total_pnl": sum})
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or(serde_json::json!([]));

    // 按信号来源分组
    let mut source_map: std::collections::HashMap<String, Vec<f64>> = std::collections::HashMap::new();
    for attr in attributions {
        let source = attr.signal_source.clone().unwrap_or_else(|| "unknown".into());
        source_map.entry(source).or_default().push(attr.net_pnl);
    }
    let by_signal_source: serde_json::Value = serde_json::to_value(
        source_map
            .iter()
            .map(|(k, v)| {
                let sum: f64 = v.iter().sum();
                let count = v.len();
                let avg = sum / count as f64;
                serde_json::json!({"source": k, "count": count, "total_pnl": sum, "avg_pnl": avg})
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or(serde_json::json!([]));

    AttributionSummary {
        total_trades,
        win_count,
        loss_count,
        breakeven_count,
        win_rate: win_count as f64 / total_trades as f64,
        total_gross_pnl,
        total_net_pnl,
        total_fee,
        total_slippage,
        total_funding,
        total_impact,
        avg_alpha,
        cost_ratio,
        by_regime,
        by_exit_reason,
        by_signal_source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(net_pnl: f64, direction: &str, regime: Option<&str>) -> AttributionInput {
        AttributionInput {
            job_id: Some(Uuid::new_v4()),
            user_id: Some(1),
            symbol: "BTC-USDT".into(),
            order_id: None,
            fill_id: None,
            decision_card_id: None,
            entry_time: Utc::now(),
            exit_time: Some(Utc::now()),
            direction: direction.into(),
            gross_pnl: net_pnl + 10.0,
            fee_cost: 2.0,
            slippage_cost: 3.0,
            funding_cost: 1.0,
            impact_cost: 4.0,
            market_regime: regime.map(|s| s.into()),
            exit_regime: None,
            signal_source: Some("agent".into()),
            signal_confidence: Some(0.8),
            calibrated_probability: Some(0.65),
            exit_reason: Some("take_profit".into()),
            benchmark_return: Some(0.05),
        }
    }

    #[test]
    fn test_analyze_attribution_win() {
        let input = make_input(100.0, "long", Some("TrendingBull"));
        let attr = analyze_attribution(&input);
        assert_eq!(attr.win_loss.as_deref(), Some("win"));
        assert!(attr.net_pnl > 0.0);
        assert!(attr.alpha.is_some());
        let tags = attr.attribution_tags.as_array().unwrap();
        assert!(!tags.is_empty());
    }

    #[test]
    fn test_analyze_attribution_loss() {
        let input = make_input(-50.0, "long", Some("TrendingBear"));
        let attr = analyze_attribution(&input);
        assert_eq!(attr.win_loss.as_deref(), Some("loss"));
        assert!(attr.net_pnl < 0.0);
    }

    #[test]
    fn test_analyze_attribution_cost_dominant() {
        let mut input = make_input(5.0, "long", None);
        input.fee_cost = 10.0;
        input.slippage_cost = 10.0;
        let attr = analyze_attribution(&input);
        let tags = attr.attribution_tags.as_array().unwrap();
        let has_cost_tag = tags.iter().any(|t| {
            t.as_str().unwrap_or("").contains("cost_dominant")
        });
        assert!(has_cost_tag, "应有 cost_dominant 标签");
    }

    #[test]
    fn test_analyze_attribution_high_confidence_loser() {
        let mut input = make_input(-50.0, "long", None);
        input.calibrated_probability = Some(0.85);
        let attr = analyze_attribution(&input);
        let tags = attr.attribution_tags.as_array().unwrap();
        let has_tag = tags.iter().any(|t| {
            t.as_str().unwrap_or("").contains("high_confidence_loser")
        });
        assert!(has_tag, "应有 high_confidence_loser 标签");
    }

    #[test]
    fn test_summarize_attributions() {
        let attrs = vec![
            analyze_attribution(&make_input(100.0, "long", Some("TrendingBull"))),
            analyze_attribution(&make_input(-50.0, "short", Some("TrendingBear"))),
            analyze_attribution(&make_input(30.0, "long", Some("Ranging"))),
        ];
        let summary = summarize_attributions(&attrs);
        assert_eq!(summary.total_trades, 3);
        assert_eq!(summary.win_count, 2);
        assert_eq!(summary.loss_count, 1);
        assert!((summary.win_rate - 2.0 / 3.0).abs() < 1e-9);
        assert!(summary.total_net_pnl > 0.0);
    }

    #[test]
    fn test_summarize_empty() {
        let summary = summarize_attributions(&[]);
        assert_eq!(summary.total_trades, 0);
        assert_eq!(summary.win_rate, 0.0);
    }
}
