//! Strategy Failure Detection
//! 策略失效检测与告警
//!
//! 依据《系统评估与演进规划》第四阶段任务4：
//!   "策略失效提醒"
//!
//! 检测维度：
//! - 回撤突破：最大回撤超过历史基线
//! - 校准漂移：概率校准 Brier Score 显著退化
//! - 胜率下降：近期胜率显著低于历史基线
//! - 盈亏比下降：近期盈亏比显著低于历史基线
//! - 相关性断裂：策略间相关性结构发生变化
//! - 状态切换：市场状态发生重大转变

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 策略失效告警类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    DrawdownBreach,
    CalibrationDrift,
    WinRateDrop,
    ProfitFactorDrop,
    CorrelationBreakdown,
    RegimeShift,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DrawdownBreach => "drawdown_breach",
            Self::CalibrationDrift => "calibration_drift",
            Self::WinRateDrop => "win_rate_drop",
            Self::ProfitFactorDrop => "profit_factor_drop",
            Self::CorrelationBreakdown => "correlation_breakdown",
            Self::RegimeShift => "regime_shift",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "drawdown_breach" => Some(Self::DrawdownBreach),
            "calibration_drift" => Some(Self::CalibrationDrift),
            "win_rate_drop" => Some(Self::WinRateDrop),
            "profit_factor_drop" => Some(Self::ProfitFactorDrop),
            "correlation_breakdown" => Some(Self::CorrelationBreakdown),
            "regime_shift" => Some(Self::RegimeShift),
            _ => None,
        }
    }
}

/// 告警级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// 策略失效告警记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StrategyFailureAlert {
    pub alert_id: Uuid,
    pub strategy_id: Option<Uuid>,
    pub strategy_name: String,
    pub symbol: Option<String>,
    pub user_id: Option<i64>,
    pub alert_type: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub trigger_metric: String,
    pub trigger_value: f64,
    pub threshold_value: Option<f64>,
    pub baseline_value: Option<f64>,
    pub eval_window_start: DateTime<Utc>,
    pub eval_window_end: DateTime<Utc>,
    pub sample_count: i32,
    pub recommended_action: Option<String>,
    pub auto_action_taken: Option<String>,
    pub status: String,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<i64>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// 失效检测输入参数
#[derive(Debug, Clone)]
pub struct FailureDetectionInput {
    pub strategy_name: String,
    pub symbol: Option<String>,
    pub user_id: Option<i64>,
    pub eval_window_start: DateTime<Utc>,
    pub eval_window_end: DateTime<Utc>,
    pub sample_count: i32,

    /// 当前最大回撤
    pub current_max_drawdown: Option<f64>,
    /// 历史基线回撤
    pub baseline_max_drawdown: Option<f64>,
    /// 回撤阈值
    pub drawdown_threshold: Option<f64>,

    /// 当前 Brier Score
    pub current_brier_score: Option<f64>,
    /// 基线 Brier Score
    pub baseline_brier_score: Option<f64>,

    /// 当前胜率
    pub current_win_rate: Option<f64>,
    /// 基线胜率
    pub baseline_win_rate: Option<f64>,

    /// 当前盈亏比
    pub current_profit_factor: Option<f64>,
    /// 基线盈亏比
    pub baseline_profit_factor: Option<f64>,

    /// 当前市场状态
    pub current_regime: Option<String>,
    /// 前一市场状态
    pub previous_regime: Option<String>,
}

impl Default for FailureDetectionInput {
    fn default() -> Self {
        Self {
            strategy_name: "default".into(),
            symbol: None,
            user_id: None,
            eval_window_start: Utc::now(),
            eval_window_end: Utc::now(),
            sample_count: 0,
            current_max_drawdown: None,
            baseline_max_drawdown: None,
            drawdown_threshold: Some(0.20),
            current_brier_score: None,
            baseline_brier_score: None,
            current_win_rate: None,
            baseline_win_rate: None,
            current_profit_factor: None,
            baseline_profit_factor: None,
            current_regime: None,
            previous_regime: None,
        }
    }
}

/// 执行策略失效检测
///
/// 返回检测到的告警列表（可能为空）
pub fn detect_failures(input: &FailureDetectionInput) -> Vec<StrategyFailureAlert> {
    let mut alerts = Vec::new();
    let now = Utc::now();

    // 1. 回撤突破检测
    if let (Some(current_dd), Some(threshold)) =
        (input.current_max_drawdown, input.drawdown_threshold)
    {
        if current_dd.abs() > threshold.abs() {
            let severity = if current_dd.abs() > threshold.abs() * 1.5 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };
            alerts.push(StrategyFailureAlert {
                alert_id: Uuid::new_v4(),
                strategy_id: None,
                strategy_name: input.strategy_name.clone(),
                symbol: input.symbol.clone(),
                user_id: input.user_id,
                alert_type: AlertType::DrawdownBreach.as_str().into(),
                severity: severity.as_str().into(),
                title: format!(
                    "最大回撤 {:.1}% 超过阈值 {:.1}%",
                    current_dd * 100.0,
                    threshold * 100.0
                ),
                description: format!(
                    "策略 '{}' 在评估窗口内最大回撤为 {:.1}%，超过设定的 {:.1}% 阈值",
                    input.strategy_name,
                    current_dd * 100.0,
                    threshold * 100.0
                ),
                trigger_metric: "max_drawdown".into(),
                trigger_value: current_dd,
                threshold_value: Some(threshold),
                baseline_value: input.baseline_max_drawdown,
                eval_window_start: input.eval_window_start,
                eval_window_end: input.eval_window_end,
                sample_count: input.sample_count,
                recommended_action: Some("考虑减仓或暂停策略，检查市场状态是否发生重大变化".into()),
                auto_action_taken: None,
                status: "active".into(),
                acknowledged_at: None,
                acknowledged_by: None,
                resolved_at: None,
                metadata: serde_json::json!({
                    "current_drawdown": current_dd,
                    "threshold": threshold,
                    "baseline": input.baseline_max_drawdown,
                }),
                created_at: now,
            });
        }
    }

    // 2. 校准漂移检测
    if let (Some(current_brier), Some(baseline_brier)) =
        (input.current_brier_score, input.baseline_brier_score)
    {
        let drift = current_brier - baseline_brier;
        if drift > 0.05 {
            let severity = if drift > 0.15 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };
            alerts.push(StrategyFailureAlert {
                alert_id: Uuid::new_v4(),
                strategy_id: None,
                strategy_name: input.strategy_name.clone(),
                symbol: input.symbol.clone(),
                user_id: input.user_id,
                alert_type: AlertType::CalibrationDrift.as_str().into(),
                severity: severity.as_str().into(),
                title: format!(
                    "概率校准漂移：Brier Score 从 {:.4} 升至 {:.4}",
                    baseline_brier, current_brier
                ),
                description: format!(
                    "策略 '{}' 的 Brier Score 从基线 {:.4} 恶化至 {:.4}，漂移 {:.4}，模型预测可靠性下降",
                    input.strategy_name,
                    baseline_brier,
                    current_brier,
                    drift
                ),
                trigger_metric: "brier_score".into(),
                trigger_value: current_brier,
                threshold_value: Some(baseline_brier + 0.05),
                baseline_value: Some(baseline_brier),
                eval_window_start: input.eval_window_start,
                eval_window_end: input.eval_window_end,
                sample_count: input.sample_count,
                recommended_action: Some("重新训练校准模型，或降低策略仓位权重".into()),
                auto_action_taken: None,
                status: "active".into(),
                acknowledged_at: None,
                acknowledged_by: None,
                resolved_at: None,
                metadata: serde_json::json!({
                    "current_brier": current_brier,
                    "baseline_brier": baseline_brier,
                    "drift": drift,
                }),
                created_at: now,
            });
        }
    }

    // 3. 胜率下降检测
    if let (Some(current_wr), Some(baseline_wr)) =
        (input.current_win_rate, input.baseline_win_rate)
    {
        let drop = baseline_wr - current_wr;
        if drop > 0.10 {
            let severity = if drop > 0.20 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };
            alerts.push(StrategyFailureAlert {
                alert_id: Uuid::new_v4(),
                strategy_id: None,
                strategy_name: input.strategy_name.clone(),
                symbol: input.symbol.clone(),
                user_id: input.user_id,
                alert_type: AlertType::WinRateDrop.as_str().into(),
                severity: severity.as_str().into(),
                title: format!(
                    "胜率下降：从 {:.1}% 降至 {:.1}%",
                    baseline_wr * 100.0,
                    current_wr * 100.0
                ),
                description: format!(
                    "策略 '{}' 的胜率从基线 {:.1}% 下降至 {:.1}%，降幅 {:.1}%",
                    input.strategy_name,
                    baseline_wr * 100.0,
                    current_wr * 100.0,
                    drop * 100.0
                ),
                trigger_metric: "win_rate".into(),
                trigger_value: current_wr,
                threshold_value: Some(baseline_wr - 0.10),
                baseline_value: Some(baseline_wr),
                eval_window_start: input.eval_window_start,
                eval_window_end: input.eval_window_end,
                sample_count: input.sample_count,
                recommended_action: Some("检查信号源是否退化，评估市场状态是否已转变".into()),
                auto_action_taken: None,
                status: "active".into(),
                acknowledged_at: None,
                acknowledged_by: None,
                resolved_at: None,
                metadata: serde_json::json!({
                    "current_win_rate": current_wr,
                    "baseline_win_rate": baseline_wr,
                    "drop": drop,
                }),
                created_at: now,
            });
        }
    }

    // 4. 盈亏比下降检测
    if let (Some(current_pf), Some(baseline_pf)) =
        (input.current_profit_factor, input.baseline_profit_factor)
    {
        let drop = baseline_pf - current_pf;
        if drop > 0.3 && baseline_pf > 0.0 {
            let severity = if drop > 0.6 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };
            alerts.push(StrategyFailureAlert {
                alert_id: Uuid::new_v4(),
                strategy_id: None,
                strategy_name: input.strategy_name.clone(),
                symbol: input.symbol.clone(),
                user_id: input.user_id,
                alert_type: AlertType::ProfitFactorDrop.as_str().into(),
                severity: severity.as_str().into(),
                title: format!(
                    "盈亏比下降：从 {:.2} 降至 {:.2}",
                    baseline_pf, current_pf
                ),
                description: format!(
                    "策略 '{}' 的盈亏比从基线 {:.2} 下降至 {:.2}，降幅 {:.2}",
                    input.strategy_name,
                    baseline_pf,
                    current_pf,
                    drop
                ),
                trigger_metric: "profit_factor".into(),
                trigger_value: current_pf,
                threshold_value: Some(baseline_pf - 0.3),
                baseline_value: Some(baseline_pf),
                eval_window_start: input.eval_window_start,
                eval_window_end: input.eval_window_end,
                sample_count: input.sample_count,
                recommended_action: Some("检查止损策略是否失效，调整风险预算".into()),
                auto_action_taken: None,
                status: "active".into(),
                acknowledged_at: None,
                acknowledged_by: None,
                resolved_at: None,
                metadata: serde_json::json!({
                    "current_pf": current_pf,
                    "baseline_pf": baseline_pf,
                    "drop": drop,
                }),
                created_at: now,
            });
        }
    }

    // 5. 市场状态切换检测
    if let (Some(current), Some(previous)) = (&input.current_regime, &input.previous_regime) {
        if current != previous {
            let is_crisis = current == "Crisis" || current == "HighVolatility";
            let severity = if is_crisis {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Info
            };
            alerts.push(StrategyFailureAlert {
                alert_id: Uuid::new_v4(),
                strategy_id: None,
                strategy_name: input.strategy_name.clone(),
                symbol: input.symbol.clone(),
                user_id: input.user_id,
                alert_type: AlertType::RegimeShift.as_str().into(),
                severity: severity.as_str().into(),
                title: format!(
                    "市场状态切换：{} → {}",
                    previous, current
                ),
                description: format!(
                    "市场状态从 '{}' 切换至 '{}'，策略可能需要重新评估适用性",
                    previous, current
                ),
                trigger_metric: "market_regime".into(),
                trigger_value: 1.0,
                threshold_value: None,
                baseline_value: None,
                eval_window_start: input.eval_window_start,
                eval_window_end: input.eval_window_end,
                sample_count: input.sample_count,
                recommended_action: Some(if is_crisis {
                    "建议减仓或暂停策略，等待市场稳定".into()
                } else {
                    "评估策略在新市场状态下的历史表现".into()
                }),
                auto_action_taken: None,
                status: "active".into(),
                acknowledged_at: None,
                acknowledged_by: None,
                resolved_at: None,
                metadata: serde_json::json!({
                    "previous_regime": previous,
                    "current_regime": current,
                }),
                created_at: now,
            });
        }
    }

    alerts
}

/// 保存告警到数据库
pub async fn save_alert(
    pool: &sqlx::PgPool,
    alert: &StrategyFailureAlert,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO strategy_failure_alerts
           (alert_id, strategy_id, strategy_name, symbol, user_id,
            alert_type, severity, title, description,
            trigger_metric, trigger_value, threshold_value, baseline_value,
            eval_window_start, eval_window_end, sample_count,
            recommended_action, auto_action_taken,
            status, acknowledged_at, acknowledged_by, resolved_at,
            metadata, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                   $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)"#,
    )
    .bind(alert.alert_id)
    .bind(alert.strategy_id)
    .bind(&alert.strategy_name)
    .bind(&alert.symbol)
    .bind(alert.user_id)
    .bind(&alert.alert_type)
    .bind(&alert.severity)
    .bind(&alert.title)
    .bind(&alert.description)
    .bind(&alert.trigger_metric)
    .bind(alert.trigger_value)
    .bind(alert.threshold_value)
    .bind(alert.baseline_value)
    .bind(alert.eval_window_start)
    .bind(alert.eval_window_end)
    .bind(alert.sample_count)
    .bind(&alert.recommended_action)
    .bind(&alert.auto_action_taken)
    .bind(&alert.status)
    .bind(alert.acknowledged_at)
    .bind(alert.acknowledged_by)
    .bind(alert.resolved_at)
    .bind(&alert.metadata)
    .bind(alert.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 查询活跃告警
pub async fn list_active_alerts(
    pool: &sqlx::PgPool,
    user_id: Option<i64>,
    symbol: Option<&str>,
    limit: i64,
) -> Result<Vec<StrategyFailureAlert>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT * FROM strategy_failure_alerts WHERE status = 'active'",
    );
    let mut binds: Vec<String> = Vec::new();

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

    sql.push_str(" ORDER BY created_at DESC LIMIT $");
    sql.push_str(&(binds.len() + 1).to_string());

    let mut query = sqlx::query_as::<_, StrategyFailureAlert>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);

    query.fetch_all(pool).await
}

/// 确认告警
pub async fn acknowledge_alert(
    pool: &sqlx::PgPool,
    alert_id: Uuid,
    user_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE strategy_failure_alerts SET status = 'acknowledged', acknowledged_at = NOW(), acknowledged_by = $1 WHERE alert_id = $2",
    )
    .bind(user_id)
    .bind(alert_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 解决告警
pub async fn resolve_alert(
    pool: &sqlx::PgPool,
    alert_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE strategy_failure_alerts SET status = 'resolved', resolved_at = NOW() WHERE alert_id = $1",
    )
    .bind(alert_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_drawdown_breach() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_max_drawdown: Some(0.25),
            baseline_max_drawdown: Some(0.10),
            drawdown_threshold: Some(0.20),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "drawdown_breach");
        assert_eq!(alerts[0].severity, "warning");
    }

    #[test]
    fn test_detect_drawdown_critical() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_max_drawdown: Some(0.35),
            baseline_max_drawdown: Some(0.10),
            drawdown_threshold: Some(0.20),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, "critical");
    }

    #[test]
    fn test_detect_calibration_drift() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_brier_score: Some(0.30),
            baseline_brier_score: Some(0.15),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "calibration_drift");
    }

    #[test]
    fn test_detect_win_rate_drop() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_win_rate: Some(0.40),
            baseline_win_rate: Some(0.60),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "win_rate_drop");
    }

    #[test]
    fn test_detect_profit_factor_drop() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_profit_factor: Some(1.0),
            baseline_profit_factor: Some(1.8),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "profit_factor_drop");
    }

    #[test]
    fn test_detect_regime_shift() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_regime: Some("Crisis".into()),
            previous_regime: Some("TrendingBull".into()),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "regime_shift");
        assert_eq!(alerts[0].severity, "critical");
    }

    #[test]
    fn test_detect_no_failures() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_max_drawdown: Some(0.05),
            drawdown_threshold: Some(0.20),
            current_brier_score: Some(0.15),
            baseline_brier_score: Some(0.14),
            current_win_rate: Some(0.58),
            baseline_win_rate: Some(0.60),
            current_profit_factor: Some(1.7),
            baseline_profit_factor: Some(1.8),
            current_regime: Some("TrendingBull".into()),
            previous_regime: Some("TrendingBull".into()),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert!(alerts.is_empty(), "无异常时不应产生告警");
    }

    #[test]
    fn test_detect_multiple_failures() {
        let input = FailureDetectionInput {
            strategy_name: "test".into(),
            current_max_drawdown: Some(0.30),
            drawdown_threshold: Some(0.20),
            current_brier_score: Some(0.35),
            baseline_brier_score: Some(0.15),
            current_win_rate: Some(0.30),
            baseline_win_rate: Some(0.60),
            current_regime: Some("Crisis".into()),
            previous_regime: Some("TrendingBull".into()),
            ..Default::default()
        };
        let alerts = detect_failures(&input);
        assert_eq!(alerts.len(), 4, "应检测到 4 种失效");
    }

    #[test]
    fn test_alert_type_serialization() {
        assert_eq!(AlertType::DrawdownBreach.as_str(), "drawdown_breach");
        assert_eq!(
            AlertType::from_str("calibration_drift"),
            Some(AlertType::CalibrationDrift)
        );
        assert_eq!(AlertType::from_str("invalid"), None);
    }
}
