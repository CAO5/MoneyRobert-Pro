//! Backtest REST API: create jobs, start, check status, fetch report.
//! 回测 API

use crate::backtest::models::BacktestStatus;
use crate::backtest::runner::run_backtest_for_job;
use crate::backtest::trust_engine::{self, TrustAssessmentInput, TrustLevel};
use crate::error::{AppError, Result};
use crate::extractors::CurrentUser;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/{job_id}", get(get_job))
        .route("/jobs/{job_id}/start", post(start_job))
        .route("/jobs/{job_id}/report", get(get_report))
        .route("/jobs/{job_id}/trades", get(get_trades))
        .route("/jobs/{job_id}/signals", post(create_signal))
        .route("/jobs/{job_id}/trust-level", get(get_trust_level).post(assess_trust_level))
}

#[derive(Debug, Deserialize)]
struct CreateJobRequest {
    job_name: String,
    strategy_id: Option<String>,
    assets: Vec<String>,
    exchanges: Option<Vec<String>>,
    start_time: String, // ISO 8601
    end_time: String,
    #[serde(default = "default_initial")]
    initial_equity: f64,
    #[serde(default = "default_freq")]
    data_frequency: String,
    #[serde(default = "default_fee_taker")]
    fee_taker_bps: f64,
    #[serde(default = "default_fee_maker")]
    fee_maker_bps: f64,
    #[serde(default = "default_slippage")]
    slippage_bps: f64,
    #[serde(default = "default_single_position_pct")]
    max_single_position_pct: f64,
    #[serde(default = "default_leverage")]
    max_total_leverage: f64,
    #[serde(default = "default_daily_loss")]
    max_daily_loss_pct: f64,
    #[serde(default = "default_min_conf")]
    min_signal_confidence: f64,
    #[serde(default = "default_min_strength")]
    min_signal_strength: f64,
}

fn default_initial() -> f64 { 100_000.0 }
fn default_freq() -> String { "1h".into() }
fn default_fee_taker() -> f64 { 5.0 }
fn default_fee_maker() -> f64 { 2.0 }
fn default_slippage() -> f64 { 3.0 }
fn default_single_position_pct() -> f64 { 0.1 }
fn default_leverage() -> f64 { 3.0 }
fn default_daily_loss() -> f64 { 0.03 }
fn default_min_conf() -> f64 { 0.3 }
fn default_min_strength() -> f64 { 0.2 }

#[derive(Debug, Serialize)]
struct JobResponse {
    job_id: Uuid,
    status: String,
    job_name: String,
    created_at: String,
}

async fn create_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(req): Json<CreateJobRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = parse_iso_dt(&req.start_time)?;
    let end_time = parse_iso_dt(&req.end_time)?;
    if end_time <= start_time {
        return Err(AppError::Validation("end_time must be after start_time".into()));
    }
    let job_id = Uuid::new_v4();
    let exchange_list: Vec<String> = req.exchanges.clone().unwrap_or_else(|| vec!["binance".into()]);

    let _ = sqlx::query(
        r#"INSERT INTO backtest_jobs
           (job_id, user_id, job_name, strategy_id, assets, exchanges,
            start_time, end_time, initial_equity, base_currency, mode, status,
            progress, data_frequency, fee_model, fee_taker_bps, fee_maker_bps,
            slippage_model, slippage_bps, max_single_position_pct, max_total_leverage,
            max_daily_loss_pct, min_signal_confidence, min_signal_strength, created_at, updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,NOW(),NOW())"#,
    )
    .bind(job_id)
    .bind(user.user_id as i64)
    .bind(&req.job_name)
    .bind(req.strategy_id.as_deref())
    .bind(&req.assets)
    .bind(&exchange_list)
    .bind(start_time.naive_utc())
    .bind(end_time.naive_utc())
    .bind(req.initial_equity)
    .bind("USDT")
    .bind("backtest")
    .bind(BacktestStatus::Created.as_str())
    .bind(0.0f64)
    .bind(&req.data_frequency)
    .bind("fixed")
    .bind(req.fee_taker_bps)
    .bind(req.fee_maker_bps)
    .bind("fixed")
    .bind(req.slippage_bps)
    .bind(req.max_single_position_pct)
    .bind(req.max_total_leverage)
    .bind(req.max_daily_loss_pct)
    .bind(req.min_signal_confidence)
    .bind(req.min_signal_strength)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "job_id": job_id.to_string(),
        "status": BacktestStatus::Created.as_str(),
        "job_name": req.job_name,
    })))
}

async fn list_jobs(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query(
        r#"SELECT job_id, job_name, strategy_id, status, progress, start_time, end_time,
                  initial_equity, total_trades, total_return_pct, sharpe_ratio, max_drawdown_pct,
                  created_at
           FROM backtest_jobs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50"#,
    )
    .bind(user.user_id as i64)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let jobs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let start: NaiveDateTime = r.get(5);
            let end: NaiveDateTime = r.get(6);
            let created: NaiveDateTime = r.get(12);
            serde_json::json!({
                "job_id": r.get::<Uuid, _>(0).to_string(),
                "job_name": r.get::<String, _>(1),
                "strategy_id": r.get::<Option<String>, _>(2),
                "status": r.get::<String, _>(3),
                "progress": r.get::<f64, _>(4),
                "start_time": DateTime::<Utc>::from_naive_utc_and_offset(start, Utc).to_rfc3339(),
                "end_time": DateTime::<Utc>::from_naive_utc_and_offset(end, Utc).to_rfc3339(),
                "initial_equity": r.get::<f64, _>(7),
                "total_trades": r.get::<Option<i64>, _>(8),
                "total_return_pct": r.get::<Option<f64>, _>(9),
                "sharpe_ratio": r.get::<Option<f64>, _>(10),
                "max_drawdown_pct": r.get::<Option<f64>, _>(11),
                "created_at": DateTime::<Utc>::from_naive_utc_and_offset(created, Utc).to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

async fn get_job(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let row = sqlx::query(
        r#"SELECT job_id, job_name, strategy_id, status, progress, start_time, end_time,
                  initial_equity, total_trades, winning_trades, total_return_pct, sharpe_ratio,
                  max_drawdown_pct, fee_total, slippage_total, created_at, completed_at, mode,
                  data_frequency, fee_taker_bps, fee_maker_bps, slippage_bps,
                  max_single_position_pct, max_total_leverage, max_daily_loss_pct, assets
           FROM backtest_jobs WHERE job_id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let r = row.ok_or_else(|| AppError::NotFound("backtest job not found".to_string()))?;
    let start: NaiveDateTime = r.get(5);
    let end: NaiveDateTime = r.get(6);
    let created: NaiveDateTime = r.get(15);
    let completed: Option<NaiveDateTime> = r.get(16);

    Ok(Json(serde_json::json!({
        "job_id": r.get::<Uuid, _>(0).to_string(),
        "job_name": r.get::<String, _>(1),
        "strategy_id": r.get::<Option<String>, _>(2),
        "status": r.get::<String, _>(3),
        "progress": r.get::<f64, _>(4),
        "start_time": DateTime::<Utc>::from_naive_utc_and_offset(start, Utc).to_rfc3339(),
        "end_time": DateTime::<Utc>::from_naive_utc_and_offset(end, Utc).to_rfc3339(),
        "initial_equity": r.get::<f64, _>(7),
        "total_trades": r.get::<Option<i64>, _>(8),
        "winning_trades": r.get::<Option<i64>, _>(9),
        "total_return_pct": r.get::<Option<f64>, _>(10),
        "sharpe_ratio": r.get::<Option<f64>, _>(11),
        "max_drawdown_pct": r.get::<Option<f64>, _>(12),
        "fee_total": r.get::<Option<f64>, _>(13),
        "slippage_total": r.get::<Option<f64>, _>(14),
        "created_at": DateTime::<Utc>::from_naive_utc_and_offset(created, Utc).to_rfc3339(),
        "completed_at": completed.map(|c| DateTime::<Utc>::from_naive_utc_and_offset(c, Utc).to_rfc3339()),
        "mode": r.get::<String, _>(17),
        "data_frequency": r.get::<String, _>(18),
        "fee_taker_bps": r.get::<f64, _>(19),
        "fee_maker_bps": r.get::<f64, _>(20),
        "slippage_bps": r.get::<f64, _>(21),
        "max_single_position_pct": r.get::<f64, _>(22),
        "max_total_leverage": r.get::<f64, _>(23),
        "max_daily_loss_pct": r.get::<f64, _>(24),
        "assets": r.get::<Vec<String>, _>(25),
    })))
}

async fn start_job(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    // Load the job row
    let row = sqlx::query(
        r#"SELECT job_id, job_name, strategy_id, assets, start_time, end_time,
                  initial_equity, data_frequency, fee_taker_bps, fee_maker_bps,
                  slippage_bps, max_single_position_pct, max_total_leverage,
                  max_daily_loss_pct, min_signal_confidence, min_signal_strength, status
           FROM backtest_jobs WHERE job_id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("not found".to_string()))?;

    let start: NaiveDateTime = row.get(4);
    let end: NaiveDateTime = row.get(5);

    let job = crate::backtest::models::BacktestJob {
        job_id: row.get(0),
        job_name: row.get(1),
        strategy_id: row.get(2),
        assets: row.get(3),
        start_time: DateTime::<Utc>::from_naive_utc_and_offset(start, Utc),
        end_time: DateTime::<Utc>::from_naive_utc_and_offset(end, Utc),
        initial_equity: row.get(6),
        data_frequency: row.get(7),
        fee_taker_bps: row.get(8),
        fee_maker_bps: row.get(9),
        slippage_bps: row.get(10),
        max_single_position_pct: row.get(11),
        max_total_leverage: row.get(12),
        max_daily_loss_pct: row.get(13),
        min_signal_confidence: row.get(14),
        min_signal_strength: row.get(15),
        status: BacktestStatus::Running,
        ..Default::default()
    };

    // Set status to running
    let _ = sqlx::query(
        "UPDATE backtest_jobs SET status = $1, started_at = NOW(), updated_at = NOW() WHERE job_id = $2",
    )
    .bind(BacktestStatus::Running.as_str())
    .bind(job_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    // Execute the backtest synchronously for now (in a real deployment, spawn a task).
    let pool = state.db_pool.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = run_backtest_for_job(&pool, job).await {
            let _ = sqlx::query(
                "UPDATE backtest_jobs SET status = $1, error_message = $2, updated_at = NOW() WHERE job_id = $3",
            )
            .bind(BacktestStatus::Failed.as_str())
            .bind(e.clone())
            .bind(job_id)
            .execute(&pool)
            .await;
        }
    });

    Ok(Json(serde_json::json!({
        "job_id": job_id.to_string(),
        "status": "running",
        "message": "backtest started (async)"
    })))
}

async fn get_report(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let row = sqlx::query(
        r#"SELECT report_id, total_return, annualized_return, max_drawdown, sharpe_ratio,
                  win_rate, profit_factor, total_trades, winning_trades, losing_trades,
                  average_win, average_loss, payoff_ratio, total_fee,
                  by_agent, by_asset, report_json, created_at
           FROM performance_reports WHERE job_id = $1 ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(job_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "report_id": row.get::<Uuid, _>(0).to_string(),
        "total_return": row.get::<Option<f64>, _>(1),
        "annualized_return": row.get::<Option<f64>, _>(2),
        "max_drawdown": row.get::<Option<f64>, _>(3),
        "sharpe_ratio": row.get::<Option<f64>, _>(4),
        "win_rate": row.get::<Option<f64>, _>(5),
        "profit_factor": row.get::<Option<f64>, _>(6),
        "total_trades": row.get::<Option<i64>, _>(7),
        "winning_trades": row.get::<Option<i64>, _>(8),
        "losing_trades": row.get::<Option<i64>, _>(9),
        "average_win": row.get::<Option<f64>, _>(10),
        "average_loss": row.get::<Option<f64>, _>(11),
        "payoff_ratio": row.get::<Option<f64>, _>(12),
        "total_fee": row.get::<Option<f64>, _>(13),
        "by_agent": row.get::<Option<serde_json::Value>, _>(14),
        "by_asset": row.get::<Option<serde_json::Value>, _>(15),
        "report": row.get::<Option<serde_json::Value>, _>(16),
    })))
}

async fn get_trades(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query(
        r#"SELECT attribution_id, asset, strategy_id, agent_id, direction,
                  entry_time, exit_time, entry_price, exit_price, quantity,
                  pnl, pnl_bps, fee_total, holding_period_sec, result, exit_reason
           FROM trade_attributions WHERE job_id = $1 ORDER BY entry_time DESC LIMIT 500"#,
    )
    .bind(job_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    let trades: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let entry: NaiveDateTime = r.get(5);
            let exit: Option<NaiveDateTime> = r.get(6);
            serde_json::json!({
                "attribution_id": r.get::<Uuid, _>(0).to_string(),
                "asset": r.get::<String, _>(1),
                "strategy_id": r.get::<Option<String>, _>(2),
                "agent_id": r.get::<Option<String>, _>(3),
                "direction": r.get::<String, _>(4),
                "entry_time": DateTime::<Utc>::from_naive_utc_and_offset(entry, Utc).to_rfc3339(),
                "exit_time": exit.map(|c| DateTime::<Utc>::from_naive_utc_and_offset(c, Utc).to_rfc3339()),
                "entry_price": r.get::<f64, _>(7),
                "exit_price": r.get::<Option<f64>, _>(8),
                "quantity": r.get::<f64, _>(9),
                "pnl": r.get::<Option<f64>, _>(10),
                "pnl_bps": r.get::<Option<f64>, _>(11),
                "fee_total": r.get::<f64, _>(12),
                "holding_period_sec": r.get::<Option<i64>, _>(13),
                "result": r.get::<Option<String>, _>(14),
                "exit_reason": r.get::<Option<String>, _>(15),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "trades": trades })))
}

#[derive(Debug, Deserialize)]
struct CreateSignalRequest {
    asset: String,
    exchange: Option<String>,
    direction: String, // "long" | "short"
    signal_strength: Option<f64>,
    confidence: Option<f64>,
    expected_return_bps: Option<f64>,
    expected_holding_period_sec: Option<i64>,
    strategy_id: Option<String>,
    agent_id: Option<String>,
    market_regime: Option<String>,
    explanation: Option<String>,
    #[serde(default = "default_event_time")]
    event_time: String,
    valid_until: Option<String>,
}

fn default_event_time() -> String {
    Utc::now().to_rfc3339()
}

async fn create_signal(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(req): Json<CreateSignalRequest>,
) -> Result<Json<serde_json::Value>> {
    let signal_id = Uuid::new_v4();
    let event_time = parse_iso_dt(&req.event_time)?;
    let valid_until = match &req.valid_until {
        Some(s) => Some(parse_iso_dt(s)?.naive_utc()),
        None => None,
    };

    let _ = sqlx::query(
        r#"INSERT INTO alpha_signals
           (signal_id, job_id, strategy_id, agent_id, asset, exchange, event_time,
            valid_until, direction, signal_strength, confidence, expected_return_bps,
            expected_holding_period_sec, market_regime, explanation, created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,NOW())"#,
    )
    .bind(signal_id)
    .bind(job_id)
    .bind(req.strategy_id.as_deref())
    .bind(req.agent_id.as_deref())
    .bind(&req.asset)
    .bind(req.exchange.as_deref())
    .bind(event_time.naive_utc())
    .bind(valid_until)
    .bind(&req.direction)
    .bind(req.signal_strength)
    .bind(req.confidence)
    .bind(req.expected_return_bps)
    .bind(req.expected_holding_period_sec)
    .bind(req.market_regime.as_deref())
    .bind(req.explanation.as_deref())
    .execute(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?;

    Ok(Json(serde_json::json!({
        "signal_id": signal_id.to_string(),
        "job_id": job_id.to_string(),
        "status": "created",
    })))
}

fn parse_iso_dt(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            s.parse::<DateTime<Utc>>()
        })
        .map_err(|_| AppError::Validation(format!("invalid datetime: {}", s)))
}

// =========================================================
// 回测可信等级 API
// =========================================================

/// 可信等级响应
#[derive(Debug, Serialize)]
struct TrustLevelResponse {
    assessment_id: String,
    job_id: String,
    trust_level: String,
    test_coverage_passed: bool,
    capital_conservation_passed: bool,
    slippage_accounted: bool,
    data_quality_grade: String,
    sample_size_sufficient: bool,
    walk_forward_validated: bool,
    calibration_healthy: bool,
    total_trades: i32,
    test_pass_rate: f64,
    data_coverage_ratio: f64,
    issues: serde_json::Value,
    recommendations: serde_json::Value,
    promotion_eligible: bool,
    promotion_blockers: serde_json::Value,
    assessed_at: String,
}

impl From<crate::backtest::trust_engine::TrustAssessment> for TrustLevelResponse {
    fn from(a: crate::backtest::trust_engine::TrustAssessment) -> Self {
        Self {
            assessment_id: a.assessment_id.to_string(),
            job_id: a.job_id.to_string(),
            trust_level: a.trust_level.as_str().to_string(),
            test_coverage_passed: a.test_coverage_passed,
            capital_conservation_passed: a.capital_conservation_passed,
            slippage_accounted: a.slippage_accounted,
            data_quality_grade: a.data_quality_grade,
            sample_size_sufficient: a.sample_size_sufficient,
            walk_forward_validated: a.walk_forward_validated,
            calibration_healthy: a.calibration_healthy,
            total_trades: a.total_trades,
            test_pass_rate: a.test_pass_rate,
            data_coverage_ratio: a.data_coverage_ratio,
            issues: a.issues,
            recommendations: a.recommendations,
            promotion_eligible: a.promotion_eligible,
            promotion_blockers: a.promotion_blockers,
            assessed_at: a.assessed_at.to_rfc3339(),
        }
    }
}

/// 查询回测可信等级
async fn get_trust_level(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<TrustLevelResponse>> {
    let assessment = trust_engine::get_assessment(&state.db_pool, job_id)
        .await
        .map_err(|e| AppError::Internal(format!("get trust assessment failed: {}", e)))?
        .ok_or_else(|| AppError::NotFound("可信等级评估不存在，请先触发评估".to_string()))?;

    Ok(Json(assessment.into()))
}

/// 评估回测可信等级请求
#[derive(Debug, Deserialize)]
struct AssessTrustRequest {
    /// 测试通过率（0-1），可选
    test_pass_rate: Option<f64>,
    /// 数据覆盖率（0-1），可选
    data_coverage_ratio: Option<f64>,
    /// 数据质量等级，可选
    data_quality_grade: Option<String>,
    /// 是否通过 Walk-forward 验证
    walk_forward_validated: Option<bool>,
    /// 概率校准是否健康
    calibration_healthy: Option<bool>,
}

/// 触发回测可信等级评估
/// 根据回测结果和外部输入（测试通过率、数据质量等）评估可信等级
async fn assess_trust_level(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(req): Json<AssessTrustRequest>,
) -> Result<Json<TrustLevelResponse>> {
    // 从数据库读取回测结果
    let row = sqlx::query(
        r#"SELECT total_trades, fee_total, slippage_total
           FROM backtest_jobs WHERE job_id = $1"#,
    )
    .bind(job_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::Database(e))?
    .ok_or_else(|| AppError::NotFound("回测任务不存在".to_string()))?;

    let total_trades: i32 = row.try_get("total_trades").unwrap_or(0);
    let total_fee: f64 = row.try_get("fee_total").unwrap_or(0.0);
    let total_slippage_cost: f64 = row.try_get("slippage_total").unwrap_or(0.0);

    // 构建评估输入
    let input = TrustAssessmentInput {
        job_id,
        total_trades,
        total_slippage_cost,
        total_fee,
        test_pass_rate: req.test_pass_rate,
        data_coverage_ratio: req.data_coverage_ratio,
        data_quality_grade: req.data_quality_grade,
        walk_forward_validated: req.walk_forward_validated.unwrap_or(false),
        calibration_healthy: req.calibration_healthy.unwrap_or(false),
    };

    // 评估可信等级
    let assessment = trust_engine::assess_trust(&input);

    // 保存评估结果
    trust_engine::save_assessment(&state.db_pool, &assessment)
        .await
        .map_err(|e| AppError::Internal(format!("save trust assessment failed: {}", e)))?;

    Ok(Json(assessment.into()))
}
