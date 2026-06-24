//! Purged K-Fold Cross-Validation
//! 清洗 K 折交叉验证
//!
//! 依据《系统评估与演进规划》第二阶段任务 4：
//! 使用 Walk-forward 样本外验证，防止过拟合。
//!
//! 核心思想（参考 López de Prado "Advances in Financial Machine Learning"）：
//! 1. 将时间序列均匀划分为 K 个连续的折（fold）
//! 2. 每次留出 1 个折作为测试集，其余 K-1 个折作为训练集
//! 3. 在测试折前后插入 purge（清洗期），防止训练集标签泄露到测试集
//! 4. 在测试折后插入 embargo（禁运期），防止测试集标签泄露到下一训练集
//! 5. 对每个折运行回测，收集样本外绩效指标
//! 6. 汇总所有折的样本外结果，评估策略稳定性
//!
//! 与 Walk-forward 的区别：
//! - Walk-forward 使用滚动窗口，训练集和测试集连续推进
//! - Purged K-fold 将数据分段，测试集可以来自任意时间段
//! - Purged K-fold 更适合评估策略在不同市场环境下的整体表现

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::walk_forward::{AggregateMetrics, OutOfSampleMetrics, WalkForwardEngine};

/// Purged K-fold 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgedKFoldConfig {
    /// 折数（K）
    pub k: usize,
    /// 清洗期长度（天）：测试折前后的隔离期
    pub purge_days: i64,
    /// 禁运期长度（天）：测试折后的观察禁止期
    pub embargo_days: i64,
}

impl Default for PurgedKFoldConfig {
    fn default() -> Self {
        Self {
            k: 5,
            purge_days: 1,
            embargo_days: 1,
        }
    }
}

/// 单个 K-fold 折
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KFold {
    /// 折序号（0 到 K-1）
    pub fold_index: usize,
    /// 测试集起始时间（已含 purge 间隔）
    pub test_start: DateTime<Utc>,
    /// 测试集结束时间
    pub test_end: DateTime<Utc>,
    /// 训练集时间段列表（可能不连续，因为排除了测试折及其 purge 区域）
    pub train_periods: Vec<TrainPeriod>,
}

/// 训练时间段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainPeriod {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Purged K-fold 验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgedKFoldResult {
    /// 配置
    pub config: PurgedKFoldConfig,
    /// 总时间范围
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// 所有折
    pub folds: Vec<KFold>,
    /// 每折的样本外绩效
    pub oos_metrics: Vec<OutOfSampleMetrics>,
    /// 汇总统计
    pub aggregate: AggregateMetrics,
    /// 是否通过验证
    pub is_validated: bool,
}

/// Purged K-fold 交叉验证引擎
pub struct PurgedKFoldEngine {
    config: PurgedKFoldConfig,
}

impl PurgedKFoldEngine {
    pub fn new(config: PurgedKFoldConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(PurgedKFoldConfig::default())
    }

    /// 生成 K-fold 折划分
    ///
    /// 将时间区间 [start, end] 均匀划分为 K 个连续的折，
    /// 每折作为测试集时，前后添加 purge 间隔，测试折后添加 embargo。
    pub fn generate_folds(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<KFold> {
        let total_days = (end - start).num_days();
        if total_days <= 0 || self.config.k == 0 {
            return Vec::new();
        }

        let days_per_fold = total_days / self.config.k as i64;
        if days_per_fold <= 0 {
            return Vec::new();
        }

        let purge = Duration::days(self.config.purge_days);
        let embargo = Duration::days(self.config.embargo_days);

        let mut folds = Vec::with_capacity(self.config.k);

        for i in 0..self.config.k {
            let fold_start = start + Duration::days(days_per_fold * i as i64);
            let fold_end = if i == self.config.k - 1 {
                end // 最后一折包含剩余时间
            } else {
                start + Duration::days(days_per_fold * (i + 1) as i64)
            };

            // 测试集：折本身（前后各加 purge 间隔）
            let test_start = fold_start + purge;
            let test_end = fold_end - purge;

            if test_start >= test_end {
                continue;
            }

            // 训练集：所有其他折，排除测试折及其 purge/embargo 区域
            let train_periods = self.generate_train_periods(start, end, fold_start, fold_end);

            folds.push(KFold {
                fold_index: i,
                test_start,
                test_end,
                train_periods,
            });
        }

        folds
    }

    /// 生成训练集时间段
    ///
    /// 训练集 = 全部时间 - 测试折 - purge - embargo
    /// 这会产生最多 2 个训练时间段（测试折之前和之后）
    fn generate_train_periods(
        &self,
        overall_start: DateTime<Utc>,
        overall_end: DateTime<Utc>,
        fold_start: DateTime<Utc>,
        fold_end: DateTime<Utc>,
    ) -> Vec<TrainPeriod> {
        let purge = Duration::days(self.config.purge_days);
        let embargo = Duration::days(self.config.embargo_days);

        let mut periods = Vec::new();

        // 测试折之前的训练段：[overall_start, fold_start - purge]
        let pre_end = fold_start - purge;
        if overall_start < pre_end {
            periods.push(TrainPeriod {
                start: overall_start,
                end: pre_end,
            });
        }

        // 测试折之后的训练段：[fold_end + embargo, overall_end]
        let post_start = fold_end + embargo;
        if post_start < overall_end {
            periods.push(TrainPeriod {
                start: post_start,
                end: overall_end,
            });
        }

        periods
    }

    /// 汇总所有折的样本外绩效
    ///
    /// 复用 WalkForwardEngine 的 aggregate_metrics 逻辑
    pub fn aggregate_metrics(&self, oos: &[OutOfSampleMetrics]) -> AggregateMetrics {
        let wf_engine = WalkForwardEngine::with_defaults();
        wf_engine.aggregate_metrics(oos)
    }

    /// 判断策略是否通过 Purged K-fold 验证
    ///
    /// 通过条件：
    /// 1. 至少 3 个折（统计显著性）
    /// 2. 正收益折占比 > 50%
    /// 3. 平均收益 > 0
    /// 4. 稳定性评分 > 0
    pub fn is_validated(&self, result: &PurgedKFoldResult) -> bool {
        let agg = &result.aggregate;
        agg.num_windows >= 3
            && agg.positive_window_ratio > 0.5
            && agg.avg_return > 0.0
            && agg.stability_score > 0.0
    }

    /// 构建完整结果（给定 OOS 指标列表）
    pub fn build_result(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        oos_metrics: Vec<OutOfSampleMetrics>,
    ) -> PurgedKFoldResult {
        let folds = self.generate_folds(start, end);
        let aggregate = self.aggregate_metrics(&oos_metrics);

        let mut result = PurgedKFoldResult {
            config: self.config.clone(),
            start,
            end,
            folds,
            oos_metrics,
            aggregate,
            is_validated: false,
        };
        result.is_validated = self.is_validated(&result);
        result
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
    fn test_generate_folds_basic() {
        let engine = PurgedKFoldEngine::new(PurgedKFoldConfig {
            k: 5,
            purge_days: 1,
            embargo_days: 1,
        });
        // 100 天，5 折，每折 20 天
        let folds = engine.generate_folds(ts(0), ts(100));
        assert_eq!(folds.len(), 5, "应生成 5 个折");

        // 每折的测试集应为 20 - 2*1 = 18 天
        for fold in &folds {
            let test_days = (fold.test_end - fold.test_start).num_days();
            assert!(
                test_days >= 17 && test_days <= 20,
                "折 {} 测试期天数 {} 不在预期范围",
                fold.fold_index,
                test_days
            );
        }
    }

    #[test]
    fn test_generate_folds_train_periods() {
        let engine = PurgedKFoldEngine::new(PurgedKFoldConfig {
            k: 3,
            purge_days: 1,
            embargo_days: 1,
        });
        // 90 天，3 折，每折 30 天
        let folds = engine.generate_folds(ts(0), ts(90));

        // 第一折（测试集在前段）：训练集应为 1 段（测试折之后）
        let fold0 = &folds[0];
        assert!(
            fold0.train_periods.len() >= 1,
            "第一折应有至少 1 个训练段"
        );

        // 中间折（测试集在中段）：训练集应为 2 段（前后各一段）
        let fold1 = &folds[1];
        assert_eq!(
            fold1.train_periods.len(),
            2,
            "中间折应有 2 个训练段，实际: {}",
            fold1.train_periods.len()
        );

        // 最后折（测试集在后段）：训练集应为 1 段（测试折之前）
        let fold2 = &folds[2];
        assert!(
            fold2.train_periods.len() >= 1,
            "最后折应有至少 1 个训练段"
        );
    }

    #[test]
    fn test_generate_folds_purge_gap() {
        let engine = PurgedKFoldEngine::new(PurgedKFoldConfig {
            k: 3,
            purge_days: 5,
            embargo_days: 1,
        });
        let folds = engine.generate_folds(ts(0), ts(90));

        // 验证 purge 间隔：测试集开始应在折开始 + purge 之后
        for fold in &folds {
            let fold_start = ts(0) + Duration::days(30 * fold.fold_index as i64);
            let purge_gap = fold.test_start - fold_start;
            assert_eq!(
                purge_gap.num_days(),
                5,
                "折 {} 的 purge 间隔应为 5 天",
                fold.fold_index
            );
        }
    }

    #[test]
    fn test_generate_folds_insufficient_data() {
        let engine = PurgedKFoldEngine::new(PurgedKFoldConfig {
            k: 10,
            purge_days: 1,
            embargo_days: 1,
        });
        // 只有 5 天，10 折无法划分
        let folds = engine.generate_folds(ts(0), ts(5));
        assert!(folds.is_empty(), "数据不足时应返回空");
    }

    #[test]
    fn test_generate_folds_k1() {
        let engine = PurgedKFoldEngine::new(PurgedKFoldConfig {
            k: 1,
            purge_days: 1,
            embargo_days: 1,
        });
        let folds = engine.generate_folds(ts(0), ts(30));
        assert_eq!(folds.len(), 1, "K=1 应生成 1 个折");
    }

    #[test]
    fn test_aggregate_metrics_empty() {
        let engine = PurgedKFoldEngine::with_defaults();
        let agg = engine.aggregate_metrics(&[]);
        assert_eq!(agg.num_windows, 0);
        assert_eq!(agg.total_trades, 0);
    }

    #[test]
    fn test_aggregate_metrics_basic() {
        let engine = PurgedKFoldEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(10),
                trades: 5,
                total_return: 0.05,
                sharpe_ratio: 1.5,
                max_drawdown: -0.02,
                win_rate: 0.6,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(10),
                test_end: ts(20),
                trades: 3,
                total_return: -0.02,
                sharpe_ratio: -0.5,
                max_drawdown: -0.03,
                win_rate: 0.33,
            },
        ];
        let agg = engine.aggregate_metrics(&oos);
        assert_eq!(agg.num_windows, 2);
        assert_eq!(agg.total_trades, 8);
        assert!((agg.avg_return - 0.015).abs() < 1e-9);
        assert!((agg.positive_window_ratio - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_is_validated_pass() {
        let engine = PurgedKFoldEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(10),
                trades: 5,
                total_return: 0.05,
                sharpe_ratio: 1.5,
                max_drawdown: -0.02,
                win_rate: 0.6,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(10),
                test_end: ts(20),
                trades: 4,
                total_return: 0.03,
                sharpe_ratio: 1.0,
                max_drawdown: -0.01,
                win_rate: 0.5,
            },
            OutOfSampleMetrics {
                window_index: 2,
                test_start: ts(20),
                test_end: ts(30),
                trades: 6,
                total_return: 0.02,
                sharpe_ratio: 0.8,
                max_drawdown: -0.015,
                win_rate: 0.55,
            },
        ];
        let result = engine.build_result(ts(0), ts(30), oos);
        assert!(
            result.is_validated,
            "3 个正收益折应通过验证，stability={}",
            result.aggregate.stability_score
        );
    }

    #[test]
    fn test_is_validated_fail_negative_return() {
        let engine = PurgedKFoldEngine::with_defaults();
        let oos = vec![
            OutOfSampleMetrics {
                window_index: 0,
                test_start: ts(0),
                test_end: ts(10),
                trades: 5,
                total_return: -0.05,
                sharpe_ratio: -1.5,
                max_drawdown: -0.02,
                win_rate: 0.4,
            },
            OutOfSampleMetrics {
                window_index: 1,
                test_start: ts(10),
                test_end: ts(20),
                trades: 4,
                total_return: -0.03,
                sharpe_ratio: -1.0,
                max_drawdown: -0.01,
                win_rate: 0.3,
            },
            OutOfSampleMetrics {
                window_index: 2,
                test_start: ts(20),
                test_end: ts(30),
                trades: 6,
                total_return: -0.02,
                sharpe_ratio: -0.8,
                max_drawdown: -0.015,
                win_rate: 0.35,
            },
        ];
        let result = engine.build_result(ts(0), ts(30), oos);
        assert!(!result.is_validated, "全部负收益折不应通过验证");
    }

    #[test]
    fn test_is_validated_fail_too_few_folds() {
        let engine = PurgedKFoldEngine::with_defaults();
        let oos = vec![OutOfSampleMetrics {
            window_index: 0,
            test_start: ts(0),
            test_end: ts(10),
            trades: 5,
            total_return: 0.05,
            sharpe_ratio: 1.5,
            max_drawdown: -0.02,
            win_rate: 0.6,
        }];
        let result = engine.build_result(ts(0), ts(10), oos);
        assert!(!result.is_validated, "少于 3 个折不应通过验证");
    }

    #[test]
    fn test_config_default() {
        let config = PurgedKFoldConfig::default();
        assert_eq!(config.k, 5);
        assert_eq!(config.purge_days, 1);
        assert_eq!(config.embargo_days, 1);
    }

    #[test]
    fn test_result_serialization() {
        let engine = PurgedKFoldEngine::with_defaults();
        let result = engine.build_result(ts(0), ts(100), vec![]);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("folds"));
        assert!(json.contains("aggregate"));
        assert!(json.contains("is_validated"));
        assert!(json.contains("config"));
    }
}
