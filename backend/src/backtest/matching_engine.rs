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
    /// 返回 (成交价, 滑点基点, 滑点成本金额)
    pub fn apply_slippage(&self, side: &str, base_price: f64, quantity: f64) -> (f64, f64, f64) {
        let s = self.config.slippage_bps / 10000.0;
        let filled = if side.eq_ignore_ascii_case("buy") {
            base_price * (1.0 + s)
        } else {
            base_price * (1.0 - s)
        };
        let notional = filled * quantity;
        let slippage_cost = notional * self.config.slippage_bps / 10000.0;
        (filled, self.config.slippage_bps, slippage_cost)
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
        let (filled_price, slippage_bps, slippage_cost) =
            self.apply_slippage(&order.side, base_price, order.quantity);
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
            slippage_cost: Some(slippage_cost),
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
            slippage_cost: Some(0.0),
            maker_taker: "maker".into(),
            signal_id: order.source_signal_id,
            strategy_id: order.strategy_id.clone(),
            agent_id: order.agent_id.clone(),
            intent_type: None,
            fill_time: now,
        })
    }

    /// Apply a fill to the account + positions, producing an updated state.
    /// 资金守恒原则：
    ///   - 开仓：cash -= notional + fee（占用保证金 = notional）
    ///   - 平仓：cash += 保证金返还 + realized_pnl - fee（保证金按 avg_entry_price 计算，避免重复记账）
    ///   - 反手：先平仓现有仓位（含保证金返还），剩余数量按新方向开仓
    /// Returns (new/updated_position, realized_pnl_for_this_trade).
    pub fn apply_fill(
        &self,
        fill: &SimulatedFill,
        positions: &mut Vec<SimulatedPosition>,
        account: &mut AccountState,
    ) -> (Option<SimulatedPosition>, f64) {
        let asset = &fill.asset;
        let qty = fill.filled_quantity;
        let price = fill.filled_price;
        let fee = fill.fee;

        // Determine desired side: buy -> long, sell -> short
        let desired_side = if fill.side.eq_ignore_ascii_case("buy") { "long" } else { "short" };

        // 1) Look for an existing opposite position (close/reduce/reverse).
        if let Some(pos_idx) = positions
            .iter()
            .position(|p| p.asset == *asset && p.closed_at.is_none() && p.side != desired_side)
        {
            let opposite = &mut positions[pos_idx];
            let avg = opposite.avg_entry_price;
            let close_qty = qty.min(opposite.quantity); // 实际平仓数量（不能超过现有仓位）
            let remaining_qty = qty - close_qty; // 反手剩余数量（按新方向开仓）

            // 计算平仓部分的实现盈亏（不含费用，费用单独扣除）
            let pnl = if opposite.side == "long" {
                (price - avg) * close_qty
            } else {
                (avg - price) * close_qty
            };
            // 保证金返还：按 avg_entry_price 计算（这是开仓时占用的资金）
            let margin_return = avg * close_qty;
            // 平仓时返还保证金 + 盈亏 - 费用
            let realized_pnl = pnl - fee;
            account.realized_pnl += realized_pnl;
            account.cash += margin_return + pnl - fee;

            opposite.quantity -= close_qty;
            if opposite.quantity.abs() < 1e-9 {
                opposite.closed_at = Some(fill.fill_time);
                opposite.unrealized_pnl = 0.0;
            } else {
                // 部分平仓：更新 mark-to-market
                opposite.update_mark_to_market(price);
            }

            // 如果有剩余数量（反手），按新方向开仓
            if remaining_qty > 1e-9 {
                let new_notional = price * remaining_qty;
                let new_pos = SimulatedPosition {
                    position_id: Uuid::new_v4(),
                    job_id: Some(account.job_id),
                    asset: asset.clone(),
                    exchange: fill.exchange.clone(),
                    side: desired_side.into(),
                    quantity: remaining_qty,
                    avg_entry_price: price,
                    mark_price: Some(price),
                    notional: Some(new_notional),
                    unrealized_pnl: 0.0,
                    realized_pnl: 0.0,
                    margin: Some(new_notional / 1.0),
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
                account.cash -= new_notional; // 新仓位占用保证金
                positions.push(new_pos.clone());
                return (Some(new_pos), realized_pnl);
            }

            return (None, realized_pnl);
        }

        // 2) Same side position -> increase (加仓)
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
            let notional = price * qty;
            account.cash -= notional + fee;
            account.realized_pnl -= fee;
            return (Some(pos.clone()), -fee);
        }

        // 3) Open new position (开仓)
        let notional = price * qty;
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
        let (filled_price, slippage_bps, slippage_cost) =
            self.apply_slippage(fill_side, price, position.quantity);
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
            slippage_cost: Some(slippage_cost),
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
    use crate::backtest::models::{AccountState, SimulatedFill, SimulatedOrder};
    use chrono::Utc;
    use uuid::Uuid;

    fn make_engine() -> MatchingEngine {
        MatchingEngine::new(MatchingConfig::default())
    }

    fn make_account(initial: f64) -> AccountState {
        AccountState::new(Uuid::new_v4(), initial, Utc::now())
    }

    fn make_fill(asset: &str, side: &str, qty: f64, price: f64, fee: f64) -> SimulatedFill {
        SimulatedFill {
            fill_id: Uuid::new_v4(),
            order_id: Uuid::new_v4(),
            job_id: Some(Uuid::new_v4()),
            asset: asset.to_string(),
            exchange: Some("OKX".into()),
            side: side.to_string(),
            filled_quantity: qty,
            filled_price: price,
            notional: Some(price * qty),
            fee,
            slippage_bps: Some(0.0),
            slippage_cost: Some(0.0),
            maker_taker: "taker".into(),
            signal_id: None,
            strategy_id: None,
            agent_id: None,
            intent_type: None,
            fill_time: Utc::now(),
        }
    }

    #[test]
    fn test_slippage_buy_sell() {
        let eng = MatchingEngine::new(MatchingConfig::default());
        let (buy, _, buy_cost) = eng.apply_slippage("buy", 100.0, 1.0);
        let (sell, _, sell_cost) = eng.apply_slippage("sell", 100.0, 1.0);
        assert!(buy > 100.0);
        assert!(sell < 100.0);
        // 滑点成本金额应为正
        assert!(buy_cost > 0.0);
        assert!(sell_cost > 0.0);
    }

    /// 验证市价单成交保存了滑点成本金额
    #[test]
    fn test_market_fill_saves_slippage_cost() {
        let eng = make_engine();
        let kline = Kline {
            symbol: "BTC".into(),
            interval: "1m".into(),
            open_time: Utc::now(),
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 102.0,
            volume: 1000.0,
            quote_volume: Some(100000.0),
        };
        let order = SimulatedOrder {
            order_id: Uuid::new_v4(),
            job_id: Some(Uuid::new_v4()),
            intent_id: None,
            source_signal_id: None,
            strategy_id: None,
            agent_id: None,
            asset: "BTC".into(),
            exchange: Some("OKX".into()),
            side: "buy".into(),
            order_type: "market".into(),
            price: Some(100.0),
            quantity: 2.0,
            notional: None,
            filled_quantity: 0.0,
            filled_price: None,
            fee: 0.0,
            slippage_bps: None,
            leverage: 1,
            stop_loss: None,
            take_profit: None,
            status: "pending".into(),
            submitted_at: Utc::now(),
            filled_at: None,
        };

        let fill = eng.fill_market_order(&order, &kline, Utc::now());
        assert!(fill.is_some());
        let fill = fill.unwrap();
        // 滑点成本金额应被保存且大于 0
        assert!(fill.slippage_cost.is_some());
        let cost = fill.slippage_cost.unwrap();
        assert!(cost > 0.0, "滑点成本应大于 0，实际: {}", cost);
        // 验证成本计算正确: notional * slippage_bps / 10000
        let expected_cost = fill.notional.unwrap() * 3.0 / 10000.0;
        assert!(
            (cost - expected_cost).abs() < 1e-9,
            "滑点成本应为 {}，实际: {}",
            expected_cost,
            cost
        );
    }

    /// 资金守恒：开多仓后，cash 减少等于 notional + fee
    #[test]
    fn test_open_long_position_cash_conservation() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        let fill = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        let (pos, realized) = eng.apply_fill(&fill, &mut positions, &mut account);

        assert!(pos.is_some());
        assert_eq!(realized, -0.5); // 仅费用
        assert_eq!(account.cash, 10000.0 - 100.0 - 0.5); // notional + fee
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].side, "long");
        assert_eq!(positions[0].quantity, 1.0);
        assert_eq!(positions[0].avg_entry_price, 100.0);
    }

    /// 资金守恒：开空仓后，cash 减少等于 notional + fee
    #[test]
    fn test_open_short_position_cash_conservation() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        let fill = make_fill("BTC", "sell", 1.0, 100.0, 0.5);
        let (pos, _) = eng.apply_fill(&fill, &mut positions, &mut account);

        assert!(pos.is_some());
        assert_eq!(account.cash, 10000.0 - 100.0 - 0.5);
        assert_eq!(positions[0].side, "short");
        assert_eq!(positions[0].quantity, 1.0);
    }

    /// 加仓：同方向加仓后，avg_entry_price 正确加权平均
    #[test]
    fn test_increase_position_weighted_average() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 第一次开多 1.0 @ 100
        let fill1 = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill1, &mut positions, &mut account);

        // 加仓 1.0 @ 120
        let fill2 = make_fill("BTC", "buy", 1.0, 120.0, 0.6);
        eng.apply_fill(&fill2, &mut positions, &mut account);

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity, 2.0);
        // 加权平均 = (100*1 + 120*1) / 2 = 110
        assert!((positions[0].avg_entry_price - 110.0).abs() < 1e-9);
        // cash = 10000 - 100 - 0.5 - 120 - 0.6
        assert!((account.cash - (10000.0 - 100.0 - 0.5 - 120.0 - 0.6)).abs() < 1e-9);
    }

    /// 部分平仓：仓位减少，cash 返还保证金 + 盈亏 - 费用
    #[test]
    fn test_partial_close_position() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 2.0 @ 100
        let fill1 = make_fill("BTC", "buy", 2.0, 100.0, 1.0);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        // cash = 10000 - 200 - 1 = 9799

        // 部分平仓 1.0 @ 110（盈利 10）
        let fill2 = make_fill("BTC", "sell", 1.0, 110.0, 0.5);
        let (_, realized) = eng.apply_fill(&fill2, &mut positions, &mut account);

        assert!((realized - (10.0 - 0.5)).abs() < 1e-9); // pnl - fee = 9.5
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity, 1.0); // 剩余 1.0
        assert!(positions[0].closed_at.is_none()); // 未平仓
        // cash = 9799 + 保证金返还(100*1) + pnl(10) - fee(0.5) = 9908.5
        assert!((account.cash - (9799.0 + 100.0 + 10.0 - 0.5)).abs() < 1e-9);
    }

    /// 完全平仓：仓位关闭，cash 返还正确
    #[test]
    fn test_full_close_position() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 1.0 @ 100
        let fill1 = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        // cash = 10000 - 100 - 0.5 = 9899.5

        // 完全平仓 1.0 @ 90（亏损 10）
        let fill2 = make_fill("BTC", "sell", 1.0, 90.0, 0.5);
        let (_, realized) = eng.apply_fill(&fill2, &mut positions, &mut account);

        assert!((realized - (-10.0 - 0.5)).abs() < 1e-9); // pnl - fee = -10.5
        assert!(positions[0].closed_at.is_some()); // 已平仓
        assert_eq!(positions[0].quantity, 0.0);
        // cash = 9899.5 + 保证金返还(100*1) + pnl(-10) - fee(0.5) = 9989.0
        assert!((account.cash - (9899.5 + 100.0 - 10.0 - 0.5)).abs() < 1e-9);
    }

    /// 反手：多头仓位大于反向订单时，先平仓再开反向仓位
    #[test]
    fn test_reverse_position_opens_new_direction() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 1.0 @ 100
        let fill1 = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        // cash = 10000 - 100 - 0.5 = 9899.5

        // 反手卖出 3.0 @ 110：平多 1.0（盈利 10），开空 2.0
        let fill2 = make_fill("BTC", "sell", 3.0, 110.0, 1.5);
        let (new_pos, realized) = eng.apply_fill(&fill2, &mut positions, &mut account);

        // 平仓部分盈亏 = (110-100)*1 - 1.5 = 8.5
        assert!((realized - 8.5).abs() < 1e-9);
        // 应有 2 个仓位：原多头（已平仓）+ 新空头
        assert_eq!(positions.len(), 2);
        assert!(positions[0].closed_at.is_some()); // 原多头已平仓
        assert_eq!(positions[0].quantity, 0.0);
        // 新空头仓位
        assert!(new_pos.is_some());
        let new_pos = new_pos.unwrap();
        assert_eq!(new_pos.side, "short");
        assert!((new_pos.quantity - 2.0).abs() < 1e-9);
        assert!((new_pos.avg_entry_price - 110.0).abs() < 1e-9);
        // cash = 9899.5 + 保证金返还(100*1) + pnl(10) - fee(1.5) - 新仓位notional(110*2)
        //      = 9899.5 + 100 + 10 - 1.5 - 220 = 9788.0
        assert!((account.cash - 9788.0).abs() < 1e-9);
    }

    /// 资金守恒：开仓 + 平仓后，总权益 = 初始权益 + realized_pnl - fees
    #[test]
    fn test_capital_conservation_full_cycle() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 1.0 @ 100
        let fill1 = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill1, &mut positions, &mut account);

        // 平仓 1.0 @ 110
        let fill2 = make_fill("BTC", "sell", 1.0, 110.0, 0.5);
        eng.apply_fill(&fill2, &mut positions, &mut account);

        // 总权益 = cash + unrealized_pnl + realized_pnl
        // cash = 10000 - 100 - 0.5 + 100 + 10 - 0.5 = 10009
        // realized_pnl = -0.5 + 9.5 = 9.0
        // unrealized_pnl = 0（已平仓）
        // 总权益 = 10009 + 0 + 9 = 10018
        // 但 cash 已经包含了 realized_pnl 的返还，所以总权益 = cash = 10009
        // 注意：account.realized_pnl 是累计的，不应再加到 cash 上
        // 总权益 = cash = 10009（已包含所有盈亏和费用）
        assert!((account.cash - 10009.0).abs() < 1e-9);
        // 初始 10000，盈利 10，费用 1.0，净 = 10009
    }

    /// 限价单撮合：价格触及限价时成交
    #[test]
    fn test_limit_order_touched() {
        let eng = make_engine();
        let kline = Kline {
            symbol: "BTC".into(),
            interval: "1m".into(),
            open_time: Utc::now(),
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 102.0,
            volume: 1000.0,
            quote_volume: Some(100000.0),
        };
        let order = SimulatedOrder {
            order_id: Uuid::new_v4(),
            job_id: Some(Uuid::new_v4()),
            intent_id: None,
            source_signal_id: None,
            strategy_id: None,
            agent_id: None,
            asset: "BTC".into(),
            exchange: Some("OKX".into()),
            side: "buy".into(),
            order_type: "limit".into(),
            price: Some(98.0),
            quantity: 1.0,
            notional: None,
            filled_quantity: 0.0,
            filled_price: None,
            fee: 0.0,
            slippage_bps: None,
            leverage: 1,
            stop_loss: None,
            take_profit: None,
            status: "pending".into(),
            submitted_at: Utc::now(),
            filled_at: None,
        };

        let fill = eng.fill_limit_order(&order, &kline, Utc::now());
        assert!(fill.is_some());
        let fill = fill.unwrap();
        assert!((fill.filled_price - 98.0).abs() < 1e-9);
        assert_eq!(fill.maker_taker, "maker");
    }

    /// 限价单撮合：价格未触及限价时不成交
    #[test]
    fn test_limit_order_not_touched() {
        let eng = make_engine();
        let kline = Kline {
            symbol: "BTC".into(),
            interval: "1m".into(),
            open_time: Utc::now(),
            open: 100.0,
            high: 105.0,
            low: 95.0,
            close: 102.0,
            volume: 1000.0,
            quote_volume: Some(100000.0),
        };
        let order = SimulatedOrder {
            order_id: Uuid::new_v4(),
            job_id: Some(Uuid::new_v4()),
            intent_id: None,
            source_signal_id: None,
            strategy_id: None,
            agent_id: None,
            asset: "BTC".into(),
            exchange: Some("OKX".into()),
            side: "buy".into(),
            order_type: "limit".into(),
            price: Some(90.0), // 低于 low(95)，不会触及
            quantity: 1.0,
            notional: None,
            filled_quantity: 0.0,
            filled_price: None,
            fee: 0.0,
            slippage_bps: None,
            leverage: 1,
            stop_loss: None,
            take_profit: None,
            status: "pending".into(),
            submitted_at: Utc::now(),
            filled_at: None,
        };

        let fill = eng.fill_limit_order(&order, &kline, Utc::now());
        assert!(fill.is_none());
    }
}
