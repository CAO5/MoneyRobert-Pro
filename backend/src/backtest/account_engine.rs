//! Account engine: maintains account state, position P&L, equity snapshots.
//! 账户引擎

use crate::backtest::models::{AccountState, Kline, SimulatedPosition};
use chrono::{DateTime, Utc};

pub struct AccountEngine {
    pub state: AccountState,
    pub positions: Vec<SimulatedPosition>,
    pub snapshots: Vec<(DateTime<Utc>, f64)>,
}

impl AccountEngine {
    pub fn new(job_id: uuid::Uuid, initial_equity: f64, now: DateTime<Utc>) -> Self {
        Self {
            state: AccountState::new(job_id, initial_equity, now),
            positions: Vec::new(),
            snapshots: vec![(now, initial_equity)],
        }
    }

    /// Mark all open positions to market using latest prices map (symbol -> price).
    pub fn mark_to_market(&mut self, prices: &std::collections::HashMap<String, f64>, now: DateTime<Utc>) {
        let mut total_unreal = 0.0_f64;
        let mut total_notional = 0.0_f64;
        for pos in self.positions.iter_mut().filter(|p| p.closed_at.is_none()) {
            if let Some(&price) = prices.get(&pos.asset) {
                pos.update_mark_to_market(price);
                total_unreal += pos.unrealized_pnl;
                total_notional += pos.notional.unwrap_or(0.0);
            }
        }
        self.state.unrealized_pnl = total_unreal;
        self.state.total_notional = total_notional;
        self.state.timestamp = now;
        self.state.recompute_total_equity();
    }

    pub fn record_snapshot(&mut self) {
        self.snapshots.push((self.state.timestamp, self.state.total_equity));
    }

    pub fn current_equity(&self) -> f64 {
        self.state.total_equity
    }

    pub fn open_position_notional_for_asset(&self, asset: &str) -> f64 {
        self.positions
            .iter()
            .filter(|p| p.asset == asset && p.closed_at.is_none())
            .map(|p| p.notional.unwrap_or(0.0))
            .sum()
    }

    /// Collect open positions that need to be force-closed at end of backtest.
    /// Returns (position_id, asset, price) triples; the runner is responsible
    /// for routing them through the matching engine so that fees, slippage and
    /// margin accounting stay consistent with live fills (P0-3).
    pub fn open_positions_for_force_close(
        &self,
        prices: &std::collections::HashMap<String, f64>,
    ) -> Vec<(uuid::Uuid, String, f64)> {
        self.positions
            .iter()
            .filter(|p| p.closed_at.is_none())
            .filter_map(|p| prices.get(&p.asset).map(|&pr| (p.position_id, p.asset.clone(), pr)))
            .collect()
    }

    /// Check if any open position triggered stop-loss / take-profit;
    /// returns list of assets that should be closed.
    pub fn check_stops(&self, prices: &std::collections::HashMap<String, f64>) -> Vec<(uuid::Uuid, String, f64)> {
        let mut triggers = Vec::new();
        for pos in self.positions.iter().filter(|p| p.closed_at.is_none()) {
            if let Some(&price) = prices.get(&pos.asset) {
                if pos.is_stop_loss_triggered(price) {
                    triggers.push((pos.position_id, pos.asset.clone(), price));
                } else if pos.is_take_profit_triggered(price) {
                    triggers.push((pos.position_id, pos.asset.clone(), price));
                }
            }
        }
        triggers
    }
}

/// Build a { symbol -> price } map from Kline vector (uses close price).
pub fn kline_prices(klines: &[Kline]) -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
    for k in klines {
        map.insert(k.symbol.clone(), k.close);
    }
    map
}
