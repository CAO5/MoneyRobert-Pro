//! Performance & attribution engine.
//! 绩效与归因分析

use crate::backtest::models::{PerformanceReport, TradeAttribution};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct PerformanceEngine;

impl PerformanceEngine {
    /// Compute annualized return given total return & time span in seconds.
    fn annualized(total_return: f64, seconds: f64) -> f64 {
        if seconds <= 0.0 {
            return total_return;
        }
        let years = seconds / (365.0 * 24.0 * 3600.0);
        (1.0 + total_return).powf(1.0 / years) - 1.0
    }

    /// 取每日最后一个权益点作为日末权益（避免使用每日最高权益美化表现）。
    fn daily_equity_close(equity: &[(DateTime<Utc>, f64)]) -> Vec<(chrono::NaiveDate, f64)> {
        let mut daily: HashMap<chrono::NaiveDate, (DateTime<Utc>, f64)> = HashMap::new();
        for (ts, eq) in equity {
            let day = ts.date_naive();
            let entry = daily.entry(day).or_insert((*ts, *eq));
            // 取该日时间戳最大的点作为日末权益
            if *ts > entry.0 {
                *entry = (*ts, *eq);
            }
        }
        let mut days: Vec<(chrono::NaiveDate, f64)> =
            daily.into_iter().map(|(d, (_, e))| (d, e)).collect();
        days.sort_by_key(|(d, _)| *d);
        days
    }

    /// Compute Sharpe from daily returns (使用日末权益，年化 sqrt(365))。
    fn sharpe_ratio(equity: &[(DateTime<Utc>, f64)]) -> f64 {
        let days = Self::daily_equity_close(equity);
        if days.len() < 2 {
            return 0.0;
        }
        let mut rets = Vec::with_capacity(days.len() - 1);
        for i in 1..days.len() {
            let prev = days[i - 1].1;
            let curr = days[i].1;
            if prev > 0.0 {
                rets.push((curr - prev) / prev);
            }
        }
        Self::sharpe_from_returns(&rets)
    }

    /// 从日收益序列计算夏普率（年化，无风险利率假设为 0）。
    fn sharpe_from_returns(rets: &[f64]) -> f64 {
        let n = rets.len() as f64;
        if n == 0.0 {
            return 0.0;
        }
        let mean = rets.iter().sum::<f64>() / n;
        let variance = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        let std = variance.sqrt();
        if std == 0.0 {
            return 0.0;
        }
        (mean / std) * (365.0_f64).sqrt()
    }

    /// Compute Sortino from daily returns（仅对下行波动率惩罚）。
    fn sortino_ratio(equity: &[(DateTime<Utc>, f64)]) -> f64 {
        let days = Self::daily_equity_close(equity);
        if days.len() < 2 {
            return 0.0;
        }
        let mut rets = Vec::with_capacity(days.len() - 1);
        for i in 1..days.len() {
            let prev = days[i - 1].1;
            let curr = days[i].1;
            if prev > 0.0 {
                rets.push((curr - prev) / prev);
            }
        }
        let n = rets.len() as f64;
        if n == 0.0 {
            return 0.0;
        }
        let mean = rets.iter().sum::<f64>() / n;
        // 下行偏差：仅对负收益计算平方
        let downside_var: f64 = rets
            .iter()
            .filter(|r| **r < 0.0)
            .map(|r| r.powi(2))
            .sum::<f64>()
            / n;
        let downside_std = downside_var.sqrt();
        if downside_std == 0.0 {
            return 0.0;
        }
        (mean / downside_std) * (365.0_f64).sqrt()
    }

    /// Max drawdown (percent), drawdown curve, and longest drawdown duration in seconds.
    fn max_drawdown(equity: &[(DateTime<Utc>, f64)]) -> (f64, Vec<(DateTime<Utc>, f64)>, i64) {
        let mut peak = f64::NEG_INFINITY;
        let mut peak_time: Option<DateTime<Utc>> = None;
        let mut active_drawdown_start: Option<DateTime<Utc>> = None;
        let mut max_duration = Duration::zero();
        let mut max_dd = 0.0_f64;
        let mut drawdown_curve = Vec::with_capacity(equity.len());

        for (ts, eq) in equity {
            if *eq >= peak {
                peak = *eq;
                peak_time = Some(*ts);
                active_drawdown_start = None;
            }

            let dd = if peak > 0.0 { (peak - eq) / peak } else { 0.0 };
            if dd > 0.0 {
                let start = active_drawdown_start.get_or_insert_with(|| peak_time.unwrap_or(*ts));
                let duration = *ts - *start;
                if duration > max_duration {
                    max_duration = duration;
                }
            }
            if dd > max_dd {
                max_dd = dd;
            }
            drawdown_curve.push((*ts, dd));
        }
        (max_dd, drawdown_curve, max_duration.num_seconds())
    }
    /// Calmar ratio = annualized_return / max_drawdown.
    fn calmar_ratio(annualized: f64, max_drawdown: f64) -> f64 {
        if max_drawdown <= 0.0 {
            return 0.0;
        }
        annualized / max_drawdown
    }

    fn daily_returns(equity: &[(DateTime<Utc>, f64)]) -> Vec<f64> {
        let days = Self::daily_equity_close(equity);
        let mut rets = Vec::with_capacity(days.len().saturating_sub(1));
        for i in 1..days.len() {
            let prev = days[i - 1].1;
            let curr = days[i].1;
            if prev > 0.0 {
                rets.push((curr - prev) / prev);
            }
        }
        rets
    }

    fn var_cvar_95(returns: &[f64]) -> (f64, f64) {
        if returns.is_empty() {
            return (0.0, 0.0);
        }
        let mut sorted = returns.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((sorted.len() as f64) * 0.05).floor() as usize;
        let idx = idx.min(sorted.len() - 1);
        let var_95 = (-sorted[idx]).max(0.0);
        let tail = &sorted[..=idx];
        let avg_tail = tail.iter().sum::<f64>() / tail.len() as f64;
        let cvar_95 = (-avg_tail).max(0.0);
        (var_95, cvar_95)
    }

    fn alpha_beta(strategy_returns: &[f64], benchmark_returns: Option<&[f64]>) -> (f64, f64) {
        let Some(bench) = benchmark_returns else {
            let mean = if strategy_returns.is_empty() {
                0.0
            } else {
                strategy_returns.iter().sum::<f64>() / strategy_returns.len() as f64
            };
            return (mean * 365.0, 0.0);
        };
        let n = strategy_returns.len().min(bench.len());
        if n == 0 {
            return (0.0, 0.0);
        }
        let s = &strategy_returns[..n];
        let b = &bench[..n];
        let mean_s = s.iter().sum::<f64>() / n as f64;
        let mean_b = b.iter().sum::<f64>() / n as f64;
        let var_b = b.iter().map(|r| (r - mean_b).powi(2)).sum::<f64>() / n as f64;
        let beta = if var_b > 0.0 {
            s.iter()
                .zip(b.iter())
                .map(|(sr, br)| (sr - mean_s) * (br - mean_b))
                .sum::<f64>()
                / n as f64
                / var_b
        } else {
            0.0
        };
        let alpha = (mean_s - beta * mean_b) * 365.0;
        (alpha, beta)
    }
    pub fn compute_report(
        &self,
        trades: Vec<TradeAttribution>,
        equity_curve: Vec<(DateTime<Utc>, f64)>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        total_fee: f64,
        total_slippage_cost: f64,
    ) -> PerformanceReport {
        self.compute_report_with_benchmark(
            trades,
            equity_curve,
            start_time,
            end_time,
            total_fee,
            total_slippage_cost,
            None,
        )
    }

    /// 计算完整绩效报告，支持可选的基准收益序列用于 Alpha/Beta 计算。
    /// 当 benchmark_returns 为 None 时，beta 恒为 0，alpha 仅为策略自身日均收益年化。
    pub fn compute_report_with_benchmark(
        &self,
        trades: Vec<TradeAttribution>,
        equity_curve: Vec<(DateTime<Utc>, f64)>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        total_fee: f64,
        total_slippage_cost: f64,
        benchmark_returns: Option<&[f64]>,
    ) -> PerformanceReport {
        // Total return
        let start_equity = equity_curve.first().map(|(_, e)| *e).unwrap_or(1.0);
        let end_equity = equity_curve.last().map(|(_, e)| *e).unwrap_or(start_equity);
        let total_return = if start_equity > 0.0 {
            (end_equity - start_equity) / start_equity
        } else {
            0.0
        };
        let seconds = (end_time - start_time).num_seconds() as f64;
        let annualized = Self::annualized(total_return, seconds);

        let (max_drawdown, drawdown_curve, max_drawdown_duration_sec) =
            Self::max_drawdown(&equity_curve);
        let daily_returns = Self::daily_returns(&equity_curve);
        let sharpe = Self::sharpe_ratio(&equity_curve);
        let sortino = Self::sortino_ratio(&equity_curve);
        let calmar = Self::calmar_ratio(annualized, max_drawdown);
        let (var_95, cvar_95) = Self::var_cvar_95(&daily_returns);
        let (alpha, beta) = Self::alpha_beta(&daily_returns, benchmark_returns);

        let total_trades = trades.len() as i64;
        let (wins, losses): (Vec<&TradeAttribution>, Vec<&TradeAttribution>) =
            trades.iter().partition(|t| t.pnl.unwrap_or(0.0) > 0.0);
        let winning_trades = wins.len() as i64;
        let losing_trades = losses.len() as i64;
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };
        let gross_profit: f64 = wins.iter().map(|t| t.pnl.unwrap_or(0.0).max(0.0)).sum();
        let gross_loss: f64 = losses
            .iter()
            .map(|t| (-t.pnl.unwrap_or(0.0)).max(0.0))
            .sum();
        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else {
            gross_profit
        };
        let average_win = if winning_trades > 0 {
            gross_profit / winning_trades as f64
        } else {
            0.0
        };
        let average_loss = if losing_trades > 0 {
            gross_loss / losing_trades as f64
        } else {
            0.0
        };
        let payoff_ratio = if average_loss > 0.0 {
            average_win / average_loss
        } else {
            average_win
        };

        // attribution by agent / asset / regime
        let mut by_agent: HashMap<String, (i64, f64)> = HashMap::new();
        let mut by_asset: HashMap<String, (i64, f64)> = HashMap::new();
        let mut by_regime: HashMap<String, (i64, f64)> = HashMap::new();
        for t in &trades {
            let agent = t.agent_id.clone().unwrap_or_else(|| "unknown".into());
            let entry = by_agent.entry(agent).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += t.pnl.unwrap_or(0.0);
            let entry2 = by_asset.entry(t.asset.clone()).or_insert((0, 0.0));
            entry2.0 += 1;
            entry2.1 += t.pnl.unwrap_or(0.0);
            // 按入场时市场状态归因
            let regime = t
                .market_regime_at_entry
                .clone()
                .unwrap_or_else(|| "unknown".into());
            let entry3 = by_regime.entry(regime).or_insert((0, 0.0));
            entry3.0 += 1;
            entry3.1 += t.pnl.unwrap_or(0.0);
        }

        let to_json = |m: HashMap<String, (i64, f64)>| -> Value {
            let map: serde_json::Map<String, Value> = m
                .into_iter()
                .map(|(k, (cnt, pnl))| (k, json!({ "trades": cnt, "pnl": pnl.round() as i64 })))
                .collect();
            Value::Object(map)
        };

        PerformanceReport {
            total_return,
            annualized_return: annualized,
            max_drawdown,
            sharpe_ratio: sharpe,
            sortino_ratio: sortino,
            calmar_ratio: calmar,
            win_rate,
            profit_factor,
            total_trades,
            winning_trades,
            losing_trades,
            average_win,
            average_loss,
            payoff_ratio,
            total_fee,
            total_slippage_cost,
            var_95,
            cvar_95,
            alpha,
            beta,
            max_drawdown_duration_sec,
            equity_curve,
            drawdown_curve,
            trades,
            by_agent: to_json(by_agent),
            by_asset: to_json(by_asset),
            by_regime: to_json(by_regime),
        }
    }
}

/// Compute performance summary (JSON) for the REST API.
pub fn report_to_summary(report: &PerformanceReport) -> Value {
    json!({
        "total_return": report.total_return,
        "annualized_return": report.annualized_return,
        "max_drawdown": report.max_drawdown,
        "sharpe_ratio": report.sharpe_ratio,
        "sortino_ratio": report.sortino_ratio,
        "calmar_ratio": report.calmar_ratio,
        "win_rate": report.win_rate,
        "profit_factor": report.profit_factor,
        "total_trades": report.total_trades,
        "winning_trades": report.winning_trades,
        "losing_trades": report.losing_trades,
        "average_win": report.average_win,
        "average_loss": report.average_loss,
        "payoff_ratio": report.payoff_ratio,
        "total_fee": report.total_fee,
        "total_slippage_cost": report.total_slippage_cost,
        "var_95": report.var_95,
        "cvar_95": report.cvar_95,
        "alpha": report.alpha,
        "beta": report.beta,
        "max_drawdown_duration_sec": report.max_drawdown_duration_sec,
        "equity_points": report.equity_curve.len(),
        "by_regime": report.by_regime,
    })
}

/// Format duration as human-readable.
pub fn fmt_duration(d: Duration) -> String {
    let days = d.num_days();
    let hours = (d - Duration::days(days)).num_hours();
    format!("{}d {}h", days, hours)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn ts(days_after_start: i64) -> DateTime<Utc> {
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        base + Duration::days(days_after_start)
    }

    /// 构造日末权益曲线：每天一个点
    fn make_equity_curve(values: &[f64]) -> Vec<(DateTime<Utc>, f64)> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| (ts(i as i64), *v))
            .collect()
    }

    #[test]
    fn test_sharpe_ratio_positive_for_growing_equity() {
        // 每日稳定增长的权益曲线，夏普率应为正
        let equity = make_equity_curve(&[100.0, 101.0, 102.0, 103.0, 104.0]);
        let sharpe = PerformanceEngine::sharpe_ratio(&equity);
        assert!(sharpe > 0.0, "夏普率应为正，实际: {}", sharpe);
    }

    #[test]
    fn test_sharpe_ratio_uses_daily_close_not_peak() {
        // 关键测试：夏普率应使用日末权益，而非每日最高权益
        // 构造一个曲线：每日内有波动但日末权益相同
        // 如果使用每日最高权益，会错误地认为有收益
        let equity = vec![
            (ts(0), 100.0),
            (ts(0), 105.0), // 当日高点
            (ts(0), 100.0), // 日末回落
            (ts(1), 100.0),
            (ts(1), 105.0),
            (ts(1), 100.0), // 日末
        ];
        let sharpe = PerformanceEngine::sharpe_ratio(&equity);
        // 日末权益相同，收益为 0，夏普率应为 0
        assert!(
            sharpe.abs() < 1e-9,
            "日末权益相同时夏普率应为 0，实际: {}",
            sharpe
        );
    }

    #[test]
    fn test_sortino_ratio_positive_for_growing_equity() {
        // 包含下行波动的增长曲线（有正有负的日收益）
        let equity = make_equity_curve(&[100.0, 103.0, 101.0, 105.0, 104.0, 108.0]);
        let sortino = PerformanceEngine::sortino_ratio(&equity);
        assert!(sortino > 0.0, "Sortino 应为正，实际: {}", sortino);
    }

    #[test]
    fn test_sortino_ratio_zero_when_no_downside() {
        // 完全无下行波动时，Sortino 应为 0（分母为 0）
        let equity = make_equity_curve(&[100.0, 101.0, 102.0, 103.0]);
        let sortino = PerformanceEngine::sortino_ratio(&equity);
        assert!(
            sortino.abs() < 1e-9,
            "无下行波动时 Sortino 应为 0，实际: {}",
            sortino
        );
    }

    #[test]
    fn test_sortino_lower_than_sharpe_for_mixed_returns() {
        // 混合收益（有正有负）时，Sortino 通常高于 Sharpe（因为只惩罚下行）
        let equity = make_equity_curve(&[100.0, 105.0, 102.0, 108.0, 104.0, 110.0]);
        let sharpe = PerformanceEngine::sharpe_ratio(&equity);
        let sortino = PerformanceEngine::sortino_ratio(&equity);
        // 两者都应为正
        assert!(sharpe > 0.0);
        assert!(sortino > 0.0);
        // Sortino >= Sharpe（因为下行波动率 <= 总波动率）
        assert!(
            sortino >= sharpe - 1e-9,
            "Sortino ({}) 应 >= Sharpe ({})",
            sortino,
            sharpe
        );
    }

    #[test]
    fn test_max_drawdown_calculation() {
        // 权益曲线：100 -> 120 -> 90 -> 110
        // 最大回撤 = (120 - 90) / 120 = 25%
        let equity = make_equity_curve(&[100.0, 120.0, 90.0, 110.0]);
        let (max_dd, curve, duration_sec) = PerformanceEngine::max_drawdown(&equity);
        assert!(
            (max_dd - 0.25).abs() < 1e-9,
            "最大回撤应为 0.25，实际: {}",
            max_dd
        );
        assert_eq!(curve.len(), 4);
        assert!(duration_sec > 0);
    }

    #[test]
    fn test_calmar_ratio_calculation() {
        // 年化收益 20%，最大回撤 10%，Calmar = 2.0
        let calmar = PerformanceEngine::calmar_ratio(0.20, 0.10);
        assert!((calmar - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_calmar_ratio_zero_when_no_drawdown() {
        let calmar = PerformanceEngine::calmar_ratio(0.20, 0.0);
        assert!((calmar - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_compute_report_full_metrics() {
        let equity = make_equity_curve(&[10000.0, 10500.0, 10200.0, 10800.0]);
        let start = ts(0);
        let end = ts(3);
        let trades = vec![];
        let report = PerformanceEngine.compute_report(trades, equity, start, end, 10.0, 2.5);

        // 验证所有指标都被计算（非默认值）
        assert!(report.total_return > 0.0);
        assert!(report.sharpe_ratio != 0.0 || report.sortino_ratio != 0.0);
        assert!(report.max_drawdown > 0.0);
        assert!(report.calmar_ratio >= 0.0);
        assert_eq!(report.total_fee, 10.0);
    }

    #[test]
    fn test_report_to_summary_includes_all_metrics() {
        let report = PerformanceReport {
            total_return: 0.10,
            annualized_return: 0.20,
            max_drawdown: 0.05,
            sharpe_ratio: 1.5,
            sortino_ratio: 2.0,
            calmar_ratio: 4.0,
            win_rate: 0.6,
            profit_factor: 1.8,
            total_trades: 100,
            winning_trades: 60,
            losing_trades: 40,
            average_win: 100.0,
            average_loss: 50.0,
            payoff_ratio: 2.0,
            total_fee: 50.0,
            total_slippage_cost: 10.0,
            var_95: 0.02,
            cvar_95: 0.03,
            alpha: 0.12,
            beta: 0.8,
            max_drawdown_duration_sec: 86_400,
            equity_curve: vec![],
            drawdown_curve: vec![],
            trades: vec![],
            by_agent: json!({}),
            by_asset: json!({}),
            by_regime: json!({}),
        };
        let summary = report_to_summary(&report);
        let obj = summary.as_object().unwrap();
        // 验证新增字段存在
        assert!(obj.contains_key("sortino_ratio"));
        assert!(obj.contains_key("calmar_ratio"));
        assert!(obj.contains_key("total_slippage_cost"));
        assert!(obj.contains_key("by_regime"));
        assert!(obj.contains_key("var_95"));
        assert!(obj.contains_key("cvar_95"));
        assert!(obj.contains_key("alpha"));
        assert!(obj.contains_key("beta"));
        assert!(obj.contains_key("max_drawdown_duration_sec"));
        assert_eq!(summary["calmar_ratio"], 4.0);
        assert_eq!(summary["sortino_ratio"], 2.0);
        assert_eq!(summary["cvar_95"], 0.03);
    }

    #[test]
    fn test_var_cvar_95_are_positive_tail_losses() {
        let returns = vec![0.01, -0.02, 0.03, -0.10, 0.02];
        let (var_95, cvar_95) = PerformanceEngine::var_cvar_95(&returns);
        assert!((var_95 - 0.10).abs() < 1e-9);
        assert!((cvar_95 - 0.10).abs() < 1e-9);
    }

    #[test]
    fn test_alpha_beta_against_benchmark() {
        let strategy = vec![0.02, 0.01, -0.01, 0.03];
        let benchmark = vec![0.01, 0.005, -0.005, 0.015];
        let (alpha, beta) = PerformanceEngine::alpha_beta(&strategy, Some(&benchmark));
        assert!(beta > 1.9 && beta < 2.1);
        assert!(alpha.abs() < 1e-9);
    }
    #[test]
    fn test_by_regime_attribution_groups_trades() {
        // 验证 by_regime 归因按市场状态分组统计交易
        let start = ts(0);
        let end = ts(10);
        let equity = make_equity_curve(&[10000.0, 10500.0, 10200.0, 10800.0]);

        let make_trade = |regime: &str, pnl: f64| TradeAttribution {
            attribution_id: Uuid::new_v4(),
            job_id: None,
            asset: "BTC-USDT-SWAP".into(),
            strategy_id: None,
            agent_id: None,
            direction: "long".into(),
            entry_time: start,
            exit_time: Some(end),
            entry_price: 100.0,
            exit_price: Some(105.0),
            quantity: 1.0,
            pnl: Some(pnl),
            pnl_bps: Some(500.0),
            fee_total: 1.0,
            holding_period_sec: Some(3600),
            signal_confidence: None,
            signal_strength: None,
            entry_signal_id: None,
            exit_reason: None,
            result: Some(if pnl > 0.0 {
                "win".into()
            } else {
                "loss".into()
            }),
            market_regime_at_entry: Some(regime.into()),
        };

        let trades = vec![
            make_trade("trending_bull", 100.0),
            make_trade("trending_bull", 50.0),
            make_trade("ranging", -30.0),
            make_trade("crisis", -200.0),
        ];

        let report = PerformanceEngine.compute_report(trades, equity, start, end, 10.0, 2.5);
        let by_regime = report.by_regime.as_object().unwrap();
        assert!(by_regime.contains_key("trending_bull"));
        assert!(by_regime.contains_key("ranging"));
        assert!(by_regime.contains_key("crisis"));

        // trending_bull: 2 笔交易，pnl = 150
        let bull = &by_regime["trending_bull"];
        assert_eq!(bull["trades"], 2);
        assert_eq!(bull["pnl"], 150);

        // ranging: 1 笔交易，pnl = -30
        let ranging = &by_regime["ranging"];
        assert_eq!(ranging["trades"], 1);
        assert_eq!(ranging["pnl"], -30);

        // crisis: 1 笔交易，pnl = -200
        let crisis = &by_regime["crisis"];
        assert_eq!(crisis["trades"], 1);
        assert_eq!(crisis["pnl"], -200);
    }

    #[test]
    fn test_by_regime_unknown_when_no_regime() {
        // 验证无 regime 信息的交易被归入 "unknown"
        let start = ts(0);
        let end = ts(10);
        let equity = make_equity_curve(&[10000.0, 10500.0]);

        let trade = TradeAttribution {
            attribution_id: Uuid::new_v4(),
            job_id: None,
            asset: "BTC-USDT-SWAP".into(),
            strategy_id: None,
            agent_id: None,
            direction: "long".into(),
            entry_time: start,
            exit_time: Some(end),
            entry_price: 100.0,
            exit_price: Some(105.0),
            quantity: 1.0,
            pnl: Some(50.0),
            pnl_bps: Some(500.0),
            fee_total: 1.0,
            holding_period_sec: Some(3600),
            signal_confidence: None,
            signal_strength: None,
            entry_signal_id: None,
            exit_reason: None,
            result: Some("win".into()),
            market_regime_at_entry: None,
        };

        let report = PerformanceEngine.compute_report(vec![trade], equity, start, end, 0.0, 0.0);
        let by_regime = report.by_regime.as_object().unwrap();
        assert!(by_regime.contains_key("unknown"));
        assert_eq!(by_regime["unknown"]["trades"], 1);
    }
}
