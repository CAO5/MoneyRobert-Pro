//! Matching engine: order matching, fee + slippage calculation
//! 订单撮合引擎

use crate::backtest::models::{AccountState, Kline, SimulatedFill, SimulatedOrder, SimulatedPosition};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MatchingConfig {
    pub fee_taker_bps: f64,
    pub fee_maker_bps: f64,
    pub slippage_bps: f64,
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self { fee_taker_bps: 5.0, fee_maker_bps: 2.0, slippage_bps: 3.0 }
    }
}

#[derive(Debug, Clone)]
pub struct MatchingEngine {
    pub config: MatchingConfig,
}

impl MatchingEngine {
    pub fn new(config: MatchingConfig) -> Self {
        Self { config }
    }

    /// Apply slippage to price based on order side.
    /// 买入: price * (1 + slippage_bps / 10000); 卖出: price * (1 - slippage_bps / 10000)
    pub fn apply_slippage(&self, side: &str, base_price: f64) -> (f64, f64) {
        let s = self.config.slippage_bps / 10000.0;
        let filled = if side.eq_ignore_ascii_case("buy") {
            base_price * (1.0 + s)
        } else {
            base_price * (1.0 - s)
        };
        (filled, self.config.slippage_bps)
    }

    /// Compute fee on notional (USDT).
    pub fn compute_fee(&self, notional: f64, maker: bool) -> f64 {
        let bps = if maker { self.config.fee_maker_bps } else { self.config.fee_taker_bps };
        notional * bps / 10000.0
    }

    /// Try to match a market order at next K-line open (simple model).
    /// 市价单: 以下一根 K线开盘价成交 (带滑点).
    pub fn fill_market_order(
        &self,
        order: &SimulatedOrder,
        kline: &Kline,
        now: DateTime<Utc>,
    ) -> Option<SimulatedFill> {
        if order.quantity <= 0.0 {
            return None;
        }
        let base_price = kline.open;
        if base_price <= 0.0 {
            return None;
        }
        let (filled_price, slippage_bps) = self.apply_slippage(&order.side, base_price);
        let notional = filled_price * order.quantity;
        let fee = self.compute_fee(notional, false);

        Some(SimulatedFill {
            fill_id: Uuid::new_v4(),
            order_id: order.order_id,
            job_id: order.job_id,
            asset: order.asset.clone(),
            exchange: order.exchange.clone(),
            side: order.side.clone(),
            filled_quantity: order.quantity,
            filled_price,
            notional: Some(notional),
            fee,
            slippage_bps: Some(slippage_bps),
            maker_taker: "taker".into(),
            signal_id: order.source_signal_id,
            strategy_id: order.strategy_id.clone(),
            agent_id: order.agent_id.clone(),
            intent_type: None,
            fill_time: now,
        })
    }

    /// Try to match a limit order against the K-line high/low range.
    /// 限价单: 若价格触及限价, 以限价成交 (maker).
    pub fn fill_limit_order(
        &self,
        order: &SimulatedOrder,
        kline: &Kline,
        now: DateTime<Utc>,
    ) -> Option<SimulatedFill> {
        let limit_price = order.price?;
        if order.quantity <= 0.0 {
            return None;
        }
        let touched = if order.side.eq_ignore_ascii_case("buy") {
            kline.low <= limit_price
        } else {
            kline.high >= limit_price
        };
        if !touched {
            return None;
        }

        let notional = limit_price * order.quantity;
        let fee = self.compute_fee(notional, true);
        Some(SimulatedFill {
            fill_id: Uuid::new_v4(),
            order_id: order.order_id,
            job_id: order.job_id,
            asset: order.asset.clone(),
            exchange: order.exchange.clone(),
            side: order.side.clone(),
            filled_quantity: order.quantity,
            filled_price: limit_price,
            notional: Some(notional),
            fee,
            slippage_bps: Some(0.0),
            maker_taker: "maker".into(),
            signal_id: order.source_signal_id,
            strategy_id: order.strategy_id.clone(),
            agent_id: order.agent_id.clone(),
            intent_type: None,
            fill_time: now,
        })
    }

    /// Apply a fill to the account + positions, producing an updated state.
    /// Returns (new/updated_position, realized_pnl_for_this_trade).
    pub fn apply_fill(
        &self,
        fill: &SimulatedFill,
        positions: &mut Vec<SimulatedPosition>,
        account: &mut AccountState,
    ) -> (Option<SimulatedPosition>, f64) {
        // Find existing opposite / same position for same asset
        let asset = &fill.asset;
        let qty = fill.filled_quantity;
        let price = fill.filled_price;
        let notional = price * qty;
        let fee = fill.fee;

        // Determine desired side: buy -> long, sell -> short
        let desired_side = if fill.side.eq_ignore_ascii_case("buy") { "long" } else { "short" };

        // 1) Look for an existing opposite position (close/reduce).
        let mut realized_pnl = 0.0_f64;
        if let Some(pos_idx) = positions
            .iter()
            .position(|p| p.asset == *asset && p.closed_at.is_none() && p.side != desired_side)
        {
            let opposite = &mut positions[pos_idx];
            if opposite.quantity >= qty {
                // Fully reduce or close
                let avg = opposite.avg_entry_price;
                let pnl = if opposite.side == "long" {
                    (price - avg) * qty
                } else {
                    (avg - price) * qty
                };
                realized_pnl = pnl - fee;
                opposite.quantity -= qty;
                if opposite.quantity.abs() < 1e-9 {
                    opposite.closed_at = Some(fill.fill_time);
                    opposite.unrealized_pnl = 0.0;
                } else {
                    // Remaining position keeps avg
                }
                account.realized_pnl += realized_pnl;
                account.cash += notional + realized_pnl; // realized P/L added to cash
                return (None, realized_pnl);
            } else {
                // Partially cover opposite, reduce position, keep avg_entry_price
                let avg = opposite.avg_entry_price;
                let pnl = if opposite.side == "long" {
                    (price - avg) * qty
                } else {
                    (avg - price) * qty
                };
                realized_pnl = pnl;
                account.realized_pnl += pnl;
                account.cash += avg * qty + pnl; // cash returned from closed portion
                opposite.quantity -= qty;
                opposite.unrealized_pnl = 0.0;
                return (None, realized_pnl);
            }
        }

        // 2) Same side position -> increase
        if let Some(pos_idx) = positions
            .iter()
            .position(|p| p.asset == *asset && p.closed_at.is_none() && p.side == desired_side)
        {
            let pos = &mut positions[pos_idx];
            let new_qty = pos.quantity + qty;
            let new_avg = if new_qty > 0.0 {
                (pos.avg_entry_price * pos.quantity + price * qty) / new_qty
            } else {
                price
            };
            pos.quantity = new_qty;
            pos.avg_entry_price = new_avg;
            pos.update_mark_to_market(price);
            account.cash -= notional + fee;
            account.realized_pnl -= fee;
            return (Some(pos.clone()), -fee);
        }

        // 3) Open new position
        let new_pos = SimulatedPosition {
            position_id: Uuid::new_v4(),
            job_id: Some(account.job_id),
            asset: asset.clone(),
            exchange: fill.exchange.clone(),
            side: desired_side.into(),
            quantity: qty,
            avg_entry_price: price,
            mark_price: Some(price),
            notional: Some(notional),
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            margin: Some(notional / 1.0),
            leverage: 1,
            liquidation_price: None,
            stop_loss_price: None,
            take_profit_price: None,
            open_signal_id: fill.signal_id,
            strategy_id: fill.strategy_id.clone(),
            agent_id: fill.agent_id.clone(),
            opened_at: fill.fill_time,
            closed_at: None,
        };
        account.cash -= notional + fee;
        account.realized_pnl -= fee;
        positions.push(new_pos.clone());
        (Some(new_pos), -fee)
    }

    /// Close an existing position at given price (stop-loss / take-profit / manual).
    pub fn close_position_at_price(
        &self,
        position: &SimulatedPosition,
        price: f64,
        now: DateTime<Utc>,
    ) -> SimulatedFill {
        let (side, _avg) = (position.side.clone(), position.avg_entry_price);
        let fill_side = if side == "long" { "sell" } else { "buy" };
        let (filled_price, slippage_bps) = self.apply_slippage(fill_side, price);
        let notional = filled_price * position.quantity;
        let fee = self.compute_fee(notional, false);

        SimulatedFill {
            fill_id: Uuid::new_v4(),
            order_id: Uuid::new_v4(),
            job_id: position.job_id,
            asset: position.asset.clone(),
            exchange: position.exchange.clone(),
            side: fill_side.into(),
            filled_quantity: position.quantity,
            filled_price,
            notional: Some(notional),
            fee,
            slippage_bps: Some(slippage_bps),
            maker_taker: "taker".into(),
            signal_id: position.open_signal_id,
            strategy_id: position.strategy_id.clone(),
            agent_id: position.agent_id.clone(),
            intent_type: None,
            fill_time: now,
        }
    }
}

/// Validate market prices are reasonable (avoid NaN/zero).
pub fn is_valid_price(p: f64) -> bool {
    p.is_finite() && p > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slippage_buy_sell() {
        let eng = MatchingEngine::new(MatchingConfig::default());
        let (buy, _) = eng.apply_slippage("buy", 100.0);
        let (sell, _) = eng.apply_slippage("sell", 100.0);
        assert!(buy > 100.0);
        assert!(sell < 100.0);
    }
}
