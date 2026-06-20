//! End-to-end backtest runner: orchestrates replay, strategy, risk, matching, account, performance.
//! 端到端回测运行器

use crate::backtest::account_engine::{kline_prices, AccountEngine};
use crate::backtest::matching_engine::{MatchingConfig, MatchingEngine};
use crate::backtest::models::{
    AccountState, AlphaSignal, BacktestJob, BacktestStatus, PerformanceReport, SimulatedFill,
    SimulatedOrder, TradeAttribution, TradeIntent,
};
use crate::backtest::performance_engine::PerformanceEngine;
use crate::backtest::replay_engine::{ReplayConfig, ReplayEngine};
use crate::backtest::risk_engine::{RiskConfig, RiskEngine};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

pub struct BacktestRunnerConfig {
    pub job_id: Uuid,
    pub initial_equity: f64,
    pub symbols: Vec<String>,
    pub interval: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub matching: MatchingConfig,
    pub risk: RiskConfig,
    pub min_signal_confidence: f64,
    pub min_signal_strength: f64,
}

pub struct BacktestRunner {
    config: BacktestRunnerConfig,
    matching: MatchingEngine,
    risk: RiskEngine,
    account: AccountEngine,
    open_orders: Vec<SimulatedOrder>,
    closed_trades: Vec<TradeAttribution>,
    last_kline_by_asset: HashMap<String, crate::backtest::models::Kline>,
    total_fee: f64,
    total_slippage_bps_sum: f64,
    total_fills: i64,
}

impl BacktestRunner {
    pub fn new(config: BacktestRunnerConfig) -> Self {
        let now = config.start_time;
        Self {
            matching: MatchingEngine::new(config.matching.clone()),
            risk: RiskEngine::new(config.risk.clone()),
            account: AccountEngine::new(config.job_id, config.initial_equity, now),
            open_orders: Vec::new(),
            closed_trades: Vec::new(),
            last_kline_by_asset: HashMap::new(),
            total_fee: 0.0,
            total_slippage_bps_sum: 0.0,
            total_fills: 0,
            config,
        }
    }

    /// Entry point: run the full backtest. Caller should have set job status to 'running'
    /// and update progress periodically.
    pub async fn run(&mut self, pool: &PgPool) -> Result<PerformanceReport, String> {
        info!("[BT] Starting backtest job_id={}", self.config.job_id);
        let replay_cfg = ReplayConfig {
            start_time: self.config.start_time,
            end_time: self.config.end_time,
            symbols: self.config.symbols.clone(),
            interval: self.config.interval.clone(),
        };
        let mut replay =
            ReplayEngine::load(pool, replay_cfg, Some(self.config.job_id)).await?;
        info!("[BT] loaded {} events", replay.total_events());

        // Reset daily stats at start
        self.risk
            .reset_daily(self.account.state.realized_pnl, self.account.state.total_equity);

        // Iterate events
        while let Some(ev) = replay.next() {
            match ev {
                crate::backtest::models::ReplayEvent::Kline { kline } => {
                    self.handle_kline(&kline).await;
                }
                crate::backtest::models::ReplayEvent::Signal { signal } => {
                    self.handle_signal(&signal).await;
                }
            }
        }

        // Force-close remaining open positions at the end price
        let mut prices = HashMap::new();
        for (sym, k) in &self.last_kline_by_asset {
            prices.insert(sym.clone(), k.close);
        }
        self.account
            .force_close_all(&prices, self.config.end_time);

        // Build final performance report
        let perf = PerformanceEngine;
        let report = perf.compute_report(
            self.closed_trades.clone(),
            self.account.snapshots.clone(),
            self.config.start_time,
            self.config.end_time,
            self.total_fee,
        );
        info!(
            "[BT] backtest done: trades={}, ret={:.2}%, sharpe={:.3}, dd={:.2}%",
            report.total_trades,
            report.total_return * 100.0,
            report.sharpe_ratio,
            report.max_drawdown * 100.0
        );
        Ok(report)
    }

    async fn handle_kline(&mut self, kline: &crate::backtest::models::Kline) {
        // 1) update market state
        self.last_kline_by_asset.insert(kline.symbol.clone(), kline.clone());
        let prices = kline_prices(&[kline.clone()]);
        self.account.mark_to_market(&prices, kline.open_time);

        // 2) process stop-loss / take-profit for any open position on this asset
        let triggers = self.account.check_stops(&prices);
        let mut stop_fills: Vec<(crate::backtest::models::SimulatedFill, String)> = Vec::new();
        for (pos_id, _asset, price) in triggers {
            if let Some(pos) = self
                .account
                .positions
                .iter()
                .find(|p| p.position_id == pos_id && p.closed_at.is_none())
                .cloned()
            {
                let fill = self
                    .matching
                    .close_position_at_price(&pos, price, kline.open_time);
                stop_fills.push((fill, pos.asset.clone()));
            }
        }
        for (fill, asset) in stop_fills {
            self.apply_fill_to_account(&asset, &fill, Some("stop_or_tp".into()));
        }

        // 3) match any previously submitted limit orders for this asset against current K-line
        let orders_to_check: Vec<SimulatedOrder> = self.open_orders.drain(..).collect();
        let mut fills_to_apply: Vec<crate::backtest::models::SimulatedFill> = Vec::new();
        for order in orders_to_check {
            if order.asset != kline.symbol {
                self.open_orders.push(order);
                continue;
            }
            if order.order_type == "limit" {
                if let Some(fill) = self.matching.fill_limit_order(&order, kline, kline.open_time) {
                    fills_to_apply.push(fill);
                } else {
                    self.open_orders.push(order);
                }
            } else {
                // market orders - should have been already filled when submitted; skip
            }
        }
        for fill in fills_to_apply {
            self.apply_fill_to_account(&fill.asset.clone(), &fill, None);
        }

        self.account.record_snapshot();
    }

    async fn handle_signal(&mut self, signal: &AlphaSignal) {
        // Validate signal
        if signal.is_hold() || signal.is_expired(&signal.event_time) {
            return;
        }
        let conf = signal.confidence.unwrap_or(0.0);
        let strength = signal.signal_strength.unwrap_or(0.0);
        if conf < self.config.min_signal_confidence || strength < self.config.min_signal_strength {
            return;
        }

        // Convert signal -> trade intent
        let kline = match self.last_kline_by_asset.get(&signal.asset) {
            Some(k) => k.clone(),
            None => return, // no price data for this asset yet
        };
        let current_price = kline.close;

        let side = if signal.is_long() { "buy" } else { "sell" };
        let position_pct = (0.05 * strength).min(self.config.risk.max_single_position_pct);
        let notional = self.account.state.total_equity.max(0.0) * position_pct;
        if notional <= 0.0 || current_price <= 0.0 {
            return;
        }
        let quantity = notional / current_price;

        let intent = TradeIntent {
            intent_id: Uuid::new_v4(),
            job_id: signal.job_id,
            source_signal_id: Some(signal.signal_id),
            strategy_id: signal.strategy_id.clone(),
            agent_id: signal.agent_id.clone(),
            asset: signal.asset.clone(),
            exchange: signal.exchange.clone(),
            side: side.into(),
            intent_type: "open_position".into(),
            target_position_pct: Some(position_pct),
            target_notional: Some(notional),
            target_quantity: Some(quantity),
            order_type: "market".into(),
            limit_price: None,
            max_slippage_bps: None,
            leverage: 1,
            stop_loss_price: signal
                .expected_return_bps
                .map(|bps| current_price * (1.0 - bps / 10000.0 * 2.0)),
            take_profit_price: signal
                .expected_return_bps
                .map(|bps| current_price * (1.0 + bps / 10000.0)),
            event_time: signal.event_time,
        };

        // Risk check
        let existing_notional = self.account.open_position_notional_for_asset(&intent.asset);
        let risk_result = self.risk.validate_intent(&intent, &self.account.state, existing_notional);
        if !risk_result.passed {
            warn!(
                "[BT] signal {} rejected by risk: {:?}",
                signal.signal_id, risk_result.reasons
            );
            return;
        }

        // Build order
        let effective_notional = risk_result.reduced_notional.unwrap_or(notional);
        let effective_qty = effective_notional / current_price;
        if effective_qty <= 1e-9 {
            return;
        }

        let order = SimulatedOrder {
            order_id: Uuid::new_v4(),
            job_id: signal.job_id,
            intent_id: Some(intent.intent_id),
            source_signal_id: Some(signal.signal_id),
            strategy_id: signal.strategy_id.clone(),
            agent_id: signal.agent_id.clone(),
            asset: signal.asset.clone(),
            exchange: signal.exchange.clone(),
            side: side.into(),
            order_type: "market".into(),
            price: Some(current_price),
            quantity: effective_qty,
            notional: Some(effective_notional),
            filled_quantity: 0.0,
            filled_price: None,
            fee: 0.0,
            slippage_bps: None,
            leverage: 1,
            stop_loss: intent.stop_loss_price,
            take_profit: intent.take_profit_price,
            status: "submitted".into(),
            submitted_at: signal.event_time,
            filled_at: None,
        };

        // Immediate market-fill at next K-line open (which is current_price for the step):
        if let Some(fill) = self.matching.fill_market_order(&order, &kline, signal.event_time) {
            self.apply_fill_to_account(&order.asset, &fill, Some(intent.intent_type.clone()));
        }
    }

    fn apply_fill_to_account(
        &mut self,
        _asset: &str,
        fill: &crate::backtest::models::SimulatedFill,
        intent_type: Option<String>,
    ) {
        // If this fill closes an existing position, record a trade attribution.
        let before_positions: Vec<(Uuid, String, f64, f64)> = self
            .account
            .positions
            .iter()
            .map(|p| (p.position_id, p.side.clone(), p.quantity, p.avg_entry_price))
            .collect();

        let closed_id_before: std::collections::HashSet<Uuid> = self
            .account
            .positions
            .iter()
            .filter(|p| p.closed_at.is_some())
            .map(|p| p.position_id)
            .collect();

        let (_new_pos, _pnl) =
            self.matching
                .apply_fill(fill, &mut self.account.positions, &mut self.account.state);

        // Track newly closed positions
        for (pid, side, qty, avg_price) in &before_positions {
            let pos = self.account.positions.iter().find(|p| p.position_id == *pid);
            if let Some(pos) = pos {
                if pos.closed_at.is_some() && !closed_id_before.contains(pid) && *qty > 0.0 {
                    // Closed this step
                    let pnl = pos.realized_pnl;
                    let pnl_bps = if avg_price > &0.0 {
                        pnl / (avg_price * qty) * 10000.0
                    } else {
                        0.0
                    };
                    let seconds = (fill.fill_time - pos.opened_at).num_seconds();
                    self.closed_trades.push(TradeAttribution {
                        attribution_id: Uuid::new_v4(),
                        job_id: fill.job_id,
                        asset: fill.asset.clone(),
                        strategy_id: fill.strategy_id.clone(),
                        agent_id: fill.agent_id.clone(),
                        direction: side.clone(),
                        entry_time: pos.opened_at,
                        exit_time: Some(fill.fill_time),
                        entry_price: *avg_price,
                        exit_price: Some(fill.filled_price),
                        quantity: *qty,
                        pnl: Some(pnl),
                        pnl_bps: Some(pnl_bps),
                        fee_total: fill.fee,
                        holding_period_sec: Some(seconds),
                        signal_confidence: None,
                        signal_strength: None,
                        entry_signal_id: pos.open_signal_id,
                        exit_reason: intent_type.clone(),
                        result: Some(if pnl > 0.0 { "win".into() } else { "loss".into() }),
                    });
                }
            }
        }

        self.total_fee += fill.fee;
        self.total_slippage_bps_sum += fill.slippage_bps.unwrap_or(0.0);
        self.total_fills += 1;

        // recompute equity
        let mut prices = HashMap::new();
        for (sym, k) in &self.last_kline_by_asset {
            prices.insert(sym.clone(), k.close);
        }
        self.account.mark_to_market(&prices, fill.fill_time);
        self.account.record_snapshot();
    }

    pub fn account_state(&self) -> &AccountState {
        &self.account.state
    }
    pub fn closed_trades(&self) -> &[TradeAttribution] {
        &self.closed_trades
    }
}

/// Convenience helper: create & execute backtest job, store results to DB.
pub async fn run_backtest_for_job(pool: &PgPool, mut job: BacktestJob) -> Result<(), String> {
    let symbols = if job.assets.is_empty() {
        // Fallback: fetch from DB
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT symbol FROM market_data WHERE open_time BETWEEN $1 AND $2 LIMIT 5",
        )
        .bind(job.start_time.naive_utc())
        .bind(job.end_time.naive_utc())
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        rows
    } else {
        job.assets.clone()
    };

    let runner_config = BacktestRunnerConfig {
        job_id: job.job_id,
        initial_equity: job.initial_equity,
        symbols,
        interval: job.data_frequency.clone(),
        start_time: job.start_time,
        end_time: job.end_time,
        matching: MatchingConfig {
            fee_taker_bps: job.fee_taker_bps,
            fee_maker_bps: job.fee_maker_bps,
            slippage_bps: job.slippage_bps,
        },
        risk: RiskConfig {
            max_single_position_pct: job.max_single_position_pct,
            max_total_leverage: job.max_total_leverage,
            max_daily_loss_pct: job.max_daily_loss_pct,
            min_signal_confidence: job.min_signal_confidence,
            min_signal_strength: job.min_signal_strength,
        },
        min_signal_confidence: job.min_signal_confidence,
        min_signal_strength: job.min_signal_strength,
    };

    let mut runner = BacktestRunner::new(runner_config);
    let report = runner.run(pool).await?;

    // Update backtest_jobs row with result summary
    let _ = sqlx::query(
        r#"UPDATE backtest_jobs
           SET status = $1, total_trades = $2, winning_trades = $3,
               total_return_pct = $4, sharpe_ratio = $5, max_drawdown_pct = $6,
               completed_at = $7, updated_at = NOW(), fee_total = $8
           WHERE job_id = $9"#,
    )
    .bind(BacktestStatus::Completed.as_str())
    .bind(report.total_trades)
    .bind(report.winning_trades)
    .bind(Some(report.total_return))
    .bind(Some(report.sharpe_ratio))
    .bind(Some(report.max_drawdown))
    .bind(Some(job.end_time.naive_utc()))
    .bind(report.total_fee)
    .bind(job.job_id)
    .execute(pool)
    .await
    .map_err(|e| format!("update job failed: {}", e))?;

    // Store report JSON into performance_reports
    let report_json = serde_json::to_value(&report).unwrap_or(serde_json::json!(null));
    let _ = sqlx::query(
        r#"INSERT INTO performance_reports
           (report_id, job_id, total_return, annualized_return, max_drawdown, sharpe_ratio,
            win_rate, profit_factor, total_trades, winning_trades, losing_trades,
            average_win, average_loss, payoff_ratio, total_fee, by_agent, by_asset,
            report_json, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, NOW())"#,
    )
    .bind(Uuid::new_v4())
    .bind(job.job_id)
    .bind(Some(report.total_return))
    .bind(Some(report.annualized_return))
    .bind(Some(report.max_drawdown))
    .bind(Some(report.sharpe_ratio))
    .bind(Some(report.win_rate))
    .bind(Some(report.profit_factor))
    .bind(Some(report.total_trades))
    .bind(Some(report.winning_trades))
    .bind(Some(report.losing_trades))
    .bind(Some(report.average_win))
    .bind(Some(report.average_loss))
    .bind(Some(report.payoff_ratio))
    .bind(Some(report.total_fee))
    .bind(report.by_agent.clone())
    .bind(report.by_asset.clone())
    .bind(report_json)
    .execute(pool)
    .await
    .map_err(|e| format!("insert report failed: {}", e))?;

    Ok(())
}
