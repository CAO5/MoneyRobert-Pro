//! Signal Store
//! 概率信号存储层
//!
//! 提供概率预测、决策卡、校准报告的读写接口

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::models::{CalibrationReport, DecisionCard, SignalPrediction};

pub struct SignalStore;

impl SignalStore {
    // =========================================================
    // 概率预测（signal_predictions）
    // =========================================================

    /// 写入概率预测
    pub async fn insert_prediction(
        pool: &PgPool,
        pred: &SignalPrediction,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO signal_predictions
               (prediction_id, symbol, prediction_time, target_horizon_sec,
                p_up, p_down, p_flat, q10, q50, q90,
                expected_volatility, mae_estimate, uncertainty,
                model_version, model_type, feature_version, features_used, market_regime,
                realized_return, realized_direction, evaluated_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)"#,
        )
        .bind(pred.prediction_id)
        .bind(&pred.symbol)
        .bind(pred.prediction_time)
        .bind(pred.target_horizon_sec)
        .bind(pred.p_up)
        .bind(pred.p_down)
        .bind(pred.p_flat)
        .bind(pred.q10)
        .bind(pred.q50)
        .bind(pred.q90)
        .bind(pred.expected_volatility)
        .bind(pred.mae_estimate)
        .bind(pred.uncertainty)
        .bind(&pred.model_version)
        .bind(&pred.model_type)
        .bind(&pred.feature_version)
        .bind(&pred.features_used)
        .bind(&pred.market_regime)
        .bind(pred.realized_return)
        .bind(&pred.realized_direction)
        .bind(pred.evaluated_at)
        .bind(pred.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 回填预测的实际结果
    pub async fn backfill_prediction_outcome(
        pool: &PgPool,
        prediction_id: Uuid,
        realized_return: f64,
        realized_direction: &str,
        evaluated_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE signal_predictions
               SET realized_return = $1,
                   realized_direction = $2,
                   evaluated_at = $3
               WHERE prediction_id = $4"#,
        )
        .bind(realized_return)
        .bind(realized_direction)
        .bind(evaluated_at)
        .bind(prediction_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 查询已评估的预测（用于校准）
    pub async fn query_evaluated_predictions(
        pool: &PgPool,
        model_version: &str,
        symbol: Option<&str>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<SignalPrediction>, sqlx::Error> {
        let rows = if let Some(sym) = symbol {
            sqlx::query(
                r#"SELECT prediction_id, symbol, prediction_time, target_horizon_sec,
                          p_up, p_down, p_flat, q10, q50, q90,
                          expected_volatility, mae_estimate, uncertainty,
                          model_version, model_type, feature_version, features_used, market_regime,
                          realized_return, realized_direction, evaluated_at, created_at
                   FROM signal_predictions
                   WHERE model_version = $1 AND symbol = $2
                     AND prediction_time >= $3 AND prediction_time <= $4
                     AND evaluated_at IS NOT NULL
                   ORDER BY prediction_time ASC"#,
            )
            .bind(model_version)
            .bind(sym)
            .bind(start_time)
            .bind(end_time)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT prediction_id, symbol, prediction_time, target_horizon_sec,
                          p_up, p_down, p_flat, q10, q50, q90,
                          expected_volatility, mae_estimate, uncertainty,
                          model_version, model_type, feature_version, features_used, market_regime,
                          realized_return, realized_direction, evaluated_at, created_at
                   FROM signal_predictions
                   WHERE model_version = $1
                     AND prediction_time >= $2 AND prediction_time <= $3
                     AND evaluated_at IS NOT NULL
                   ORDER BY prediction_time ASC"#,
            )
            .bind(model_version)
            .bind(start_time)
            .bind(end_time)
            .fetch_all(pool)
            .await?
        };

        let preds = rows
            .into_iter()
            .map(|row| SignalPrediction {
                prediction_id: row.get("prediction_id"),
                symbol: row.get("symbol"),
                prediction_time: row.get("prediction_time"),
                target_horizon_sec: row.get("target_horizon_sec"),
                p_up: row.get("p_up"),
                p_down: row.get("p_down"),
                p_flat: row.get("p_flat"),
                q10: row.get("q10"),
                q50: row.get("q50"),
                q90: row.get("q90"),
                expected_volatility: row.get("expected_volatility"),
                mae_estimate: row.get("mae_estimate"),
                uncertainty: row.get("uncertainty"),
                model_version: row.get("model_version"),
                model_type: row.get("model_type"),
                feature_version: row.get("feature_version"),
                features_used: row.get("features_used"),
                market_regime: row.get("market_regime"),
                realized_return: row.get("realized_return"),
                realized_direction: row.get("realized_direction"),
                evaluated_at: row.get("evaluated_at"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok(preds)
    }

    // =========================================================
    // 决策卡（decision_cards）
    // =========================================================

    /// 写入决策卡
    pub async fn insert_decision_card(
        pool: &PgPool,
        card: &DecisionCard,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO decision_cards
               (card_id, user_id, symbol, generated_at,
                suggested_action, target_horizon_sec,
                p_up, p_down, p_flat, q10, q50, q90,
                expected_value, worst_case, position_suggestion, risk_budget_used,
                applicable_regime, data_freshness_sec,
                supporting_evidence, opposing_evidence, sample_performance, data_lineage,
                invalidation_conditions, model_version, prediction_id, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26)"#,
        )
        .bind(card.card_id)
        .bind(card.user_id)
        .bind(&card.symbol)
        .bind(card.generated_at)
        .bind(card.suggested_action.as_str())
        .bind(card.target_horizon_sec)
        .bind(card.p_up)
        .bind(card.p_down)
        .bind(card.p_flat)
        .bind(card.q10)
        .bind(card.q50)
        .bind(card.q90)
        .bind(card.expected_value)
        .bind(card.worst_case)
        .bind(card.position_suggestion)
        .bind(card.risk_budget_used)
        .bind(&card.applicable_regime)
        .bind(card.data_freshness_sec)
        .bind(&card.supporting_evidence)
        .bind(&card.opposing_evidence)
        .bind(&card.sample_performance)
        .bind(&card.data_lineage)
        .bind(&card.invalidation_conditions)
        .bind(&card.model_version)
        .bind(card.prediction_id)
        .bind(card.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 查询用户决策卡列表
    pub async fn list_decision_cards_by_user(
        pool: &PgPool,
        user_id: i64,
        limit: i64,
    ) -> Result<Vec<DecisionCard>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT card_id, user_id, symbol, generated_at,
                      suggested_action, target_horizon_sec,
                      p_up, p_down, p_flat, q10, q50, q90,
                      expected_value, worst_case, position_suggestion, risk_budget_used,
                      applicable_regime, data_freshness_sec,
                      supporting_evidence, opposing_evidence, sample_performance, data_lineage,
                      invalidation_conditions, model_version, prediction_id,
                      user_action, user_feedback, acted_at, created_at
               FROM decision_cards
               WHERE user_id = $1
               ORDER BY generated_at DESC
               LIMIT $2"#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

        let cards = rows
            .into_iter()
            .map(|row| DecisionCard {
                card_id: row.get("card_id"),
                user_id: row.get("user_id"),
                symbol: row.get("symbol"),
                generated_at: row.get("generated_at"),
                suggested_action: super::models::SuggestedAction::from_str(
                    row.get::<String, _>("suggested_action").as_str(),
                )
                .unwrap_or(super::models::SuggestedAction::Hold),
                target_horizon_sec: row.get("target_horizon_sec"),
                p_up: row.get("p_up"),
                p_down: row.get("p_down"),
                p_flat: row.get("p_flat"),
                q10: row.get("q10"),
                q50: row.get("q50"),
                q90: row.get("q90"),
                expected_value: row.get("expected_value"),
                worst_case: row.get("worst_case"),
                position_suggestion: row.get("position_suggestion"),
                risk_budget_used: row.get("risk_budget_used"),
                applicable_regime: row.get("applicable_regime"),
                data_freshness_sec: row.get("data_freshness_sec"),
                supporting_evidence: row.get("supporting_evidence"),
                opposing_evidence: row.get("opposing_evidence"),
                sample_performance: row.get("sample_performance"),
                data_lineage: row.get("data_lineage"),
                invalidation_conditions: row.get("invalidation_conditions"),
                model_version: row.get("model_version"),
                prediction_id: row.get("prediction_id"),
                user_action: row.get("user_action"),
                user_feedback: row.get("user_feedback"),
                acted_at: row.get("acted_at"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok(cards)
    }

    /// 更新用户对决策卡的反馈
    pub async fn update_card_user_feedback(
        pool: &PgPool,
        card_id: Uuid,
        user_action: &str,
        user_feedback: Option<&str>,
        acted_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE decision_cards
               SET user_action = $1,
                   user_feedback = $2,
                   acted_at = $3
               WHERE card_id = $4"#,
        )
        .bind(user_action)
        .bind(user_feedback)
        .bind(acted_at)
        .bind(card_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    // =========================================================
    // 校准报告（signal_calibration_reports）
    // =========================================================

    /// 写入校准报告
    pub async fn insert_calibration_report(
        pool: &PgPool,
        report: &CalibrationReport,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO signal_calibration_reports
               (report_id, model_version, symbol, market_regime,
                eval_start, eval_end,
                brier_score, log_loss, accuracy, calibration_error,
                calibration_curve,
                sample_count, up_count, down_count, flat_count,
                is_well_calibrated, degradation_detected, metadata, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)"#,
        )
        .bind(report.report_id)
        .bind(&report.model_version)
        .bind(&report.symbol)
        .bind(&report.market_regime)
        .bind(report.eval_start)
        .bind(report.eval_end)
        .bind(report.brier_score)
        .bind(report.log_loss)
        .bind(report.accuracy)
        .bind(report.calibration_error)
        .bind(&report.calibration_curve)
        .bind(report.sample_count)
        .bind(report.up_count)
        .bind(report.down_count)
        .bind(report.flat_count)
        .bind(report.is_well_calibrated)
        .bind(report.degradation_detected)
        .bind(&report.metadata)
        .bind(report.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 查询最新校准报告
    pub async fn get_latest_calibration(
        pool: &PgPool,
        model_version: &str,
    ) -> Result<Option<CalibrationReport>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT report_id, model_version, symbol, market_regime,
                      eval_start, eval_end,
                      brier_score, log_loss, accuracy, calibration_error,
                      calibration_curve,
                      sample_count, up_count, down_count, flat_count,
                      is_well_calibrated, degradation_detected, metadata, created_at
               FROM signal_calibration_reports
               WHERE model_version = $1
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(model_version)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|row| CalibrationReport {
            report_id: row.get("report_id"),
            model_version: row.get("model_version"),
            symbol: row.get("symbol"),
            market_regime: row.get("market_regime"),
            eval_start: row.get("eval_start"),
            eval_end: row.get("eval_end"),
            brier_score: row.get("brier_score"),
            log_loss: row.get("log_loss"),
            accuracy: row.get("accuracy"),
            calibration_error: row.get("calibration_error"),
            calibration_curve: row.get("calibration_curve"),
            sample_count: row.get("sample_count"),
            up_count: row.get("up_count"),
            down_count: row.get("down_count"),
            flat_count: row.get("flat_count"),
            is_well_calibrated: row.get("is_well_calibrated"),
            degradation_detected: row.get("degradation_detected"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        }))
    }
}
