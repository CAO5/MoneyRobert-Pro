//! Backtest / Trading Simulation core data models
//! 回测核心数据模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ============================================================
// 1. Backtest job
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BacktestJob {
    pub job_id: Uuid,
    pub user_id: Option<i64>,
    pub job_name: String,
    pub strategy_id: Option<String>,
    pub strategy_version: Option<String>,
    pub assets: Vec<String>,
    pub exchanges: Vec<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub initial_equity: f64,
    pub base_currency: String,
    pub mode: String,
    pub status: BacktestStatus,
    pub progress: f64,
    pub data_frequency: String,

    pub fee_model: String,
    pub fee_taker_bps: f64,
    pub fee_maker_bps: f64,
    pub slippage_model: String,
    pub slippage_bps: f64,
    pub max_single_position_pct: f64,
    pub max_total_leverage: f64,
    pub max_daily_loss_pct: f64,
    pub min_signal_confidence: f64,
    pub min_signal_strength: f64,

    pub total_trades: i64,
    pub winning_trades: i64,
    pub total_return_pct: Option<f64>,
    pub sharpe_ratio: Option<f64>,
    pub max_drawdown_pct: Option<f64>,

    pub error_message: Option<String>,
    pub config: Option<Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BacktestStatus {
    #[default]
    Created,
    Validating,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl BacktestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BacktestStatus::Created => "created",
            BacktestStatus::Validating => "validating",
            BacktestStatus::Running => "running",
            BacktestStatus::Paused => "paused",
            BacktestStatus::Completed => "completed",
            BacktestStatus::Failed => "failed",
            BacktestStatus::Cancelled => "cancelled",
        }
    }
}

// ============================================================
// 2. Alpha signal
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlphaSignal {
    pub signal_id: Uuid,
    pub job_id: Option<Uuid>,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub asset: String,
    pub exchange: Option<String>,
    pub timeframe: Option<String>,
    pub event_time: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub direction: String, // "long" | "short" | "hold"
    pub signal_strength: Option<f64>,
    pub confidence: Option<f64>,
    pub expected_return_bps: Option<f64>,
    pub expected_holding_period_sec: Option<i64>,
    pub market_regime: Option<String>,
    pub features_used: Option<Value>,
    pub risk_flags: Option<Value>,
    pub explanation: Option<String>,
}

impl AlphaSignal {
    pub fn is_long(&self) -> bool {
        self.direction.eq_ignore_ascii_case("long")
    }
    pub fn is_short(&self) -> bool {
        self.direction.eq_ignore_ascii_case("short")
    }
    pub fn is_hold(&self) -> bool {
        self.direction.eq_ignore_ascii_case("hold")
    }
    pub fn is_expired(&self, now: &DateTime<Utc>) -> bool {
        self.valid_until.map(|v| v < *now).unwrap_or(false)
    }
}

// ============================================================
// 3. Trade Intent
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeIntent {
    pub intent_id: Uuid,
    pub job_id: Option<Uuid>,
    pub source_signal_id: Option<Uuid>,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub asset: String,
    pub exchange: Option<String>,
    pub side: String,        // "buy" | "sell"
    pub intent_type: String, // "open_position" | "increase_position" | "reduce_position" | "close_position" | "stop_loss" | "take_profit"
    pub target_position_pct: Option<f64>,
    pub target_notional: Option<f64>,
    pub target_quantity: Option<f64>,
    pub order_type: String, // "market" | "limit" | "stop"
    pub limit_price: Option<f64>,
    pub max_slippage_bps: Option<f64>,
    pub leverage: i32,
    pub stop_loss_price: Option<f64>,
    pub take_profit_price: Option<f64>,
    pub event_time: DateTime<Utc>,
}

// ============================================================
// 4. Simulated Order / Fill / Position
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimulatedOrder {
    pub order_id: Uuid,
    pub job_id: Option<Uuid>,
    pub intent_id: Option<Uuid>,
    pub source_signal_id: Option<Uuid>,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub asset: String,
    pub exchange: Option<String>,
    pub side: String,
    pub order_type: String,
    pub price: Option<f64>,
    pub quantity: f64,
    pub notional: Option<f64>,
    pub filled_quantity: f64,
    pub filled_price: Option<f64>,
    pub fee: f64,
    pub slippage_bps: Option<f64>,
    pub leverage: i32,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub filled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimulatedFill {
    pub fill_id: Uuid,
    pub order_id: Uuid,
    pub job_id: Option<Uuid>,
    pub asset: String,
    pub exchange: Option<String>,
    pub side: String,
    pub filled_quantity: f64,
    pub filled_price: f64,
    pub notional: Option<f64>,
    pub fee: f64,
    /// 滑点（基点）
    pub slippage_bps: Option<f64>,
    /// 滑点成本金额（= notional * slippage_bps / 10000），用于绩效归因
    pub slippage_cost: Option<f64>,
    pub maker_taker: String,
    pub signal_id: Option<Uuid>,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub intent_type: Option<String>,
    pub fill_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimulatedPosition {
    pub position_id: Uuid,
    pub job_id: Option<Uuid>,
    pub asset: String,
    pub exchange: Option<String>,
    pub side: String, // "long" | "short"
    pub quantity: f64,
    pub avg_entry_price: f64,
    pub mark_price: Option<f64>,
    pub notional: Option<f64>,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub margin: Option<f64>,
    pub leverage: i32,
    pub liquidation_price: Option<f64>,
    pub stop_loss_price: Option<f64>,
    pub take_profit_price: Option<f64>,
    pub open_signal_id: Option<Uuid>,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

impl SimulatedPosition {
    pub fn update_mark_to_market(&mut self, current_price: f64) {
        self.mark_price = Some(current_price);
        self.notional = Some(self.quantity * current_price);
        let signed_pnl = if self.side == "long" {
            (current_price - self.avg_entry_price) * self.quantity
        } else {
            (self.avg_entry_price - current_price) * self.quantity
        };
        self.unrealized_pnl = signed_pnl;
    }

    pub fn is_liquidating(&self, current_price: f64) -> bool {
        if let Some(liq) = self.liquidation_price {
            if self.side == "long" && current_price <= liq {
                return true;
            }
            if self.side == "short" && current_price >= liq {
                return true;
            }
        }
        false
    }

    pub fn is_stop_loss_triggered(&self, current_price: f64) -> bool {
        if let Some(sl) = self.stop_loss_price {
            if self.side == "long" && current_price <= sl {
                return true;
            }
            if self.side == "short" && current_price >= sl {
                return true;
            }
        }
        false
    }

    pub fn is_take_profit_triggered(&self, current_price: f64) -> bool {
        if let Some(tp) = self.take_profit_price {
            if self.side == "long" && current_price >= tp {
                return true;
            }
            if self.side == "short" && current_price <= tp {
                return true;
            }
        }
        false
    }
}

// ============================================================
// 5. Account state (in-memory)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub job_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub initial_equity: f64,
    pub cash: f64,
    pub margin_used: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub total_equity: f64,
    pub total_notional: f64,
    pub leverage: f64,
    pub drawdown_pct: f64,
    pub peak_equity: f64,
}

impl AccountState {
    pub fn new(job_id: Uuid, initial_equity: f64, now: DateTime<Utc>) -> Self {
        Self {
            job_id,
            timestamp: now,
            initial_equity,
            cash: initial_equity,
            margin_used: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            total_equity: initial_equity,
            total_notional: 0.0,
            leverage: 0.0,
            drawdown_pct: 0.0,
            peak_equity: initial_equity,
        }
    }

    pub fn recompute_total_equity(&mut self) {
        // cash already includes realized P&L after fills/forced closes.
        // realized_pnl is kept for attribution and must not be added again.
        self.total_equity = self.cash + self.unrealized_pnl;
        self.leverage = if self.total_equity > 0.0 {
            self.total_notional / self.total_equity
        } else {
            0.0
        };
        if self.total_equity > self.peak_equity {
            self.peak_equity = self.total_equity;
        }
        if self.peak_equity > 0.0 {
            self.drawdown_pct = (self.peak_equity - self.total_equity) / self.peak_equity;
        }
    }
}

// ============================================================
// 6. Trade attribution (完整周期)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeAttribution {
    pub attribution_id: Uuid,
    pub job_id: Option<Uuid>,
    pub asset: String,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub direction: String,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub pnl: Option<f64>,
    pub pnl_bps: Option<f64>,
    pub fee_total: f64,
    pub holding_period_sec: Option<i64>,
    pub signal_confidence: Option<f64>,
    pub signal_strength: Option<f64>,
    pub entry_signal_id: Option<Uuid>,
    pub exit_reason: Option<String>,
    pub result: Option<String>,
    /// 入场时的市场状态（trending_bull/trending_bear/ranging/high_volatility/crisis）
    pub market_regime_at_entry: Option<String>,
}

// ============================================================
// 7. Market data (K-line)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Kline {
    pub symbol: String,
    pub interval: String,
    pub open_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub quote_volume: Option<f64>,
}

// ============================================================
// 8. Replay event (统一事件)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ReplayEvent {
    Kline { kline: Kline },
    Signal { signal: AlphaSignal },
}

impl ReplayEvent {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            ReplayEvent::Kline { kline } => kline.open_time,
            ReplayEvent::Signal { signal } => signal.event_time,
        }
    }
    pub fn asset(&self) -> String {
        match self {
            ReplayEvent::Kline { kline } => kline.symbol.clone(),
            ReplayEvent::Signal { signal } => signal.asset.clone(),
        }
    }
}

// ============================================================
// 9. Performance report
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceReport {
    pub total_return: f64,
    pub annualized_return: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: i64,
    pub winning_trades: i64,
    pub losing_trades: i64,
    pub average_win: f64,
    pub average_loss: f64,
    pub payoff_ratio: f64,
    pub total_fee: f64,
    pub total_slippage_cost: f64,
    pub var_95: f64,
    pub cvar_95: f64,
    pub alpha: f64,
    pub beta: f64,
    pub max_drawdown_duration_sec: i64,
    pub equity_curve: Vec<(DateTime<Utc>, f64)>,
    pub drawdown_curve: Vec<(DateTime<Utc>, f64)>,
    pub trades: Vec<TradeAttribution>,
    pub by_agent: Value,
    pub by_asset: Value,
    /// 按市场状态归因（trending_bull/trending_bear/ranging/high_volatility/crisis）
    pub by_regime: Value,
}

// ============================================================
// 10. Risk check result
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RiskCheckResult {
    pub passed: bool,
    pub reasons: Vec<String>,
    pub reduced_notional: Option<f64>,
}

impl RiskCheckResult {
    pub fn pass() -> Self {
        Self {
            passed: true,
            reasons: vec![],
            reduced_notional: None,
        }
    }
    pub fn reject(reason: impl Into<String>) -> Self {
        Self {
            passed: false,
            reasons: vec![reason.into()],
            reduced_notional: None,
        }
    }
}
