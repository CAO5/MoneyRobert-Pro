//! Signal Calibration
//! 概率校准模块
//!
//! 依据《系统评估与演进规划》第 4.2 节：
//! - Brier Score：衡量概率预测的准确性（越小越好）
//! - Log Loss：对数损失（越小越好）
//! - 校准曲线：预测概率与实际频率的对比
//!
//! 校准闭环（新增）：
//! - Platt Scaling：逻辑回归校准
//! - Isotonic Regression：保序回归校准
//! - apply_calibration：将校准应用到推理时的置信度

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

// ============================================================
// 校准模型：Platt Scaling & Isotonic Regression
// ============================================================

/// 校准模型类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationModel {
    /// Platt Scaling（逻辑回归）
    Platt,
    /// Isotonic Regression（保序回归）
    Isotonic,
    /// 线性缩放（简单比例校准）
    Linear,
}

/// Platt Scaling 参数
/// calibrated_p = 1 / (1 + exp(-(a * p + b)))
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlattParams {
    pub a: f64,
    pub b: f64,
}

/// 线性缩放参数
/// calibrated_p = clamp(factor * p + bias, 0, 1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearParams {
    pub factor: f64,
    pub bias: f64,
}

/// 校准后的模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FittedCalibration {
    Platt(PlattParams),
    Isotonic(Vec<(f64, f64)>), // (原始概率, 校准后概率) 的保序回归映射点
    Linear(LinearParams),
}

/// 拟合 Platt Scaling（梯度下降）
///
/// 使用逻辑回归将原始概率映射到校准概率
/// 输入：原始概率序列 + 实际结果序列
pub fn fit_platt_scaling(predictions: &[f64], outcomes: &[bool]) -> PlattParams {
    if predictions.is_empty() || predictions.len() != outcomes.len() {
        return PlattParams { a: 1.0, b: 0.0 };
    }

    let n = predictions.len() as f64;
    let mut a = 1.0_f64;
    let mut b = 0.0_f64;
    let lr = 0.01; // 学习率
    let epochs = 500;

    for _ in 0..epochs {
        let mut grad_a = 0.0;
        let mut grad_b = 0.0;
        for (p, o) in predictions.iter().zip(outcomes.iter()) {
            let z = a * p + b;
            let sig = 1.0 / (1.0 + (-z).exp());
            let target = if *o { 1.0 } else { 0.0 };
            let err = sig - target;
            grad_a += err * p;
            grad_b += err;
        }
        a -= lr * grad_a / n;
        b -= lr * grad_b / n;
    }

    PlattParams { a, b }
}

/// 拟合 Isotonic Regression（保序回归）
///
/// 将原始概率排序后，找到单调递增的最佳拟合
/// 输入：原始概率序列 + 实际结果序列
pub fn fit_isotonic_regression(predictions: &[f64], outcomes: &[bool]) -> Vec<(f64, f64)> {
    if predictions.is_empty() || predictions.len() != outcomes.len() {
        return vec![];
    }

    // 按预测概率排序
    let mut paired: Vec<(f64, f64)> = predictions
        .iter()
        .zip(outcomes.iter())
        .map(|(p, o)| (*p, if *o { 1.0 } else { 0.0 }))
        .collect();
    paired.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Pool Adjacent Violators (PAVA) 算法
    let mut blocks: Vec<(f64, f64, usize)> = vec![]; // (sum, weight, count)
    for (p, o) in &paired {
        blocks.push((*p, *o, 1));
        // 合并违反单调性的块
        while blocks.len() >= 2 {
            let n = blocks.len();
            let prev_avg = blocks[n - 2].0 / blocks[n - 2].1.max(1e-9);
            let curr_avg = blocks[n - 1].0 / blocks[n - 1].1.max(1e-9);
            if prev_avg > curr_avg {
                // 合并
                let merged_sum = blocks[n - 2].0 + blocks[n - 1].0;
                let merged_weight = blocks[n - 2].1 + blocks[n - 1].1;
                let merged_count = blocks[n - 2].2 + blocks[n - 1].2;
                blocks.pop();
                blocks.pop();
                blocks.push((merged_sum, merged_weight, merged_count));
            } else {
                break;
            }
        }
    }

    // 生成映射点
    let mut result: Vec<(f64, f64)> = Vec::new();
    let mut idx = 0;
    for (sum, weight, count) in &blocks {
        let avg = sum / weight.max(1e-9);
        for _ in 0..*count {
            if idx < paired.len() {
                result.push((paired[idx].0, avg));
                idx += 1;
            }
        }
    }
    result
}

/// 拟合线性缩放
/// calibrated_p = factor * p + bias
pub fn fit_linear_scaling(predictions: &[f64], outcomes: &[bool]) -> LinearParams {
    if predictions.is_empty() || predictions.len() != outcomes.len() {
        return LinearParams { factor: 1.0, bias: 0.0 };
    }

    let n = predictions.len() as f64;
    let mean_p = predictions.iter().sum::<f64>() / n;
    let mean_o = outcomes.iter().map(|o| if *o { 1.0 } else { 0.0 }).sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_p = 0.0;
    for (p, o) in predictions.iter().zip(outcomes.iter()) {
        let o_f = if *o { 1.0 } else { 0.0 };
        cov += (p - mean_p) * (o_f - mean_o);
        var_p += (p - mean_p).powi(2);
    }

    let factor = if var_p > 1e-9 { cov / var_p } else { 1.0 };
    let bias = mean_o - factor * mean_p;

    LinearParams { factor, bias }
}

/// 应用校准模型到单个概率
pub fn apply_calibration(model: &FittedCalibration, p: f64) -> f64 {
    match model {
        FittedCalibration::Platt(params) => {
            let z = params.a * p + params.b;
            1.0 / (1.0 + (-z).exp())
        }
        FittedCalibration::Isotonic(points) => {
            if points.is_empty() {
                return p;
            }
            // 线性插值
            if p <= points[0].0 {
                return points[0].1;
            }
            if p >= points[points.len() - 1].0 {
                return points[points.len() - 1].1;
            }
            for i in 1..points.len() {
                if p <= points[i].0 {
                    let (p0, v0) = points[i - 1];
                    let (p1, v1) = points[i];
                    if p1 - p0 > 1e-9 {
                        return v0 + (v1 - v0) * (p - p0) / (p1 - p0);
                    }
                    return v0;
                }
            }
            points[points.len() - 1].1
        }
        FittedCalibration::Linear(params) => {
            (params.factor * p + params.bias).clamp(0.0, 1.0)
        }
    }
}

/// 批量应用校准
pub fn apply_calibration_batch(model: &FittedCalibration, predictions: &[f64]) -> Vec<f64> {
    predictions.iter().map(|p| apply_calibration(model, *p)).collect()
}

/// 对三分类概率（p_up, p_down, p_flat）应用校准
///
/// 策略：分别对 p_up 和 p_down 做二分类校准，然后重新归一化
pub fn calibrate_three_class(
    model_up: &FittedCalibration,
    model_down: &FittedCalibration,
    p_up: f64,
    p_down: f64,
    _p_flat: f64,
) -> (f64, f64, f64) {
    let cal_up = apply_calibration(model_up, p_up);
    let cal_down = apply_calibration(model_down, p_down);
    let cal_flat_raw = 1.0 - cal_up - cal_down;
    let cal_flat = cal_flat_raw.max(0.0);

    let total = cal_up + cal_down + cal_flat;
    if total <= 0.0 {
        return (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);
    }

    (cal_up / total, cal_down / total, cal_flat / total)
}

/// 拟合并评估校准模型
///
/// 返回校准后的模型和校准后的 Brier Score
pub fn fit_and_evaluate(
    model_type: CalibrationModel,
    predictions: &[f64],
    outcomes: &[bool],
) -> (FittedCalibration, f64) {
    let model = match model_type {
        CalibrationModel::Platt => {
            let params = fit_platt_scaling(predictions, outcomes);
            FittedCalibration::Platt(params)
        }
        CalibrationModel::Isotonic => {
            let points = fit_isotonic_regression(predictions, outcomes);
            FittedCalibration::Isotonic(points)
        }
        CalibrationModel::Linear => {
            let params = fit_linear_scaling(predictions, outcomes);
            FittedCalibration::Linear(params)
        }
    };

    let calibrated = apply_calibration_batch(&model, predictions);
    let brier = compute_brier_score(&calibrated, outcomes);
    (model, brier)
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

    // ===== 校准模型测试 =====

    #[test]
    fn test_platt_scaling_improves_calibration() {
        // 构造过度自信的预测：高概率但实际频率较低
        let preds = vec![0.9, 0.9, 0.9, 0.9, 0.9, 0.1, 0.1, 0.1, 0.1, 0.1];
        let outcomes = vec![true, true, false, false, false, false, false, false, true, true];
        // 原始 Brier
        let original_brier = compute_brier_score(&preds, &outcomes);
        // Platt 校准
        let (model, calibrated_brier) = fit_and_evaluate(CalibrationModel::Platt, &preds, &outcomes);
        // 校准后 Brier 应不差于原始
        assert!(
            calibrated_brier <= original_brier + 1e-6,
            "Platt 校准后 Brier ({}) 应 <= 原始 ({})",
            calibrated_brier,
            original_brier
        );
        // 校准后的高概率应降低（因为实际频率低于预测）
        let calibrated_high = apply_calibration(&model, 0.9);
        assert!(calibrated_high < 0.9, "过度自信的预测应被校准降低");
    }

    #[test]
    fn test_isotonic_regression_monotonic() {
        let preds = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        let outcomes = vec![false, false, true, false, true, true, true, true, true];
        let points = fit_isotonic_regression(&preds, &outcomes);
        assert!(!points.is_empty());
        // 校准后的值应单调递增
        for i in 1..points.len() {
            assert!(
                points[i].1 >= points[i - 1].1 - 1e-9,
                "保序回归结果应单调递增"
            );
        }
    }

    #[test]
    fn test_linear_scaling() {
        // 构造过度自信的预测：预测概率极端但实际频率较低
        // 5 次预测 0.95，其中 2 次实际发生（40%）
        // 5 次预测 0.05，其中 0 次实际发生（0%）
        // 预测与结果弱正相关，但实际频率远低于预测概率
        let preds = vec![0.95, 0.05, 0.95, 0.05, 0.95, 0.05, 0.95, 0.05, 0.95, 0.05];
        let outcomes = vec![true, false, false, false, false, false, false, false, true, false];
        let params = fit_linear_scaling(&preds, &outcomes);
        // 实际频率 2/10 = 0.2，平均预测 0.5
        // factor 应 < 1（压缩过度自信的预测）
        assert!(
            params.factor < 1.0,
            "过度自信预测的 factor 应 < 1，实际: {}",
            params.factor
        );
        // bias 应为负（降低整体概率）
        assert!(
            params.bias < 0.0,
            "bias 应为负，实际: {}",
            params.bias
        );
    }

    #[test]
    fn test_apply_calibration_platt() {
        let model = FittedCalibration::Platt(PlattParams { a: 2.0, b: -1.0 });
        let p = apply_calibration(&model, 0.5);
        // z = 2*0.5 - 1 = 0, sigmoid(0) = 0.5
        assert!((p - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_apply_calibration_isotonic() {
        let model = FittedCalibration::Isotonic(vec![
            (0.0, 0.0),
            (0.5, 0.4),
            (1.0, 1.0),
        ]);
        // 插值测试
        let p = apply_calibration(&model, 0.25);
        // 0.25 在 0.0 和 0.5 之间，线性插值 = 0.2
        assert!((p - 0.2).abs() < 1e-9, "插值结果应为 0.2，实际: {}", p);
    }

    #[test]
    fn test_apply_calibration_linear() {
        let model = FittedCalibration::Linear(LinearParams { factor: 0.8, bias: 0.1 });
        let p = apply_calibration(&model, 0.5);
        // 0.8 * 0.5 + 0.1 = 0.5
        assert!((p - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_fit_and_evaluate_all_models() {
        let preds = vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1];
        let outcomes = vec![true, true, true, false, true, false, false, false, false];
        for model_type in [CalibrationModel::Platt, CalibrationModel::Isotonic, CalibrationModel::Linear] {
            let (_model, brier) = fit_and_evaluate(model_type, &preds, &outcomes);
            assert!(brier >= 0.0 && brier <= 1.0, "Brier 应在 [0,1] 范围内");
        }
    }

    #[test]
    fn test_empty_inputs() {
        let platt = fit_platt_scaling(&[], &[]);
        assert_eq!(platt.a, 1.0);
        assert_eq!(platt.b, 0.0);

        let isotonic = fit_isotonic_regression(&[], &[]);
        assert!(isotonic.is_empty());

        let linear = fit_linear_scaling(&[], &[]);
        assert_eq!(linear.factor, 1.0);
        assert_eq!(linear.bias, 0.0);
    }

    #[test]
    fn test_calibrate_three_class_identity() {
        // 全等概率情况
        let model_up = FittedCalibration::Linear(LinearParams { factor: 1.0, bias: 0.0 });
        let model_down = FittedCalibration::Linear(LinearParams { factor: 1.0, bias: 0.0 });
        let (up, down, flat) = calibrate_three_class(&model_up, &model_down, 0.33, 0.33, 0.34);
        // 归一化后仍接近原值
        assert!((up - 0.33).abs() < 0.02);
        assert!((down - 0.33).abs() < 0.02);
        assert!((flat - 0.34).abs() < 0.02);
        // 和为 1
        assert!((up + down + flat - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_calibrate_three_class_rescale() {
        // 校准后概率重新归一化
        let model_up = FittedCalibration::Linear(LinearParams { factor: 0.5, bias: 0.0 });
        let model_down = FittedCalibration::Linear(LinearParams { factor: 2.0, bias: 0.0 });
        let (up, down, flat) = calibrate_three_class(&model_up, &model_down, 0.5, 0.3, 0.2);
        // 校准后: cal_up = 0.25, cal_down = 0.6, cal_flat = 0.15
        assert!((up + down + flat - 1.0).abs() < 1e-9);
        assert!(up >= 0.0 && down >= 0.0 && flat >= 0.0);
    }

    #[test]
    fn test_calibrate_three_class_platt_underconfidence() {
        // Platt 模型能修正过度自信
        let model_up = FittedCalibration::Platt(PlattParams { a: 0.5, b: 0.5 });
        let model_down = FittedCalibration::Platt(PlattParams { a: 0.5, b: 0.5 });
        let (up, down, flat) = calibrate_three_class(&model_up, &model_down, 0.8, 0.1, 0.1);
        // 校准后 up 应下降（修正过度自信），flat 应上升
        assert!(up < 0.8, "过度自信应被修正，up 应从 0.8 下降");
        assert!((up + down + flat - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_calibrate_three_class_bounds_nonnegative() {
        // 所有输出应始终为非负且归一化
        let model_up = FittedCalibration::Linear(LinearParams { factor: 1.5, bias: 0.5 });
        let model_down = FittedCalibration::Linear(LinearParams { factor: 1.5, bias: 0.5 });
        for (pu, pd, pf) in [(0.2, 0.3, 0.5), (0.5, 0.3, 0.2), (0.1, 0.1, 0.8)] {
            let (up, down, flat) = calibrate_three_class(&model_up, &model_down, pu, pd, pf);
            assert!(up >= 0.0 && down >= 0.0 && flat >= 0.0, "所有概率应非负");
            assert!((up + down + flat - 1.0).abs() < 1e-9, "应归一化为 1");
        }
    }
}

// ============================================================
// 校准模型持久化存储
// ============================================================

pub mod store {
    use super::*;
    use chrono::{DateTime, Utc};
    use serde_json::Value;
    use sqlx::{PgPool, Row};
    use uuid::Uuid;

    /// 校准模型实体（对应 calibration_models 表）
    #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
    pub struct CalibrationModelEntity {
        pub model_id: Uuid,
        pub model_name: String,
        pub model_type: String,
        pub symbol: Option<String>,
        pub market_regime: Option<String>,
        pub target_horizon_sec: Option<i32>,
        pub source_model_version: Option<String>,
        pub platt_a: Option<f64>,
        pub platt_b: Option<f64>,
        pub linear_factor: Option<f64>,
        pub linear_bias: Option<f64>,
        pub isotonic_points: Option<Value>,
        pub training_start: Option<DateTime<Utc>>,
        pub training_end: Option<DateTime<Utc>>,
        pub sample_count: i32,
        pub train_brier_score: Option<f64>,
        pub train_calibration_error: Option<f64>,
        pub status: String,
        pub is_default: bool,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    impl CalibrationModelEntity {
        /// 转换为运行时可用的 FittedCalibration
        pub fn to_fitted(&self) -> Option<FittedCalibration> {
            match self.model_type.as_str() {
                "platt" => {
                    let a = self.platt_a?;
                    let b = self.platt_b?;
                    Some(FittedCalibration::Platt(PlattParams { a, b }))
                }
                "linear" => {
                    let factor = self.linear_factor?;
                    let bias = self.linear_bias?;
                    Some(FittedCalibration::Linear(LinearParams { factor, bias }))
                }
                "isotonic" => {
                    let points_val = self.isotonic_points.as_ref()?;
                    let arr = points_val.as_array()?;
                    let mut points = Vec::new();
                    for item in arr {
                        let p = item.get("p")?.as_f64()?;
                        let calibrated = item.get("calibrated")?.as_f64()?;
                        points.push((p, calibrated));
                    }
                    if points.is_empty() {
                        return None;
                    }
                    Some(FittedCalibration::Isotonic(points))
                }
                _ => None,
            }
        }
    }

    pub struct CalibrationStore;

    impl CalibrationStore {
        /// 保存一个校准模型
        pub async fn insert_model(
            pool: &PgPool,
            model_name: &str,
            model_type: CalibrationModel,
            fitted: &FittedCalibration,
            symbol: Option<&str>,
            market_regime: Option<&str>,
            target_horizon_sec: Option<i32>,
            source_model_version: Option<&str>,
            sample_count: i32,
            train_brier_score: f64,
            train_calibration_error: f64,
            training_start: Option<DateTime<Utc>>,
            training_end: Option<DateTime<Utc>>,
        ) -> Result<Uuid, sqlx::Error> {
            let model_type_str = match model_type {
                CalibrationModel::Platt => "platt",
                CalibrationModel::Isotonic => "isotonic",
                CalibrationModel::Linear => "linear",
            };

            let (platt_a, platt_b) = match fitted {
                FittedCalibration::Platt(p) => (Some(p.a), Some(p.b)),
                _ => (None, None),
            };

            let (linear_factor, linear_bias) = match fitted {
                FittedCalibration::Linear(p) => (Some(p.factor), Some(p.bias)),
                _ => (None, None),
            };

            let isotonic_points = match fitted {
                FittedCalibration::Isotonic(points) => {
                    let arr: Vec<Value> = points
                        .iter()
                        .map(|(p, c)| {
                            serde_json::json!({"p": p, "calibrated": c})
                        })
                        .collect();
                    Some(Value::Array(arr))
                }
                _ => None,
            };

            let row = sqlx::query(
                r#"INSERT INTO calibration_models
                   (model_name, model_type, symbol, market_regime,
                    target_horizon_sec, source_model_version,
                    platt_a, platt_b, linear_factor, linear_bias, isotonic_points,
                    training_start, training_end, sample_count,
                    train_brier_score, train_calibration_error,
                    status, is_default)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                           $12, $13, $14, $15, $16, 'active', FALSE)
                   RETURNING model_id"#,
            )
            .bind(model_name)
            .bind(model_type_str)
            .bind(symbol)
            .bind(market_regime)
            .bind(target_horizon_sec)
            .bind(source_model_version)
            .bind(platt_a)
            .bind(platt_b)
            .bind(linear_factor)
            .bind(linear_bias)
            .bind(isotonic_points)
            .bind(training_start)
            .bind(training_end)
            .bind(sample_count)
            .bind(train_brier_score)
            .bind(train_calibration_error)
            .fetch_one(pool)
            .await?;

            Ok(row.get("model_id"))
        }

        /// 查询指定维度的默认校准模型
        pub async fn get_default_model(
            pool: &PgPool,
            symbol: Option<&str>,
            market_regime: Option<&str>,
            model_type: Option<&str>,
        ) -> Result<Option<CalibrationModelEntity>, sqlx::Error> {
            let mut sql = String::from(
                "SELECT * FROM calibration_models WHERE status = 'active'",
            );
            let mut params: Vec<&str> = Vec::new();

            if symbol.is_some() {
                sql.push_str(" AND symbol = $");
                params.push(symbol.unwrap());
                sql.push_str(&params.len().to_string());
            } else {
                sql.push_str(" AND symbol IS NULL");
            }

            if market_regime.is_some() {
                sql.push_str(" AND market_regime = $");
                params.push(market_regime.unwrap());
                sql.push_str(&params.len().to_string());
            } else {
                sql.push_str(" AND market_regime IS NULL");
            }

            if model_type.is_some() {
                sql.push_str(" AND model_type = $");
                params.push(model_type.unwrap());
                sql.push_str(&params.len().to_string());
            }

            sql.push_str(" ORDER BY is_default DESC, created_at DESC LIMIT 1");

            let row = sqlx::query_as::<_, CalibrationModelEntity>(&sql)
                .fetch_optional(pool)
                .await?;

            Ok(row)
        }

        /// 按 ID 查询校准模型
        pub async fn get_model_by_id(
            pool: &PgPool,
            model_id: Uuid,
        ) -> Result<Option<CalibrationModelEntity>, sqlx::Error> {
            let row = sqlx::query_as::<_, CalibrationModelEntity>(
                "SELECT * FROM calibration_models WHERE model_id = $1",
            )
            .bind(model_id)
            .fetch_optional(pool)
            .await?;
            Ok(row)
        }

        /// 列出校准模型
        pub async fn list_models(
            pool: &PgPool,
            symbol: Option<&str>,
            market_regime: Option<&str>,
            status: Option<&str>,
            limit: i64,
        ) -> Result<Vec<CalibrationModelEntity>, sqlx::Error> {
            let mut sql = String::from("SELECT * FROM calibration_models WHERE 1=1");
            let mut binds: Vec<String> = Vec::new();

            if let Some(s) = symbol {
                sql.push_str(" AND symbol = $");
                sql.push_str(&(binds.len() + 1).to_string());
                binds.push(s.to_string());
            }
            if let Some(r) = market_regime {
                sql.push_str(" AND market_regime = $");
                sql.push_str(&(binds.len() + 1).to_string());
                binds.push(r.to_string());
            }
            if let Some(st) = status {
                sql.push_str(" AND status = $");
                sql.push_str(&(binds.len() + 1).to_string());
                binds.push(st.to_string());
            }

            sql.push_str(" ORDER BY created_at DESC LIMIT $");
            sql.push_str(&(binds.len() + 1).to_string());

            let mut query = sqlx::query_as::<_, CalibrationModelEntity>(&sql);
            for b in &binds {
                query = query.bind(b);
            }
            query = query.bind(limit);

            let rows = query.fetch_all(pool).await?;
            Ok(rows)
        }

        /// 设置为默认模型
        pub async fn set_default(
            pool: &PgPool,
            model_id: Uuid,
        ) -> Result<(), sqlx::Error> {
            sqlx::query(
                "UPDATE calibration_models SET is_default = FALSE WHERE model_id != $1",
            )
            .bind(model_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "UPDATE calibration_models SET is_default = TRUE, updated_at = NOW() WHERE model_id = $1",
            )
            .bind(model_id)
            .execute(pool)
            .await?;

            Ok(())
        }

        /// 停用模型
        pub async fn deprecate_model(
            pool: &PgPool,
            model_id: Uuid,
        ) -> Result<(), sqlx::Error> {
            sqlx::query(
                "UPDATE calibration_models SET status = 'deprecated', updated_at = NOW() WHERE model_id = $1",
            )
            .bind(model_id)
            .execute(pool)
            .await?;
            Ok(())
        }

        /// 从预测数据训练校准模型
        ///
        /// 输入：模型版本、symbol、市场状态、时间范围、校准类型
        /// 输出：校准模型 ID
        pub async fn train_from_predictions(
            pool: &PgPool,
            source_model_version: &str,
            symbol: Option<&str>,
            market_regime: Option<&str>,
            target_horizon_sec: Option<i32>,
            model_type: CalibrationModel,
            model_name: &str,
        ) -> Result<Uuid, String> {
            // 1. 从数据库读取已评估的预测
            let mut sql = String::from(
                "SELECT p_up, realized_direction FROM signal_predictions \
                 WHERE model_version = $1 AND evaluated_at IS NOT NULL",
            );

            let mut params: Vec<&str> = vec![source_model_version];

            if let Some(s) = symbol {
                sql.push_str(" AND symbol = $");
                sql.push_str(&(params.len() + 1).to_string());
                params.push(s);
            }
            if let Some(r) = market_regime {
                sql.push_str(" AND market_regime = $");
                sql.push_str(&(params.len() + 1).to_string());
                params.push(r);
            }
            if target_horizon_sec.is_some() {
                sql.push_str(&format!(" AND target_horizon_sec = ${}", params.len() + 1));
            }

            sql.push_str(" ORDER BY prediction_time ASC");

            let mut query = sqlx::query(&sql);
            for p in &params {
                query = query.bind(p);
            }
            if let Some(h) = target_horizon_sec {
                query = query.bind(h);
            }

            let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;

            if rows.is_empty() {
                return Err("No evaluated predictions found for training".to_string());
            }

            let mut predictions: Vec<f64> = Vec::with_capacity(rows.len());
            let mut outcomes: Vec<bool> = Vec::with_capacity(rows.len());

            for row in &rows {
                let p_up: f64 = row.get("p_up");
                let direction: Option<String> = row.get("realized_direction");
                if let Some(dir) = direction {
                    predictions.push(p_up);
                    outcomes.push(dir == "up");
                }
            }

            if predictions.len() < 10 {
                return Err("Insufficient samples (need >= 10)".to_string());
            }

            // 2. 拟合校准模型
            let (fitted, brier) = fit_and_evaluate(model_type, &predictions, &outcomes);
            let cal_error = {
                let curve = compute_calibration_curve(&predictions, &outcomes, 10);
                compute_calibration_error(&curve)
            };

            // 3. 保存到数据库
            let sample_count = predictions.len() as i32;
            let model_id = Self::insert_model(
                pool,
                model_name,
                model_type,
                &fitted,
                symbol,
                market_regime,
                target_horizon_sec,
                Some(source_model_version),
                sample_count,
                brier,
                cal_error,
                None,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;

            Ok(model_id)
        }
    }
}
