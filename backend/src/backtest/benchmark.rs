//! Benchmark Strategies
//! 基准策略对比
//!
//! 依据《系统评估与演进规划》第二阶段验收标准：
//! "样本外测试显著优于简单基准"
//!
//! 提供简单基准策略，用于与策略回测结果对比：
//! 1. Buy and Hold：买入持有
//! 2. Always Long：永远看涨（类似 Buy and Hold 但含手续费）
//! 3. Trend Following：趋势延续（均线交叉）
//! 4. Random Entry：随机入场（统计显著性基线）

use crate::backtest::models::{Kline, PerformanceReport, TradeAttribution};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 基准策略类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkType {
    /// 买入持有
    BuyAndHold,
    /// 永远看涨（含手续费）
    AlwaysLong,
    /// 趋势延续（均线交叉）
    TrendFollowing,
    /// 随机入场
    RandomEntry,
}

impl BenchmarkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BenchmarkType::BuyAndHold => "buy_and_hold",
            BenchmarkType::AlwaysLong => "always_long",
            BenchmarkType::TrendFollowing => "trend_following",
            BenchmarkType::RandomEntry => "random_entry",
        }
    }
}

/// 基准策略绩效摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// 基准类型
    pub benchmark: BenchmarkType,
    /// 标的
    pub symbol: String,
    /// 总收益
    pub total_return: f64,
    /// 夏普率
    pub sharpe_ratio: f64,
    /// 最大回撤
    pub max_drawdown: f64,
    /// 交易笔数
    pub trades: i64,
    /// 胜率
    pub win_rate: f64,
}

/// 策略与基准的对比结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    /// 策略绩效摘要
    pub strategy: StrategySummary,
    /// 各基准绩效
    pub benchmarks: Vec<BenchmarkResult>,
    /// 策略是否优于所有基准
    pub beats_all_benchmarks: bool,
    /// 策略优于基准的数量
    pub benchmarks_beaten: usize,
    /// 总基准数量
    pub total_benchmarks: usize,
    /// Alpha（相对于最佳基准的超额收益）
    pub alpha_vs_best: f64,
    /// 最佳基准名称
    pub best_benchmark: String,
}

/// 策略绩效摘要（从 PerformanceReport 提取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySummary {
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub trades: i64,
    pub win_rate: f64,
}

impl StrategySummary {
    pub fn from_report(report: &PerformanceReport) -> Self {
        Self {
            total_return: report.total_return,
            sharpe_ratio: report.sharpe_ratio,
            max_drawdown: report.max_drawdown,
            trades: report.total_trades,
            win_rate: report.win_rate,
        }
    }
}

/// 基准策略引擎
pub struct BenchmarkEngine;

impl BenchmarkEngine {
    /// 计算 Buy and Hold 基准收益
    ///
    /// 买入持有：在区间起始买入，区间结束卖出
    /// 收益 = (end_price - start_price) / start_price
    pub fn buy_and_hold(klines: &[Kline], symbol: &str) -> BenchmarkResult {
        let prices: Vec<&Kline> = klines.iter().filter(|k| k.symbol == symbol).collect();
        if prices.is_empty() {
            return Self::empty_benchmark(BenchmarkType::BuyAndHold, symbol);
        }
        let start_price = prices.first().unwrap().open;
        let end_price = prices.last().unwrap().close;
        let total_return = if start_price > 0.0 {
            (end_price - start_price) / start_price
        } else {
            0.0
        };
        let sharpe = Self::simple_sharpe(&prices.iter().map(|k| k.close).collect::<Vec<_>>());
        let max_dd = Self::max_drawdown(&prices.iter().map(|k| k.close).collect::<Vec<_>>());

        BenchmarkResult {
            benchmark: BenchmarkType::BuyAndHold,
            symbol: symbol.to_string(),
            total_return,
            sharpe_ratio: sharpe,
            max_drawdown: max_dd,
            trades: 1,
            win_rate: if total_return > 0.0 { 1.0 } else { 0.0 },
        }
    }

    /// 计算 Always Long 基准收益（含手续费）
    ///
    /// 与 Buy and Hold 类似，但模拟实际交易的手续费
    pub fn always_long(klines: &[Kline], symbol: &str, fee_bps: f64) -> BenchmarkResult {
        let mut result = Self::buy_and_hold(klines, symbol);
        result.benchmark = BenchmarkType::AlwaysLong;
        // 扣除开仓和平仓手续费
        let fee_cost = 2.0 * fee_bps / 10000.0;
        result.total_return -= fee_cost;
        result.trades = 2;
        result
    }

    /// 计算 Trend Following 基准收益（简单均线交叉）
    ///
    /// SMA(7) 上穿 SMA(25) 买入，SMA(7) 下穿 SMA(25) 卖出
    pub fn trend_following(klines: &[Kline], symbol: &str, fee_bps: f64) -> BenchmarkResult {
        let prices: Vec<&Kline> = klines.iter().filter(|k| k.symbol == symbol).collect();
        if prices.len() < 25 {
            return Self::empty_benchmark(BenchmarkType::TrendFollowing, symbol);
        }

        let closes: Vec<f64> = prices.iter().map(|k| k.close).collect();
        let sma_short = Self::sma(&closes, 7);
        let sma_long = Self::sma(&closes, 25);

        let mut position = false;
        let mut entry_price = 0.0;
        let mut trades: i64 = 0;
        let mut wins: i64 = 0;
        let mut total_pnl = 0.0;
        let mut equity_curve: Vec<f64> = Vec::new();
        let mut current_equity = 1.0; // 归一化为 1.0

        for i in 25..closes.len() {
            let prev_short = sma_short[i - 1];
            let curr_short = sma_short[i];
            let prev_long = sma_long[i - 1];
            let curr_long = sma_long[i];

            // 金叉买入
            if !position && prev_short <= prev_long && curr_short > curr_long {
                position = true;
                entry_price = closes[i];
                trades += 1;
            }
            // 死叉卖出
            else if position && prev_short >= prev_long && curr_short < curr_long {
                position = false;
                let exit_price = closes[i];
                let pnl = (exit_price - entry_price) / entry_price - 2.0 * fee_bps / 10000.0;
                total_pnl += pnl;
                if pnl > 0.0 {
                    wins += 1;
                }
                trades += 1;
                entry_price = 0.0;
            }

            // 更新权益曲线
            if position {
                let unrealized = (closes[i] - entry_price) / entry_price;
                current_equity = 1.0 + total_pnl + unrealized;
            } else {
                current_equity = 1.0 + total_pnl;
            }
            equity_curve.push(current_equity);
        }

        // 如果最后还持仓，按最后价格平仓
        if position && entry_price > 0.0 {
            let exit_price = *closes.last().unwrap();
            let pnl = (exit_price - entry_price) / entry_price - 2.0 * fee_bps / 10000.0;
            total_pnl += pnl;
            if pnl > 0.0 {
                wins += 1;
            }
            trades += 1;
        }

        let sharpe = Self::simple_sharpe(&equity_curve);
        let max_dd = Self::max_drawdown(&equity_curve);
        let win_rate = if trades > 0 {
            wins as f64 / trades as f64
        } else {
            0.0
        };

        BenchmarkResult {
            benchmark: BenchmarkType::TrendFollowing,
            symbol: symbol.to_string(),
            total_return: total_pnl,
            sharpe_ratio: sharpe,
            max_drawdown: max_dd,
            trades,
            win_rate,
        }
    }

    /// 计算 Random Entry 基准收益
    ///
    /// 使用固定种子随机入场，作为统计显著性基线
    pub fn random_entry(klines: &[Kline], symbol: &str, fee_bps: f64, seed: u64) -> BenchmarkResult {
        let prices: Vec<&Kline> = klines.iter().filter(|k| k.symbol == symbol).collect();
        if prices.len() < 10 {
            return Self::empty_benchmark(BenchmarkType::RandomEntry, symbol);
        }

        let closes: Vec<f64> = prices.iter().map(|k| k.close).collect();
        // 简单 LCG 随机数生成器（确定性，可复现）
        let mut rng_state = seed.max(1);
        let mut next_rand = || {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng_state >> 33) as f64 / (1u64 << 31) as f64
        };

        let mut position = false;
        let mut entry_price = 0.0;
        let mut trades: i64 = 0;
        let mut wins: i64 = 0;
        let mut total_pnl = 0.0;
        let mut equity_curve: Vec<f64> = Vec::new();
        let mut current_equity = 1.0;

        for i in 5..closes.len() {
            let r = next_rand();
            // 10% 概率入场，10% 概率平仓
            if !position && r < 0.1 {
                position = true;
                entry_price = closes[i];
                trades += 1;
            } else if position && r < 0.1 {
                position = false;
                let exit_price = closes[i];
                let pnl = (exit_price - entry_price) / entry_price - 2.0 * fee_bps / 10000.0;
                total_pnl += pnl;
                if pnl > 0.0 {
                    wins += 1;
                }
                trades += 1;
                entry_price = 0.0;
            }

            if position {
                let unrealized = (closes[i] - entry_price) / entry_price;
                current_equity = 1.0 + total_pnl + unrealized;
            } else {
                current_equity = 1.0 + total_pnl;
            }
            equity_curve.push(current_equity);
        }

        if position && entry_price > 0.0 {
            let exit_price = *closes.last().unwrap();
            let pnl = (exit_price - entry_price) / entry_price - 2.0 * fee_bps / 10000.0;
            total_pnl += pnl;
            if pnl > 0.0 {
                wins += 1;
            }
            trades += 1;
        }

        let sharpe = Self::simple_sharpe(&equity_curve);
        let max_dd = Self::max_drawdown(&equity_curve);
        let win_rate = if trades > 0 {
            wins as f64 / trades as f64
        } else {
            0.0
        };

        BenchmarkResult {
            benchmark: BenchmarkType::RandomEntry,
            symbol: symbol.to_string(),
            total_return: total_pnl,
            sharpe_ratio: sharpe,
            max_drawdown: max_dd,
            trades,
            win_rate,
        }
    }

    /// 生成所有基准策略并与策略对比
    pub fn compare_all(
        report: &PerformanceReport,
        klines: &[Kline],
        symbol: &str,
        fee_bps: f64,
    ) -> BenchmarkComparison {
        let strategy = StrategySummary::from_report(report);

        let benchmarks = vec![
            Self::buy_and_hold(klines, symbol),
            Self::always_long(klines, symbol, fee_bps),
            Self::trend_following(klines, symbol, fee_bps),
            Self::random_entry(klines, symbol, fee_bps, 42),
        ];

        // 策略优于基准的判断：总收益更高
        let strategy_return = strategy.total_return;
        let benchmarks_beaten = benchmarks
            .iter()
            .filter(|b| strategy_return > b.total_return)
            .count();

        // 找最佳基准
        let best_benchmark = benchmarks
            .iter()
            .max_by(|a, b| {
                a.total_return
                    .partial_cmp(&b.total_return)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
            .unwrap_or_else(|| Self::empty_benchmark(BenchmarkType::BuyAndHold, symbol));

        let alpha_vs_best = strategy_return - best_benchmark.total_return;

        BenchmarkComparison {
            strategy,
            benchmarks,
            beats_all_benchmarks: benchmarks_beaten == 4,
            benchmarks_beaten,
            total_benchmarks: 4,
            alpha_vs_best,
            best_benchmark: best_benchmark.benchmark.as_str().to_string(),
        }
    }

    // ===== 辅助计算 =====

    fn empty_benchmark(b: BenchmarkType, symbol: &str) -> BenchmarkResult {
        BenchmarkResult {
            benchmark: b,
            symbol: symbol.to_string(),
            total_return: 0.0,
            sharpe_ratio: 0.0,
            max_drawdown: 0.0,
            trades: 0,
            win_rate: 0.0,
        }
    }

    fn sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = vec![0.0; data.len()];
        if data.len() < period {
            return result;
        }
        let mut sum: f64 = data[..period].iter().sum();
        result[period - 1] = sum / period as f64;
        for i in period..data.len() {
            sum += data[i] - data[i - period];
            result[i] = sum / period as f64;
        }
        result
    }

    fn simple_sharpe(equity: &[f64]) -> f64 {
        if equity.len() < 2 {
            return 0.0;
        }
        let rets: Vec<f64> = equity
            .windows(2)
            .map(|w| {
                if w[0] > 0.0 {
                    (w[1] - w[0]) / w[0]
                } else {
                    0.0
                }
            })
            .collect();
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

    fn max_drawdown(equity: &[f64]) -> f64 {
        let mut peak = f64::NEG_INFINITY;
        let mut max_dd = 0.0_f64;
        for &e in equity {
            if e > peak {
                peak = e;
            }
            let dd = if peak > 0.0 { (peak - e) / peak } else { 0.0 };
            if dd > max_dd {
                max_dd = dd;
            }
        }
        max_dd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_klines(symbol: &str, prices: &[f64]) -> Vec<Kline> {
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        prices
            .iter()
            .enumerate()
            .map(|(i, &p)| Kline {
                symbol: symbol.into(),
                interval: "1H".into(),
                open_time: base + chrono::Duration::hours(i as i64),
                open: p,
                high: p * 1.01,
                low: p * 0.99,
                close: p,
                volume: 1000.0,
                quote_volume: Some(1000.0 * p),
            })
            .collect()
    }

    #[test]
    fn test_buy_and_hold_positive() {
        let klines = make_klines("BTC", &[100.0, 105.0, 110.0, 108.0, 115.0]);
        let result = BenchmarkEngine::buy_and_hold(&klines, "BTC");
        assert!(result.total_return > 0.0);
        assert_eq!(result.trades, 1);
        assert_eq!(result.win_rate, 1.0);
    }

    #[test]
    fn test_buy_and_hold_negative() {
        let klines = make_klines("BTC", &[100.0, 95.0, 90.0, 88.0, 85.0]);
        let result = BenchmarkEngine::buy_and_hold(&klines, "BTC");
        assert!(result.total_return < 0.0);
        assert_eq!(result.win_rate, 0.0);
    }

    #[test]
    fn test_buy_and_hold_empty() {
        let klines: Vec<Kline> = vec![];
        let result = BenchmarkEngine::buy_and_hold(&klines, "BTC");
        assert_eq!(result.total_return, 0.0);
        assert_eq!(result.trades, 0);
    }

    #[test]
    fn test_always_long_deducts_fee() {
        let klines = make_klines("BTC", &[100.0, 100.0, 100.0, 100.0, 100.0]);
        let bh = BenchmarkEngine::buy_and_hold(&klines, "BTC");
        let al = BenchmarkEngine::always_long(&klines, "BTC", 5.0);
        // 价格不变时，always_long 应因手续费而亏损
        assert!(al.total_return < bh.total_return);
        assert!(al.total_return < 0.0);
    }

    #[test]
    fn test_trend_following_generates_trades() {
        // 构造一个有趋势变化的价格序列
        let mut prices: Vec<f64> = Vec::new();
        // 先下跌（触发空头）
        for i in 0..15 {
            prices.push(100.0 - i as f64);
        }
        // 再上涨（触发多头）
        for i in 0..20 {
            prices.push(85.0 + i as f64);
        }
        let klines = make_klines("BTC", &prices);
        let result = BenchmarkEngine::trend_following(&klines, "BTC", 5.0);
        assert!(result.trades > 0, "趋势策略应产生交易");
    }

    #[test]
    fn test_trend_following_insufficient_data() {
        let klines = make_klines("BTC", &[100.0, 101.0, 102.0]);
        let result = BenchmarkEngine::trend_following(&klines, "BTC", 5.0);
        assert_eq!(result.trades, 0);
    }

    #[test]
    fn test_random_entry_deterministic() {
        let klines = make_klines("BTC", &(0..50).map(|i| 100.0 + i as f64 * 0.5).collect::<Vec<_>>());
        let r1 = BenchmarkEngine::random_entry(&klines, "BTC", 5.0, 42);
        let r2 = BenchmarkEngine::random_entry(&klines, "BTC", 5.0, 42);
        // 相同种子应产生相同结果
        assert_eq!(r1.trades, r2.trades);
        assert_eq!(r1.total_return, r2.total_return);
    }

    #[test]
    fn test_compare_all() {
        let klines = make_klines("BTC", &(0..50).map(|i| 100.0 + i as f64 * 0.5).collect::<Vec<_>>());
        let report = PerformanceReport {
            total_return: 0.5,
            annualized_return: 0.5,
            max_drawdown: 0.05,
            sharpe_ratio: 2.0,
            sortino_ratio: 2.5,
            calmar_ratio: 10.0,
            win_rate: 0.7,
            profit_factor: 2.0,
            total_trades: 20,
            winning_trades: 14,
            losing_trades: 6,
            average_win: 100.0,
            average_loss: 50.0,
            payoff_ratio: 2.0,
            total_fee: 10.0,
            total_slippage_cost: 5.0,
            var_95: 0.0,
            cvar_95: 0.0,
            alpha: 0.0,
            beta: 0.0,
            max_drawdown_duration_sec: 0,
            equity_curve: vec![],
            drawdown_curve: vec![],
            trades: vec![],
            by_agent: serde_json::json!({}),
            by_asset: serde_json::json!({}),
            by_regime: serde_json::json!({}),
        };
        let comparison = BenchmarkEngine::compare_all(&report, &klines, "BTC", 5.0);
        assert_eq!(comparison.total_benchmarks, 4);
        assert!(comparison.benchmarks.len() == 4);
        // 策略收益 50%，应优于大部分基准
        assert!(comparison.benchmarks_beaten >= 1);
    }

    #[test]
    fn test_sma_calculation() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let sma = BenchmarkEngine::sma(&data, 3);
        assert_eq!(sma.len(), 5);
        assert_eq!(sma[2], 2.0); // (1+2+3)/3
        assert_eq!(sma[4], 4.0); // (3+4+5)/3
    }

    #[test]
    fn test_simple_sharpe() {
        let equity = vec![1.0, 1.01, 1.02, 1.03, 1.04];
        let sharpe = BenchmarkEngine::simple_sharpe(&equity);
        assert!(sharpe > 0.0);
    }

    #[test]
    fn test_max_drawdown() {
        let equity = vec![1.0, 1.2, 0.9, 1.1];
        let dd = BenchmarkEngine::max_drawdown(&equity);
        // 最大回撤 = (1.2 - 0.9) / 1.2 = 0.25
        assert!((dd - 0.25).abs() < 1e-9);
    }
}
