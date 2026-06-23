//! Signal Calibration
//! 概率校准模块
//!
//! 依据《系统评估与演进规划》第 4.2 节：
//! - Brier Score：衡量概率预测的准确性（越小越好）
//! - Log Loss：对数损失（越小越好）
//! - 校准曲线：预测概率与实际频率的对比

use serde::{Deserialize, Serialize};

/// 校准曲线上的一个点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationPoint {
    /// 预测概率区间下限
    pub predicted: f64,
    /// 实际发生频率
    pub actual: f64,
    /// 样本数
    pub count: i32,
}

/// 计算 Brier 分数
/// Brier Score = (1/N) * Σ (p_i - o_i)^2
/// 其中 p_i 是预测概率，o_i 是实际结果（1 发生，0 未发生）
/// 越小越好，0 表示完美预测
pub fn compute_brier_score(predictions: &[f64], outcomes: &[bool]) -> f64 {
    if predictions.is_empty() || predictions.len() != outcomes.len() {
        return 0.0;
    }
    let n = predictions.len() as f64;
    let sum: f64 = predictions
        .iter()
        .zip(outcomes.iter())
        .map(|(p, o)| {
            let o_f = if *o { 1.0 } else { 0.0 };
            (p - o_f).powi(2)
        })
        .sum();
    sum / n
}

/// 计算对数损失
/// Log Loss = -(1/N) * Σ [o_i * log(p_i) + (1-o_i) * log(1-p_i)]
/// 越小越好，0 表示完美预测
pub fn compute_log_loss(predictions: &[f64], outcomes: &[bool]) -> f64 {
    if predictions.is_empty() || predictions.len() != outcomes.len() {
        return 0.0;
    }
    let n = predictions.len() as f64;
    let eps = 1e-15; // 避免 log(0)
    let sum: f64 = predictions
        .iter()
        .zip(outcomes.iter())
        .map(|(p, o)| {
            let p_clamped = p.clamp(eps, 1.0 - eps);
            let o_f = if *o { 1.0 } else { 0.0 };
            o_f * p_clamped.ln() + (1.0 - o_f) * (1.0 - p_clamped).ln()
        })
        .sum();
    -sum / n
}

/// 计算校准曲线
/// 将预测概率分桶，计算每个桶内实际发生的频率
pub fn compute_calibration_curve(
    predictions: &[f64],
    outcomes: &[bool],
    num_bins: usize,
) -> Vec<CalibrationPoint> {
    if predictions.is_empty() || num_bins == 0 {
        return Vec::new();
    }
    let bin_width = 1.0 / num_bins as f64;
    let mut bins: Vec<(i32, i32)> = vec![(0, 0); num_bins]; // (positive_count, total_count)

    for (p, o) in predictions.iter().zip(outcomes.iter()) {
        let bin_idx = ((p / bin_width).floor() as usize).min(num_bins - 1);
        bins[bin_idx].1 += 1;
        if *o {
            bins[bin_idx].0 += 1;
        }
    }

    bins.iter()
        .enumerate()
        .map(|(i, (pos, total))| {
            let predicted = (i as f64 + 0.5) * bin_width; // 桶中心
            let actual = if *total > 0 {
                *pos as f64 / *total as f64
            } else {
                0.0
            };
            CalibrationPoint {
                predicted,
                actual,
                count: *total,
            }
        })
        .collect()
}

/// 计算校准误差
/// 校准误差 = 各桶 |预测概率 - 实际频率| 的加权平均
pub fn compute_calibration_error(curve: &[CalibrationPoint]) -> f64 {
    let total_count: i32 = curve.iter().map(|p| p.count).sum();
    if total_count == 0 {
        return 0.0;
    }
    let weighted_sum: f64 = curve
        .iter()
        .map(|p| (p.predicted - p.actual).abs() * p.count as f64)
        .sum();
    weighted_sum / total_count as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brier_score_perfect_prediction() {
        // 完美预测：预测 1.0 且实际发生，预测 0.0 且实际未发生
        let preds = vec![0.9, 0.1, 0.8, 0.2];
        let outcomes = vec![true, false, true, false];
        let brier = compute_brier_score(&preds, &outcomes);
        // Brier = ((0.9-1)^2 + (0.1-0)^2 + (0.8-1)^2 + (0.2-0)^2) / 4
        //       = (0.01 + 0.01 + 0.04 + 0.04) / 4 = 0.025
        assert!((brier - 0.025).abs() < 1e-9, "Brier 应为 0.025，实际: {}", brier);
    }

    #[test]
    fn test_brier_score_worst_prediction() {
        // 最差预测：预测 1.0 但实际未发生
        let preds = vec![1.0, 1.0];
        let outcomes = vec![false, false];
        let brier = compute_brier_score(&preds, &outcomes);
        assert!((brier - 1.0).abs() < 1e-9, "最差预测 Brier 应为 1.0，实际: {}", brier);
    }

    #[test]
    fn test_brier_score_empty() {
        let brier = compute_brier_score(&[], &[]);
        assert_eq!(brier, 0.0);
    }

    #[test]
    fn test_log_loss_perfect_prediction() {
        // 完美预测：预测接近 1.0 且实际发生
        let preds = vec![0.999, 0.001];
        let outcomes = vec![true, false];
        let ll = compute_log_loss(&preds, &outcomes);
        assert!(ll < 0.01, "完美预测 Log Loss 应接近 0，实际: {}", ll);
    }

    #[test]
    fn test_log_loss_worst_prediction() {
        // 最差预测：预测 1.0 但实际未发生
        let preds = vec![1.0];
        let outcomes = vec![false];
        let ll = compute_log_loss(&preds, &outcomes);
        // 由于 clamp，Log Loss 不会是无穷大，但会很大
        assert!(ll > 30.0, "最差预测 Log Loss 应很大，实际: {}", ll);
    }

    #[test]
    fn test_calibration_curve_well_calibrated() {
        // 良好校准：预测概率与实际频率接近
        // 使用 0.05 确保落在第一个桶（0.0-0.1）内
        let preds = vec![0.05; 10];
        let outcomes = vec![false, false, false, false, false, false, false, false, false, true];
        let curve = compute_calibration_curve(&preds, &outcomes, 10);
        // 第一个桶（0.0-0.1）：预测中心 0.05，实际频率 0.1
        let first_bin = &curve[0];
        assert_eq!(first_bin.count, 10);
        assert!((first_bin.actual - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_calibration_curve_empty() {
        let curve = compute_calibration_curve(&[], &[], 10);
        assert!(curve.is_empty());
    }

    #[test]
    fn test_calibration_error_zero_when_perfect() {
        // 完美校准：预测概率 = 实际频率
        let curve = vec![CalibrationPoint {
            predicted: 0.5,
            actual: 0.5,
            count: 10,
        }];
        let err = compute_calibration_error(&curve);
        assert!((err - 0.0).abs() < 1e-9, "完美校准误差应为 0，实际: {}", err);
    }

    #[test]
    fn test_calibration_error_positive_when_miscalibrated() {
        let curve = vec![CalibrationPoint {
            predicted: 0.9,
            actual: 0.1,
            count: 10,
        }];
        let err = compute_calibration_error(&curve);
        assert!((err - 0.8).abs() < 1e-9, "校准误差应为 0.8，实际: {}", err);
    }
}
