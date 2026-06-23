//! Walk-forward Validation
//! 样本外滚动验证
//!
//! 依据《系统评估与演进规划》第二阶段任务 4：
//! 使用 Walk-forward 样本外验证，防止过拟合。
//!
//! 核心思想：
//! 1. 将总时间区间划分为多个滚动窗口
//! 2. 每个窗口分为训练集（in-sample）和测试集（out-of-sample）
//! 3. 在训练集和测试集之间插入 purge（清洗期）和 embargo（禁运期）
//!    防止训练集末尾数据泄露到测试集
//! 4. 仅在测试集上评估绩效，汇总所有窗口的样本外结果
//!
//! 参考：Marcos López de Prado - "Advances in Financial Machine Learning"
//! 中的 Combinatorial Purged Cross-Validation 思想

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Walk-forward 验证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardConfig {
    /// 训练窗口长度（天）
    pub train_window_days: i64,
    /// 测试窗口长度（天）
    pub test_window_days: i64,
    /// 步进长度（天），即每次滚动前进的天数
    /// 默认等于 test_window_days（无重叠）
    pub step_days: i64,
    /// 清洗期长度（天）：训练集和测试集之间的隔离期
    /// 防止训练集末尾标签泄露到测试集
    pub purge_days: i64,
    /// 禁运期长度（天）：测试集后的观察禁止期
    /// 防止测试集的标签（如持有期收益）泄露到下一个训练集
    pub embargo_days: i64,
}

impl Default for WalkForwardConfig {
    fn default() -> Self {
        Self {
            train_window_days: 90,  // 90 天训练
            test_window_days: 30,   // 30 天测试
            step_days: 30,          // 每次滚动 30 天
            purge_days: 1,           // 1 天清洗期
            embargo_days: 1,         // 1 天禁运期
        }
    }
}

/// 单个 Walk-forward 窗口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardWindow {
    /// 窗口序号（从 0 开始）
    pub window_index: usize,
    /// 训练集起始时间
    pub train_start: DateTime<Utc>,
    /// 训练集结束时间
    pub train_end: DateTime<Utc>,
    /// 测试集起始时间（已含 purge 间隔）
    pub test_start: DateTime<Utc>,
    /// 测试集结束时间
    pub test_end: DateTime<Utc>,
}

/// Walk-forward 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardResult {
    /// 配置
    pub config: WalkForwardConfig,
    /// 所有窗口
    pub windows: Vec<WalkForwardWindow>,
    /// 每个窗口的样本外绩效摘要
    pub oos_metrics: Vec<OutOfSampleMetrics>,
    /// 汇总的样本外统计
    pub aggregate: AggregateMetrics,
}

/// 单个窗口的样本外绩效
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutOfSampleMetrics {
    pub window_index: usize,
    pub test_start: DateTime<Utc>,
    pub test_end: DateTime<Utc>,
    /// 样本外交易数
    pub trades: i64,
    /// 样本外总收益
    pub total_return: f64,
    /// 样本外夏普率
    pub sharpe_ratio: f64,
    /// 样本外最大回撤
    pub max_drawdown: f64,
    /// 样本外胜率
    pub win_rate: f64,
}

/// 汇总统计（所有窗口的样本外结果）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateMetrics {
    /// 总窗口数
    pub num_windows: usize,
    /// 总样本外交易数
    pub total_trades: i64,
    /// 平均样本外收益
    pub avg_return: f64,
    /// 收益标准差（跨窗口）
    pub std_return: f64,
    /// 平均夏普率
    pub avg_sharpe: f64,
    /// 最差窗口收益
    pub worst_return: f64,
    /// 最佳窗口收益
    pub best_return: f64,
    /// 正收益窗口占比
    pub positive_window_ratio: f64,
    /// 平均最大回撤
    pub avg_max_drawdown: f64,
    /// 平均胜率
    pub avg_win_rate: f64,
    /// 策略稳定性评分：正收益窗口占比 × 平均收益 / 收益标准差
    /// 越高表示策略在不同市场环境下越稳定
    pub stability_score: f64,
}

/// Walk-forward 验证引擎
pub struct WalkForwardEngine {
    config: WalkForwardConfig,
}

impl WalkForwardEngine {
    pub fn new(config: WalkForwardConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(WalkForwardConfig::default())
    }

    /// 生成 Walk-forward 窗口划分
    ///
    /// 给定总时间区间 [start, end]，生成多个滚动窗口。
    /// 每个窗口包含训练集和测试集，之间有 purge 和 embargo 间隔。
    pub fn generate_windows(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<WalkForwardWindow> {
        let mut windows = Vec::new();
        let total_days = (end - start).num_days();
        let min_required = self.config.train_window_days
            + self.config.purge_days
            + self.config.test_window_days;

        if total_days < min_required {
            return windows;
        }

        let step = if self.config.step_days > 0 {
            self.config.step_days
        } else {
            self.config.test_window_days
        };

        let mut train_start = start;
        let mut window_index = 0;

        loop {
            let train_end = train_start + Duration::days(self.config.train_window_days);
            // purge 间隔
            let test_start = train_end + Duration::days(self.config.purge_days);
            let test_end = test_start + Duration::days(self.config.test_window_days);

            // 测试集结束时间不能超过总区间
            if test_end > end {
                break;
            }

            windows.push(WalkForwardWindow {
                window_index,
                train_start,
                train_end,
                test_start,
                test_end,
            });

            window_index += 1;
            // 滚动到下一个窗口（考虑 embargo：下一个训练集从测试集结束 + embargo 后开始）
            train_start = test_end + Duration::days(self.config.embargo_days);

            // 如果下一个窗口的训练集结束时间已超过总区间，停止
            let next_train_end =
                train_start + Duration::days(self.config.train_window_days);
            if next_train_end > end {
                break;
            }
        }

        windows
    }

    /// 汇总所有窗口的样本外绩效
    pub fn aggregate_metrics(&self, oos: &[OutOfSampleMetrics]) -> AggregateMetrics {
        if oos.is_empty() {
            return AggregateMetrics::default();
        }

        let n = oos.len() as f64;
        let returns: Vec<f64> = oos.iter().map(|m| m.total_return).collect();
        let sharpes: Vec<f64> = oos.iter().map(|m| m.sharpe_ratio).collect();
        let drawdowns: Vec<f64> = oos.iter().map(|m| m.max_drawdown).collect();
        let win_rates: Vec<f64> = oos.iter().map(|m| m.win_rate).collect();

        let total_trades: i64 = oos.iter().map(|m| m.trades).sum();
        let avg_return = returns.iter().sum::<f64>() / n;
        let variance = if returns.len() > 1 {
            returns.iter().map(|r| (r - avg_return).powi(2)).sum::<f64>() / n
        } else {
            0.0
        };
        let std_return = variance.sqrt();
        let avg_sharpe = sharpes.iter().sum::<f64>() / n;
        let worst_return = returns.iter().cloned().fold(f64::INFINITY, f64::min);
        let best_return = returns
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let positive_count = returns.iter().filter(|&&r| r > 0.0).count();
        let positive_window_ratio = positive_count as f64 / n;
        let avg_max_drawdown = drawdowns.iter().sum::<f64>() / n;
        let avg_win_rate = win_rates.iter().sum::<f64>() / n;

        // 稳定性评分：正收益窗口占比 × 平均收益 / (收益标准差 + 1e-9)
        let stability_score = if std_return > 1e-9 {
            positive_window_ratio * avg_return / std_return
        } else {
            positive_window_ratio * avg_return * 100.0 // 标准差极小时放大
        };

        AggregateMetrics {
            num_windows: oos.len(),
            total_trades,
            avg_return,
            std_return,
            avg_sharpe,
            worst_return,
            best_return,
            positive_window_ratio,
            avg_max_drawdown,
            avg_win_rate,
            stability_score,
        }
    }

    /// 判断策略是否通过 Walk-forward 验证
    ///
    /// 通过条件：
    /// 1. 至少 3 个窗口（统计显著性）
    /// 2. 正收益窗口占比 > 50%
    /// 3. 平均收益 > 0
    /// 4. 稳定性评分 > 0
    pub fn is_validated(&self, result: &WalkForwardResult) -> bool {
        let agg = &result.aggregate;
        agg.num_windows >= 3
            && agg.positive_window_ratio > 0.5
            && agg.avg_return > 0.0
            && agg.stability_score > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(days: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::days(days)
    }

    #[test]
    fn test_generate_windows_basic() {
        let engine = WalkForwardEngine::new(WalkForwardConfig {
            train_window_days: 30,
            test_window_days: 10,
            step_days: 10,
            purge_days: 1,
            embargo_days: 1,
        });
        let windows = engine.generate_windows(ts(0), ts(100));
        // 100 天 = 30(train) + 1(purge) + 10(test) + 1(embargo) + 30(train) + 1 + 10 + 1 + ...
        // 第一窗口: [0,30] train, [31,41] test
        // embargo: 42
        // 第二窗口: [42,72] train, [73,83] test
        // embargo: 84
        // 第三窗口: [84,114] train -> 超过 100，停止
        assert!(windows.len() >= 2, "应至少生成 2 个窗口，实际: {}", windows.len());
    }

    #[test]
    fn test_generate_windows_insufficient_data() {
        let engine = WalkForwardEngine::with_defaults();
        // 默认需要 90+1+30=121 天，只给 50 天
        let windows = engine.generate_windows(ts(0), ts(50));
        assert!(windows.is_empty(), "数据不足应返回空窗口");
    }

    #[test]
    fn test_window_purge_gap() {
        let engine = WalkForwardEngine::new(WalkForwardConfig {
            train_window_days: 30,
            test_window_days: 10,
            step_days: 10,
            purge_days: 5,
            embargo_days: 0,
        });
        let windows = engine.generate_windows(ts(0), ts(200));
        assert!(!windows.is_empty());
        let w = &windows[0];
        // train_end + purge = test_start
        let gap = (w.test_start - w.train_end).num_days();
        assert_eq!(gap, 5, "purge 间隔应为 5 天");
    }

    #[test]
    fn test_window_embargo_between_windows() {
        let engine = WalkForwardEngine::new(WalkForwardConfig {
            train_window_days: 30,
            test_window_days: 10,
            step_days: 10,
            purge_days: 0,
            embargo_days: 3,
        });
        let windows = engine.generate_windows(ts(0), ts(200));
        assert!(windows.len() >= 2);
        // 第二窗口的 train_start = 第一窗口的 test_end + embargo
        let gap = (windows[1].train_start - windows[0].test_end).num_days();
        assert_eq!(gap, 3, "embargo 间隔应为 3 天");
    }

    #[test]
    fn test_aggregate_metrics_all_positive() {
        let engine = WalkForwardEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(30),
                trades: 10,
                total_return: 0.05,
                sharpe_ratio: 1.5,
                max_drawdown: 0.03,
                win_rate: 0.6,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(30),
                test_end: ts(60),
                trades: 12,
                total_return: 0.08,
                sharpe_ratio: 1.8,
                max_drawdown: 0.04,
                win_rate: 0.58,
            },
            OutOfSampleMetrics {
                window_index: 2,
                test_start: ts(60),
                test_end: ts(90),
                trades: 8,
                total_return: 0.03,
                sharpe_ratio: 1.2,
                max_drawdown: 0.02,
                win_rate: 0.62,
            },
        ];
        let agg = engine.aggregate_metrics(&oos);
        assert_eq!(agg.num_windows, 3);
        assert!(agg.avg_return > 0.0);
        assert_eq!(agg.positive_window_ratio, 1.0);
        assert!(agg.stability_score > 0.0);
    }

    #[test]
    fn test_aggregate_metrics_mixed() {
        let engine = WalkForwardEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(30),
                trades: 10,
                total_return: 0.05,
                sharpe_ratio: 1.5,
                max_drawdown: 0.03,
                win_rate: 0.6,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(30),
                test_end: ts(60),
                trades: 12,
                total_return: -0.03,
                sharpe_ratio: -0.5,
                max_drawdown: 0.06,
                win_rate: 0.4,
            },
        ];
        let agg = engine.aggregate_metrics(&oos);
        assert_eq!(agg.positive_window_ratio, 0.5);
        assert!(agg.worst_return < 0.0);
        assert!(agg.best_return > 0.0);
    }

    #[test]
    fn test_is_validated_pass() {
        let engine = WalkForwardEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(30),
                trades: 10,
                total_return: 0.05,
                sharpe_ratio: 1.5,
                max_drawdown: 0.03,
                win_rate: 0.6,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(30),
                test_end: ts(60),
                trades: 12,
                total_return: 0.08,
                sharpe_ratio: 1.8,
                max_drawdown: 0.04,
                win_rate: 0.58,
            },
            OutOfSampleMetrics {
                window_index: 2,
                test_start: ts(60),
                test_end: ts(90),
                trades: 8,
                total_return: 0.03,
                sharpe_ratio: 1.2,
                max_drawdown: 0.02,
                win_rate: 0.62,
            },
        ];
        let agg = engine.aggregate_metrics(&oos);
        let result = WalkForwardResult {
            config: WalkForwardConfig::default(),
            windows: vec![],
            oos_metrics: oos,
            aggregate: agg,
        };
        assert!(engine.is_validated(&result), "3 个正收益窗口应通过验证");
    }

    #[test]
    fn test_is_validated_fail_insufficient_windows() {
        let engine = WalkForwardEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(30),
                trades: 10,
                total_return: 0.05,
                sharpe_ratio: 1.5,
                max_drawdown: 0.03,
                win_rate: 0.6,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(30),
                test_end: ts(60),
                trades: 12,
                total_return: 0.08,
                sharpe_ratio: 1.8,
                max_drawdown: 0.04,
                win_rate: 0.58,
            },
        ];
        let agg = engine.aggregate_metrics(&oos);
        let result = WalkForwardResult {
            config: WalkForwardConfig::default(),
            windows: vec![],
            oos_metrics: oos,
            aggregate: agg,
        };
        // 不足 3 个窗口
        assert!(!engine.is_validated(&result));
    }

    #[test]
    fn test_is_validated_fail_negative_return() {
        let engine = WalkForwardEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(30),
                trades: 10,
                total_return: -0.05,
                sharpe_ratio: -0.5,
                max_drawdown: 0.08,
                win_rate: 0.3,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(30),
                test_end: ts(60),
                trades: 12,
                total_return: -0.03,
                sharpe_ratio: -0.3,
                max_drawdown: 0.06,
                win_rate: 0.35,
            },
            OutOfSampleMetrics {
                window_index: 2,
                test_start: ts(60),
                test_end: ts(90),
                trades: 8,
                total_return: -0.02,
                sharpe_ratio: -0.2,
                max_drawdown: 0.05,
                win_rate: 0.4,
            },
        ];
        let agg = engine.aggregate_metrics(&oos);
        let result = WalkForwardResult {
            config: WalkForwardConfig::default(),
            windows: vec![],
            oos_metrics: oos,
            aggregate: agg,
        };
        assert!(!engine.is_validated(&result), "全部负收益不应通过验证");
    }

    #[test]
    fn test_empty_aggregate() {
        let engine = WalkForwardEngine::with_defaults();
        let agg = engine.aggregate_metrics(&[]);
        assert_eq!(agg.num_windows, 0);
        assert_eq!(agg.total_trades, 0);
    }
}
