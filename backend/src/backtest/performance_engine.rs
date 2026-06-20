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

    /// Compute Sharpe from daily returns.
    fn sharpe_ratio(equity: &[(DateTime<Utc>, f64)]) -> f64 {
        if equity.len() < 2 {
            return 0.0;
        }
        // Compute daily returns from equity curve.
        let mut daily: HashMap<chrono::NaiveDate, (f64, f64)> = HashMap::new();
        for (ts, eq) in equity {
            let day = ts.date_naive();
            let entry = daily.entry(day).or_insert((f64::INFINITY, f64::NEG_INFINITY));
            entry.0 = entry.0.min(*eq);
            entry.1 = entry.1.max(*eq);
        }
        let mut days: Vec<chrono::NaiveDate> = daily.keys().copied().collect();
        days.sort();
        if days.len() < 2 {
            return 0.0;
        }
        let mut rets = Vec::with_capacity(days.len() - 1);
        for i in 1..days.len() {
            let prev = daily[&days[i - 1]].1; // last peak of previous day as proxy end
            let curr = daily[&days[i]].1;
            if prev > 0.0 {
                rets.push((curr - prev) / prev);
            }
        }
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

    /// Max drawdown (percent) + peak-to-valley.
    fn max_drawdown(equity: &[(DateTime<Utc>, f64)]) -> (f64, Vec<(DateTime<Utc>, f64)>) {
        let mut peak = f64::NEG_INFINITY;
        let mut max_dd = 0.0_f64;
        let mut drawdown_curve = Vec::with_capacity(equity.len());
        for (ts, eq) in equity {
            if *eq > peak {
                peak = *eq;
            }
            let dd = if peak > 0.0 { (peak - eq) / peak } else { 0.0 };
            if dd > max_dd {
                max_dd = dd;
            }
            drawdown_curve.push((*ts, dd));
        }
        (max_dd, drawdown_curve)
    }

    pub fn compute_report(
        &self,
        trades: Vec<TradeAttribution>,
        equity_curve: Vec<(DateTime<Utc>, f64)>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        total_fee: f64,
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

        let (max_drawdown, drawdown_curve) = Self::max_drawdown(&equity_curve);
        let sharpe = Self::sharpe_ratio(&equity_curve);

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
        let gross_loss: f64 = losses.iter().map(|t| (-t.pnl.unwrap_or(0.0)).max(0.0)).sum();
        let profit_factor = if gross_loss > 0.0 { gross_profit / gross_loss } else { gross_profit };
        let average_win = if winning_trades > 0 { gross_profit / winning_trades as f64 } else { 0.0 };
        let average_loss = if losing_trades > 0 { gross_loss / losing_trades as f64 } else { 0.0 };
        let payoff_ratio = if average_loss > 0.0 { average_win / average_loss } else { average_win };

        // attribution by agent / asset
        let mut by_agent: HashMap<String, (i64, f64)> = HashMap::new();
        let mut by_asset: HashMap<String, (i64, f64)> = HashMap::new();
        for t in &trades {
            let agent = t.agent_id.clone().unwrap_or_else(|| "unknown".into());
            let entry = by_agent.entry(agent).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += t.pnl.unwrap_or(0.0);
            let entry2 = by_asset.entry(t.asset.clone()).or_insert((0, 0.0));
            entry2.0 += 1;
            entry2.1 += t.pnl.unwrap_or(0.0);
        }

        let to_json = |m: HashMap<String, (i64, f64)>| -> Value {
            let map: serde_json::Map<String, Value> = m
                .into_iter()
                .map(|(k, (cnt, pnl))| {
                    (k, json!({ "trades": cnt, "pnl": pnl.round() as i64 }))
                })
                .collect();
            Value::Object(map)
        };

        PerformanceReport {
            total_return,
            annualized_return: annualized,
            max_drawdown,
            sharpe_ratio: sharpe,
            sortino_ratio: 0.0, // simplified: leave for later
            win_rate,
            profit_factor,
            total_trades,
            winning_trades,
            losing_trades,
            average_win,
            average_loss,
            payoff_ratio,
            total_fee,
            total_slippage_cost: 0.0,
            equity_curve,
            drawdown_curve,
            trades,
            by_agent: to_json(by_agent),
            by_asset: to_json(by_asset),
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
        "win_rate": report.win_rate,
        "profit_factor": report.profit_factor,
        "total_trades": report.total_trades,
        "winning_trades": report.winning_trades,
        "losing_trades": report.losing_trades,
        "average_win": report.average_win,
        "average_loss": report.average_loss,
        "payoff_ratio": report.payoff_ratio,
        "total_fee": report.total_fee,
        "equity_points": report.equity_curve.len(),
    })
}

/// Format duration as human-readable.
pub fn fmt_duration(d: Duration) -> String {
    let days = d.num_days();
    let hours = (d - Duration::days(days)).num_hours();
    format!("{}d {}h", days, hours)
}
