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
    ///
    /// 合约/永续保证金模型（依据系统 2.0 待补全问题清单 P0）：
    ///   - 开仓：cash -= fee; margin_used += notional / leverage; total_notional += notional
    ///   - 平仓：cash += net_pnl; margin_used -= released_margin; position.realized_pnl += net_pnl
    ///   - 加仓：cash -= fee; margin_used += added_notional / leverage
    ///   - 反手：先平仓（结算 PnL），再开新仓（只扣 fee，加 margin_used）
    ///
    /// 资金守恒：total_equity = cash + unrealized_pnl
    ///   cash 只反映余额、手续费、已实现盈亏，不扣 notional
    ///   margin_used 独立记录保证金占用
    ///   realized_pnl 是统计归因字段，不直接进入权益
    ///
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
            let pos_leverage = opposite.leverage.max(1) as f64;

            // 计算平仓部分的毛盈亏
            let gross_pnl = if opposite.side == "long" {
                (price - avg) * close_qty
            } else {
                (avg - price) * close_qty
            };
            // 平仓释放的保证金（按开仓时占用的保证金比例释放）
            let released_margin = if opposite.quantity > 0.0 {
                opposite.margin.unwrap_or(0.0) * (close_qty / opposite.quantity)
            } else {
                0.0
            };
            // 反手时手续费按数量拆分
            let close_fee = if remaining_qty > 1e-9 {
                fee * close_qty / qty
            } else {
                fee
            };
            let net_pnl = gross_pnl - close_fee;

            // 合约口径：cash 只加净盈亏，不返还保证金（保证金从未从 cash 扣除）
            account.cash += net_pnl;
            account.realized_pnl += net_pnl;
            account.margin_used -= released_margin;
            // total_notional 由 mark_to_market 重算，此处不维护
            // 更新 position 级别归因（P0-2 修复：确保 position.realized_pnl 被更新）
            opposite.realized_pnl += net_pnl;

            opposite.quantity -= close_qty;
            if opposite.quantity.abs() < 1e-9 {
                opposite.closed_at = Some(fill.fill_time);
                opposite.unrealized_pnl = 0.0;
                opposite.margin = Some(0.0);
            } else {
                // 部分平仓：更新 mark-to-market 和剩余保证金
                opposite.update_mark_to_market(price);
                opposite.margin = Some(opposite.margin.unwrap_or(0.0) - released_margin);
            }

            // 如果有剩余数量（反手），按新方向开仓
            if remaining_qty > 1e-9 {
                let open_fee = fee - close_fee;
                let new_notional = price * remaining_qty;
                let new_margin = new_notional / pos_leverage;
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
                    margin: Some(new_margin),
                    leverage: pos_leverage as i32,
                    liquidation_price: None,
                    stop_loss_price: None,
                    take_profit_price: None,
                    open_signal_id: fill.signal_id,
                    strategy_id: fill.strategy_id.clone(),
                    agent_id: fill.agent_id.clone(),
                    opened_at: fill.fill_time,
                    closed_at: None,
                };
                // 合约口径：cash 只扣手续费，margin_used 记录保证金占用
                account.cash -= open_fee;
                account.realized_pnl -= open_fee;
                account.margin_used += new_margin;
                account.total_notional += new_notional;
                positions.push(new_pos.clone());
                return (Some(new_pos), net_pnl);
            }

            return (None, net_pnl);
        }

        // 2) Same side position -> increase (加仓)
        if let Some(pos_idx) = positions
            .iter()
            .position(|p| p.asset == *asset && p.closed_at.is_none() && p.side == desired_side)
        {
            let pos = &mut positions[pos_idx];
            let pos_leverage = pos.leverage.max(1) as f64;
            let old_qty = pos.quantity;
            let new_qty = old_qty + qty;
            let new_avg = if new_qty > 0.0 {
                (pos.avg_entry_price * old_qty + price * qty) / new_qty
            } else {
                price
            };
            let added_notional = price * qty;
            let added_margin = added_notional / pos_leverage;

            pos.quantity = new_qty;
            pos.avg_entry_price = new_avg;
            pos.update_mark_to_market(price);
            pos.margin = Some(pos.margin.unwrap_or(0.0) + added_margin);
            pos.notional = Some(pos.notional.unwrap_or(0.0) + added_notional);

            // 合约口径：cash 只扣手续费，margin_used 增加保证金
            account.cash -= fee;
            account.realized_pnl -= fee;
            account.margin_used += added_margin;
            account.total_notional += added_notional;
            return (Some(pos.clone()), -fee);
        }

        // 3) Open new position (开仓)
        let notional = price * qty;
        let leverage = 1; // 默认杠杆 1x（全仓）
        let margin = notional / leverage as f64;
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
            margin: Some(margin),
            leverage,
            liquidation_price: None,
            stop_loss_price: None,
            take_profit_price: None,
            open_signal_id: fill.signal_id,
            strategy_id: fill.strategy_id.clone(),
            agent_id: fill.agent_id.clone(),
            opened_at: fill.fill_time,
            closed_at: None,
        };
        // 合约口径：cash 只扣手续费，margin_used 记录保证金占用
        account.cash -= fee;
        account.realized_pnl -= fee;
        account.margin_used += margin;
        account.total_notional += notional;
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

    /// 合约口径：开多仓后，cash 只扣 fee，margin_used = notional / leverage
    #[test]
    fn test_open_long_position_cash_conservation() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        let fill = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        let (pos, realized) = eng.apply_fill(&fill, &mut positions, &mut account);

        assert!(pos.is_some());
        assert_eq!(realized, -0.5); // 仅费用
        // 合约口径：cash 只扣 fee，不扣 notional
        assert_eq!(account.cash, 10000.0 - 0.5);
        // margin_used = notional / leverage = 100 / 1 = 100
        assert_eq!(account.margin_used, 100.0);
        assert_eq!(account.total_notional, 100.0);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].side, "long");
        assert_eq!(positions[0].quantity, 1.0);
        assert_eq!(positions[0].avg_entry_price, 100.0);
    }

    /// 合约口径：开空仓后，cash 只扣 fee
    #[test]
    fn test_open_short_position_cash_conservation() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        let fill = make_fill("BTC", "sell", 1.0, 100.0, 0.5);
        let (pos, _) = eng.apply_fill(&fill, &mut positions, &mut account);

        assert!(pos.is_some());
        // 合约口径：cash 只扣 fee
        assert_eq!(account.cash, 10000.0 - 0.5);
        assert_eq!(account.margin_used, 100.0);
        assert_eq!(positions[0].side, "short");
        assert_eq!(positions[0].quantity, 1.0);
    }

    /// 加仓：同方向加仓后，avg_entry_price 正确加权平均，cash 只扣 fee
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
        // 合约口径：cash 只扣两次 fee
        assert!((account.cash - (10000.0 - 0.5 - 0.6)).abs() < 1e-9);
        // margin_used = 100 + 120 = 220
        assert!((account.margin_used - 220.0).abs() < 1e-9);
    }

    /// 部分平仓：仓位减少，cash 只加净盈亏
    #[test]
    fn test_partial_close_position() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 2.0 @ 100
        let fill1 = make_fill("BTC", "buy", 2.0, 100.0, 1.0);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        // cash = 10000 - 1 = 9999, margin_used = 200

        // 部分平仓 1.0 @ 110（盈利 10）
        let fill2 = make_fill("BTC", "sell", 1.0, 110.0, 0.5);
        let (_, realized) = eng.apply_fill(&fill2, &mut positions, &mut account);

        assert!((realized - (10.0 - 0.5)).abs() < 1e-9); // pnl - fee = 9.5
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity, 1.0); // 剩余 1.0
        assert!(positions[0].closed_at.is_none()); // 未平仓
        // 合约口径：cash = 9999 + net_pnl(9.5) = 10008.5
        assert!((account.cash - (9999.0 + 9.5)).abs() < 1e-9);
        // margin_used = 200 - 100 = 100（释放一半保证金）
        assert!((account.margin_used - 100.0).abs() < 1e-9);
        // position.realized_pnl 应被更新（P0-2 修复）
        assert!((positions[0].realized_pnl - 9.5).abs() < 1e-9);
    }

    /// 完全平仓：仓位关闭，cash 只加净盈亏
    #[test]
    fn test_full_close_position() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 1.0 @ 100
        let fill1 = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        // cash = 10000 - 0.5 = 9999.5, margin_used = 100

        // 完全平仓 1.0 @ 90（亏损 10）
        let fill2 = make_fill("BTC", "sell", 1.0, 90.0, 0.5);
        let (_, realized) = eng.apply_fill(&fill2, &mut positions, &mut account);

        assert!((realized - (-10.0 - 0.5)).abs() < 1e-9); // pnl - fee = -10.5
        assert!(positions[0].closed_at.is_some()); // 已平仓
        assert_eq!(positions[0].quantity, 0.0);
        // 合约口径：cash = 9999.5 + net_pnl(-10.5) = 9989.0
        assert!((account.cash - (9999.5 - 10.5)).abs() < 1e-9);
        // margin_used = 0（全部释放）
        assert!((account.margin_used - 0.0).abs() < 1e-9);
        // position.realized_pnl 应被更新（P0-2 修复）
        assert!((positions[0].realized_pnl - (-10.5)).abs() < 1e-9);
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
        // cash = 10000 - 0.5 = 9999.5, margin_used = 100

        // 反手卖出 3.0 @ 110：平多 1.0（盈利 10），开空 2.0
        // 手续费按数量拆分：close_fee = 1.5 * 1/3 = 0.5, open_fee = 1.0
        let fill2 = make_fill("BTC", "sell", 3.0, 110.0, 1.5);
        let (new_pos, realized) = eng.apply_fill(&fill2, &mut positions, &mut account);

        // 平仓部分净盈亏 = gross_pnl(10) - close_fee(0.5) = 9.5
        assert!((realized - 9.5).abs() < 1e-9);
        // 应有 2 个仓位：原多头（已平仓）+ 新空头
        assert_eq!(positions.len(), 2);
        assert!(positions[0].closed_at.is_some()); // 原多头已平仓
        assert_eq!(positions[0].quantity, 0.0);
        // 原多头 realized_pnl 应被更新（P0-2 修复）
        assert!((positions[0].realized_pnl - 9.5).abs() < 1e-9);
        // 新空头仓位
        assert!(new_pos.is_some());
        let new_pos = new_pos.unwrap();
        assert_eq!(new_pos.side, "short");
        assert!((new_pos.quantity - 2.0).abs() < 1e-9);
        assert!((new_pos.avg_entry_price - 110.0).abs() < 1e-9);
        // 合约口径：cash = 9999.5 + net_pnl(9.5) - open_fee(1.0) = 10008.0
        assert!((account.cash - 10008.0).abs() < 1e-9);
        // margin_used = 100(释放) - 0 + 220(新仓) = 220
        assert!((account.margin_used - 220.0).abs() < 1e-9);
    }

    /// 资金守恒：开仓 + 平仓后，total_equity = 初始权益 + 净盈亏
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

        // 合约口径资金守恒：
        // cash = 10000 - 0.5(开仓fee) + 9.5(平仓net_pnl) = 10009.0
        // unrealized_pnl = 0（已平仓）
        // total_equity = cash + unrealized_pnl = 10009.0
        assert!((account.cash - 10009.0).abs() < 1e-9);
        // 初始 10000，盈利 10，费用 1.0，净 = 10009
        account.recompute_total_equity();
        assert!((account.total_equity - 10009.0).abs() < 1e-9);
    }

    /// P0-4 回归：多资产资金守恒 — cash + unrealized_pnl = total_equity
    #[test]
    fn test_capital_conservation_multi_asset() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 BTC 1.0 @ 100 (fee 0.5)
        let fill_btc = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill_btc, &mut positions, &mut account);
        // 开空 ETH 2.0 @ 50 (fee 0.5)
        let fill_eth = make_fill("ETH", "sell", 2.0, 50.0, 0.5);
        eng.apply_fill(&fill_eth, &mut positions, &mut account);

        // cash = 10000 - 0.5 - 0.5 = 9999.0
        assert!((account.cash - 9999.0).abs() < 1e-9);
        // margin_used = 100 + 100 = 200
        assert!((account.margin_used - 200.0).abs() < 1e-9);
        assert_eq!(positions.len(), 2);

        // 模拟 mark-to-market：BTC 涨到 110，ETH 跌到 45（空仓盈利）
        for pos in positions.iter_mut() {
            let price = if pos.asset == "BTC" { 110.0 } else { 45.0 };
            pos.update_mark_to_market(price);
        }
        // BTC unrealized = (110-100)*1 = 10; ETH unrealized = (50-45)*2 = 10
        account.unrealized_pnl = positions.iter().map(|p| p.unrealized_pnl).sum();
        account.recompute_total_equity();
        // total_equity = cash(9999) + unrealized(20) = 10019
        assert!((account.total_equity - 10019.0).abs() < 1e-9);
    }

    /// P0-4 回归：通过 close_position_at_price 平仓后归因正确
    /// 验证 P0-2 修复：position.realized_pnl 在平仓后被正确更新
    #[test]
    fn test_force_close_attribution_via_matching() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 1.0 @ 100 (fee 0.5)
        let fill_open = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill_open, &mut positions, &mut account);
        let pos_id = positions[0].position_id;

        // 通过 close_position_at_price 平仓 @ 110（模拟 P0-3 强平链路）
        let close_fill = eng.close_position_at_price(&positions[0], 110.0, Utc::now());
        // close_position_at_price 使用 taker fee + slippage，验证 fill 字段
        assert_eq!(close_fill.side, "sell");
        assert_eq!(close_fill.filled_quantity, 1.0);
        assert!(close_fill.fee > 0.0);

        let (_, realized) = eng.apply_fill(&close_fill, &mut positions, &mut account);

        // 归因验证：position.realized_pnl 应等于 net_pnl（P0-2）
        let closed_pos = positions.iter().find(|p| p.position_id == pos_id).unwrap();
        assert!(closed_pos.closed_at.is_some());
        assert!((closed_pos.realized_pnl - realized).abs() < 1e-9,
            "position.realized_pnl 应等于 apply_fill 返回的 realized_pnl");
        // realized 应为正（价格上涨盈利）
        assert!(realized > 0.0, "盈利平仓 realized 应为正，实际: {}", realized);
    }

    /// P0-4 回归：保证金守恒 — 全平后 margin_used 归零
    #[test]
    fn test_margin_conservation_full_cycle() {
        let eng = make_engine();
        let mut account = make_account(10000.0);
        let mut positions = Vec::new();

        // 开多 2.0 @ 100 (fee 1.0, margin 200)
        let fill1 = make_fill("BTC", "buy", 2.0, 100.0, 1.0);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        assert!((account.margin_used - 200.0).abs() < 1e-9);

        // 部分平仓 1.0 @ 105
        let fill2 = make_fill("BTC", "sell", 1.0, 105.0, 0.5);
        eng.apply_fill(&fill2, &mut positions, &mut account);
        // 释放一半保证金
        assert!((account.margin_used - 100.0).abs() < 1e-9);

        // 全平剩余 1.0 @ 108
        let fill3 = make_fill("BTC", "sell", 1.0, 108.0, 0.5);
        eng.apply_fill(&fill3, &mut positions, &mut account);
        // 保证金全部释放
        assert!((account.margin_used - 0.0).abs() < 1e-9,
            "全平后 margin_used 应归零，实际: {}", account.margin_used);
        // total_notional 由 mark_to_market 重算，此处不断言
    }

    /// P0-4 回归：亏损平仓后资金守恒 — cash 不为负
    #[test]
    fn test_capital_conservation_losing_trade() {
        let eng = make_engine();
        let mut account = make_account(1000.0);
        let mut positions = Vec::new();

        // 开多 1.0 @ 100 (fee 0.5)
        let fill1 = make_fill("BTC", "buy", 1.0, 100.0, 0.5);
        eng.apply_fill(&fill1, &mut positions, &mut account);
        // cash = 1000 - 0.5 = 999.5

        // 平仓 1.0 @ 90（亏损 10）
        let fill2 = make_fill("BTC", "sell", 1.0, 90.0, 0.5);
        eng.apply_fill(&fill2, &mut positions, &mut account);
        // cash = 999.5 + (-10 - 0.5) = 989.0
        assert!((account.cash - 989.0).abs() < 1e-9);
        assert!((account.margin_used - 0.0).abs() < 1e-9);

        // 资金守恒：total_equity = cash（已无未实现盈亏）
        account.recompute_total_equity();
        assert!((account.total_equity - 989.0).abs() < 1e-9);
        // 初始 1000，亏损 10，费用 1.0，净 = 989
        assert!(account.cash > 0.0, "亏损后 cash 仍应为正");
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
