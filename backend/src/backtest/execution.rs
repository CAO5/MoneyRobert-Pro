//! Unified Execution Engine
//! 统一执行引擎
//!
//! 依据《系统评估与演进规划》第三阶段任务 3：
//! 统一模拟、回测和实盘执行模型
//!
//! 设计目标：
//! - 回测、模拟盘、实盘共用同一套执行语义
//! - 统一的订单、成交、持仓、账户模型
//! - 统一的费用、滑点、保证金计算
//! - 通过 trait 抽象不同执行后端

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================
// 1. 统一数据模型
// ============================================================

/// 订单方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        }
    }
}

/// 订单类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Market,
    Limit,
    Stop,
}

/// 订单状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,
    Submitted,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    Long,
    Short,
}

impl PositionSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            PositionSide::Long => "long",
            PositionSide::Short => "short",
        }
    }

    pub fn from_order_side(side: OrderSide) -> Self {
        match side {
            OrderSide::Buy => PositionSide::Long,
            OrderSide::Sell => PositionSide::Short,
        }
    }
}

/// 统一订单模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub order_id: Uuid,
    pub symbol: String,
    pub exchange: Option<String>,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub leverage: i32,
    pub status: OrderStatus,
    pub client_order_id: Option<String>,
    pub strategy_id: Option<String>,
    pub agent_id: Option<String>,
    pub signal_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 统一成交模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub fill_id: Uuid,
    pub order_id: Uuid,
    pub symbol: String,
    pub exchange: Option<String>,
    pub side: OrderSide,
    pub quantity: f64,
    pub price: f64,
    pub notional: f64,
    pub fee: f64,
    pub slippage_bps: f64,
    pub slippage_cost: f64,
    pub fee_rate_bps: f64,
    pub is_maker: bool,
    pub fill_time: DateTime<Utc>,
}

/// 统一持仓模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub exchange: Option<String>,
    pub side: PositionSide,
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
    pub opened_at: DateTime<Utc>,
}

impl Position {
    /// 按市价更新持仓
    pub fn update_mark_price(&mut self, current_price: f64) {
        self.mark_price = Some(current_price);
        self.notional = Some(self.quantity * current_price);
        let signed_pnl = match self.side {
            PositionSide::Long => (current_price - self.avg_entry_price) * self.quantity,
            PositionSide::Short => (self.avg_entry_price - current_price) * self.quantity,
        };
        self.unrealized_pnl = signed_pnl;
    }

    /// 是否触发止损
    pub fn is_stop_loss_triggered(&self, price: f64) -> bool {
        if let Some(sl) = self.stop_loss_price {
            match self.side {
                PositionSide::Long => price <= sl,
                PositionSide::Short => price >= sl,
            }
        } else {
            false
        }
    }

    /// 是否触发止盈
    pub fn is_take_profit_triggered(&self, price: f64) -> bool {
        if let Some(tp) = self.take_profit_price {
            match self.side {
                PositionSide::Long => price >= tp,
                PositionSide::Short => price <= tp,
            }
        } else {
            false
        }
    }
}

/// 统一账户状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub account_id: String,
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
    pub timestamp: DateTime<Utc>,
}

impl Account {
    pub fn new(account_id: &str, initial_equity: f64) -> Self {
        let now = Utc::now();
        Self {
            account_id: account_id.to_string(),
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
            timestamp: now,
        }
    }

    /// 重新计算权益和杠杆
    pub fn recompute_equity(&mut self) {
        // 总权益 = 可用现金 + 已用保证金 + 未实现盈亏
        // cash 中已扣除了保证金，所以需要加回 margin_used
        self.total_equity = self.cash + self.margin_used + self.unrealized_pnl;
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
        self.timestamp = Utc::now();
    }
}

// ============================================================
// 2. 执行配置
// ============================================================

/// 执行配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Taker 手续费率（基点）
    pub fee_taker_bps: f64,
    /// Maker 手续费率（基点）
    pub fee_maker_bps: f64,
    /// 默认滑点（基点）
    pub slippage_bps: f64,
    /// 默认杠杆
    pub default_leverage: i32,
    /// 最大单资产仓位占比
    pub max_position_pct: f64,
    /// 最大总杠杆
    pub max_leverage: f64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            fee_taker_bps: 5.0,
            fee_maker_bps: 2.0,
            slippage_bps: 3.0,
            default_leverage: 1,
            max_position_pct: 0.10,
            max_leverage: 3.0,
        }
    }
}

// ============================================================
// 3. 执行引擎 trait
// ============================================================

/// 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub order: Order,
    pub fills: Vec<Fill>,
    pub success: bool,
    pub error: Option<String>,
}

/// 统一执行引擎接口
///
/// 回测、模拟盘、实盘均实现此 trait，保证执行语义一致
#[async_trait::async_trait]
pub trait ExecutionEngine: Send + Sync {
    /// 提交订单
    async fn submit_order(&self, order: Order) -> ExecutionResult;

    /// 取消订单
    async fn cancel_order(&self, order_id: Uuid) -> bool;

    /// 获取持仓
    async fn get_position(&self, symbol: &str) -> Option<Position>;

    /// 获取所有持仓
    async fn get_all_positions(&self) -> Vec<Position>;

    /// 获取账户状态
    async fn get_account(&self) -> Account;

    /// 更新市场价格（用于模拟盘/回测的市价更新）
    async fn update_price(&self, symbol: &str, price: f64, timestamp: DateTime<Utc>);

    /// 获取配置
    fn config(&self) -> &ExecutionConfig;
}

// ============================================================
// 4. 费用与滑点计算工具
// ============================================================

/// 计算手续费
pub fn compute_fee(notional: f64, fee_rate_bps: f64) -> f64 {
    notional * fee_rate_bps / 10000.0
}

/// 计算滑点价格（买入向上滑，卖出向下滑）
pub fn apply_slippage(side: OrderSide, price: f64, slippage_bps: f64) -> (f64, f64) {
    let s = slippage_bps / 10000.0;
    let filled = match side {
        OrderSide::Buy => price * (1.0 + s),
        OrderSide::Sell => price * (1.0 - s),
    };
    let slippage_cost = (filled - price).abs() * (filled / price); // 近似
    (filled, slippage_cost)
}

/// 计算保证金需求
pub fn compute_margin_required(notional: f64, leverage: i32) -> f64 {
    if leverage <= 0 {
        notional
    } else {
        notional / leverage as f64
    }
}

// ============================================================
// 5. 内存执行器（回测/模拟盘基础实现）
// ============================================================

/// 内存执行器（用于回测和模拟盘）
///
/// 在内存中维护账户和持仓状态，基于传入的市场价格撮合订单
pub struct InMemoryExecutor {
    config: ExecutionConfig,
    account: std::sync::RwLock<Account>,
    positions: std::sync::RwLock<HashMap<String, Position>>,
    orders: std::sync::RwLock<Vec<Order>>,
    fills: std::sync::RwLock<Vec<Fill>>,
    current_prices: std::sync::RwLock<HashMap<String, f64>>,
}

impl InMemoryExecutor {
    pub fn new(config: ExecutionConfig, initial_equity: f64) -> Self {
        let account = Account::new("in_memory", initial_equity);
        Self {
            config,
            account: std::sync::RwLock::new(account),
            positions: std::sync::RwLock::new(HashMap::new()),
            orders: std::sync::RwLock::new(Vec::new()),
            fills: std::sync::RwLock::new(Vec::new()),
            current_prices: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 市价单即时成交（使用当前价格）
    fn execute_market_order(&self, order: &Order, price: f64) -> ExecutionResult {
        let now = Utc::now();
        let (filled_price, _) = apply_slippage(order.side, price, self.config.slippage_bps);
        let notional = filled_price * order.quantity;
        let fee = compute_fee(notional, self.config.fee_taker_bps);
        let slippage_cost = (filled_price - price).abs() * order.quantity;

        let fill = Fill {
            fill_id: Uuid::new_v4(),
            order_id: order.order_id,
            symbol: order.symbol.clone(),
            exchange: order.exchange.clone(),
            side: order.side,
            quantity: order.quantity,
            price: filled_price,
            notional,
            fee,
            slippage_bps: self.config.slippage_bps,
            slippage_cost,
            fee_rate_bps: self.config.fee_taker_bps,
            is_maker: false,
            fill_time: now,
        };

        let mut filled_order = order.clone();
        filled_order.filled_quantity = order.quantity;
        filled_order.status = OrderStatus::Filled;
        filled_order.updated_at = now;

        // 应用成交到账户和持仓
        self.apply_fill(&fill, &filled_order);

        ExecutionResult {
            order: filled_order,
            fills: vec![fill],
            success: true,
            error: None,
        }
    }

    /// 应用成交到账户和持仓
    fn apply_fill(&self, fill: &Fill, order: &Order) {
        let mut account = self.account.write().unwrap();
        let mut positions = self.positions.write().unwrap();

        let symbol = &fill.symbol;
        let side = fill.side;

        // 更新订单历史
        let mut orders = self.orders.write().unwrap();
        orders.push(order.clone());
        drop(orders);

        let mut fills = self.fills.write().unwrap();
        fills.push(fill.clone());
        drop(fills);

        // 获取或创建持仓
        let position_side = PositionSide::from_order_side(side);
        let existing = positions.get(symbol).cloned();

        match existing {
            None => {
                // 新开仓
                let margin = compute_margin_required(fill.notional, order.leverage);
                let mut pos = Position {
                    symbol: symbol.clone(),
                    exchange: fill.exchange.clone(),
                    side: position_side,
                    quantity: fill.quantity,
                    avg_entry_price: fill.price,
                    mark_price: Some(fill.price),
                    notional: Some(fill.notional),
                    unrealized_pnl: 0.0,
                    realized_pnl: 0.0,
                    margin: Some(margin),
                    leverage: order.leverage,
                    liquidation_price: None,
                    stop_loss_price: order.stop_price,
                    take_profit_price: None,
                    opened_at: fill.fill_time,
                };
                if let Some(current_price) = self.current_prices.read().unwrap().get(symbol) {
                    pos.update_mark_price(*current_price);
                }
                // 扣减保证金和手续费
                account.cash -= margin + fill.fee;
                account.margin_used += margin;
                account.total_notional += fill.notional;
                positions.insert(symbol.clone(), pos);
            }
            Some(mut existing_pos) => {
                // 判断是加仓还是减仓/平仓
                if existing_pos.side == position_side {
                    // 加仓
                    let total_qty = existing_pos.quantity + fill.quantity;
                    let total_cost = existing_pos.avg_entry_price * existing_pos.quantity
                        + fill.price * fill.quantity;
                    let new_avg = total_cost / total_qty;

                    let margin_delta = compute_margin_required(fill.notional, order.leverage);

                    existing_pos.quantity = total_qty;
                    existing_pos.avg_entry_price = new_avg;

                    account.cash -= margin_delta + fill.fee;
                    account.margin_used += margin_delta;
                    account.total_notional += fill.notional;

                    if let Some(current_price) = self.current_prices.read().unwrap().get(symbol) {
                        existing_pos.update_mark_price(*current_price);
                    }
                    positions.insert(symbol.clone(), existing_pos);
                } else {
                    // 反向：先平掉原有持仓，再开新方向（如果数量更多）
                    let close_qty = existing_pos.quantity.min(fill.quantity);
                    let remaining_qty = fill.quantity - close_qty;

                    // 计算平仓盈亏
                    let pnl = match existing_pos.side {
                        PositionSide::Long => (fill.price - existing_pos.avg_entry_price) * close_qty,
                        PositionSide::Short => (existing_pos.avg_entry_price - fill.price) * close_qty,
                    };

                    // 释放保证金
                    let margin_released = existing_pos.margin.unwrap_or(0.0)
                        * (close_qty / existing_pos.quantity);

                    account.cash += margin_released + pnl - fill.fee;
                    account.margin_used -= margin_released;
                    account.total_notional -= close_qty * existing_pos.avg_entry_price;
                    account.realized_pnl += pnl;

                    if remaining_qty <= 0.0 {
                        // 完全平仓（可能有部分剩余）
                        existing_pos.quantity -= close_qty;
                        if existing_pos.quantity <= 0.0 {
                            positions.remove(symbol);
                        } else {
                            if let Some(current_price) =
                                self.current_prices.read().unwrap().get(symbol)
                            {
                                existing_pos.update_mark_price(*current_price);
                            }
                            positions.insert(symbol.clone(), existing_pos);
                        }
                    } else {
                        // 平仓后开反向仓
                        let new_margin = compute_margin_required(
                            remaining_qty * fill.price,
                            order.leverage,
                        );
                        let mut new_pos = Position {
                            symbol: symbol.clone(),
                            exchange: fill.exchange.clone(),
                            side: position_side,
                            quantity: remaining_qty,
                            avg_entry_price: fill.price,
                            mark_price: Some(fill.price),
                            notional: Some(remaining_qty * fill.price),
                            unrealized_pnl: 0.0,
                            realized_pnl: 0.0,
                            margin: Some(new_margin),
                            leverage: order.leverage,
                            liquidation_price: None,
                            stop_loss_price: order.stop_price,
                            take_profit_price: None,
                            opened_at: fill.fill_time,
                        };
                        account.cash -= new_margin;
                        account.margin_used += new_margin;
                        account.total_notional += remaining_qty * fill.price;

                        if let Some(current_price) =
                            self.current_prices.read().unwrap().get(symbol)
                        {
                            new_pos.update_mark_price(*current_price);
                        }
                        positions.insert(symbol.clone(), new_pos);
                    }
                }
            }
        }

        // 重新计算权益
        // 更新未实现盈亏和名义价值
        let mut total_unrealized = 0.0;
        let mut total_notional = 0.0;
        for pos in positions.values() {
            total_unrealized += pos.unrealized_pnl;
            total_notional += pos.notional.unwrap_or(0.0);
        }
        account.unrealized_pnl = total_unrealized;
        account.total_notional = total_notional;
        account.recompute_equity();
    }

    /// 手动平仓（用于止损/止盈触发）
    pub fn close_position(&self, symbol: &str, price: f64, reason: &str) -> Option<Fill> {
        let position = self.positions.read().unwrap().get(symbol).cloned()?;
        let side = match position.side {
            PositionSide::Long => OrderSide::Sell,
            PositionSide::Short => OrderSide::Buy,
        };
        let order = Order {
            order_id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            exchange: position.exchange.clone(),
            side,
            order_type: OrderType::Market,
            quantity: position.quantity,
            filled_quantity: 0.0,
            price: None,
            stop_price: None,
            leverage: position.leverage,
            status: OrderStatus::Pending,
            client_order_id: Some(format!("close_{}", reason)),
            strategy_id: None,
            agent_id: None,
            signal_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result = self.execute_market_order(&order, price);
        result.fills.into_iter().next()
    }

    /// 获取总成交数量
    pub fn total_fills(&self) -> usize {
        self.fills.read().unwrap().len()
    }

    /// 获取总手续费
    pub fn total_fees(&self) -> f64 {
        self.fills.read().unwrap().iter().map(|f| f.fee).sum()
    }
}

// async_trait for InMemoryExecutor
#[async_trait::async_trait]
impl ExecutionEngine for InMemoryExecutor {
    async fn submit_order(&self, order: Order) -> ExecutionResult {
        // 检查账户余额
        let account = self.account.read().unwrap().clone();
        let notional = match order.order_type {
            OrderType::Market => {
                let price = *self
                    .current_prices
                    .read()
                    .unwrap()
                    .get(&order.symbol)
                    .unwrap_or(&1.0);
                price * order.quantity
            }
            OrderType::Limit => {
                order.price.unwrap_or(1.0) * order.quantity
            }
            _ => 0.0,
        };
        let margin_needed = compute_margin_required(notional, order.leverage);
        let fee_est = compute_fee(notional, self.config.fee_taker_bps);

        if account.cash < margin_needed + fee_est {
            return ExecutionResult {
                order: order.clone(),
                fills: vec![],
                success: false,
                error: Some("Insufficient margin".to_string()),
            };
        }
        drop(account);

        // 市价单直接成交
        match order.order_type {
            OrderType::Market => {
                let price = *self
                    .current_prices
                    .read()
                    .unwrap()
                    .get(&order.symbol)
                    .unwrap_or(&1.0);
                self.execute_market_order(&order, price)
            }
            _ => {
                // 限价单等暂不支持完整撮合，直接返回待提交状态
                ExecutionResult {
                    order: order.clone(),
                    fills: vec![],
                    success: false,
                    error: Some("Only market orders supported in memory executor".to_string()),
                }
            }
        }
    }

    async fn cancel_order(&self, _order_id: Uuid) -> bool {
        false // 简化实现
    }

    async fn get_position(&self, symbol: &str) -> Option<Position> {
        self.positions.read().unwrap().get(symbol).cloned()
    }

    async fn get_all_positions(&self) -> Vec<Position> {
        self.positions.read().unwrap().values().cloned().collect()
    }

    async fn get_account(&self) -> Account {
        self.account.read().unwrap().clone()
    }

    async fn update_price(&self, symbol: &str, price: f64, _timestamp: DateTime<Utc>) {
        self.current_prices.write().unwrap().insert(symbol.to_string(), price);

        // 更新持仓市值
        let mut positions = self.positions.write().unwrap();
        if let Some(pos) = positions.get_mut(symbol) {
            pos.update_mark_price(price);
        }

        // 更新账户未实现盈亏
        let mut total_unrealized = 0.0;
        let mut total_notional = 0.0;
        for pos in positions.values() {
            total_unrealized += pos.unrealized_pnl;
            total_notional += pos.notional.unwrap_or(0.0);
        }
        drop(positions);

        let mut account = self.account.write().unwrap();
        account.unrealized_pnl = total_unrealized;
        account.total_notional = total_notional;
        account.recompute_equity();
    }

    fn config(&self) -> &ExecutionConfig {
        &self.config
    }
}

// ============================================================
// 6. 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_account_initial_state() {
        let acc = Account::new("test", 10000.0);
        assert_eq!(acc.total_equity, 10000.0);
        assert_eq!(acc.cash, 10000.0);
        assert_eq!(acc.drawdown_pct, 0.0);
        assert_eq!(acc.leverage, 0.0);
    }

    #[test]
    fn test_position_update_mark_price_long() {
        let mut pos = Position {
            symbol: "BTC".to_string(),
            exchange: None,
            side: PositionSide::Long,
            quantity: 1.0,
            avg_entry_price: 100.0,
            mark_price: Some(100.0),
            notional: Some(100.0),
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            margin: Some(50.0),
            leverage: 2,
            liquidation_price: None,
            stop_loss_price: Some(95.0),
            take_profit_price: Some(110.0),
            opened_at: Utc::now(),
        };
        pos.update_mark_price(110.0);
        assert_eq!(pos.unrealized_pnl, 10.0, "做多涨 10% 应盈利 10");
        assert!(pos.is_take_profit_triggered(110.0));
        assert!(!pos.is_stop_loss_triggered(110.0));
    }

    #[test]
    fn test_position_update_mark_price_short() {
        let mut pos = Position {
            symbol: "BTC".to_string(),
            exchange: None,
            side: PositionSide::Short,
            quantity: 1.0,
            avg_entry_price: 100.0,
            mark_price: Some(100.0),
            notional: Some(100.0),
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            margin: Some(50.0),
            leverage: 2,
            liquidation_price: None,
            stop_loss_price: Some(105.0),
            take_profit_price: Some(90.0),
            opened_at: Utc::now(),
        };
        pos.update_mark_price(90.0);
        assert_eq!(pos.unrealized_pnl, 10.0, "做空跌 10% 应盈利 10");
        assert!(pos.is_take_profit_triggered(90.0));
    }

    #[test]
    fn test_compute_fee() {
        let fee = compute_fee(1000.0, 5.0);
        assert!((fee - 0.5).abs() < 1e-9, "1000 * 5bps = 0.5");
    }

    #[test]
    fn test_apply_slippage_buy() {
        let (price, _) = apply_slippage(OrderSide::Buy, 100.0, 100.0); // 1%
        assert!(price > 100.0, "买入滑点应向上");
        assert!((price - 101.0).abs() < 0.5);
    }

    #[test]
    fn test_apply_slippage_sell() {
        let (price, _) = apply_slippage(OrderSide::Sell, 100.0, 100.0);
        assert!(price < 100.0, "卖出滑点应向下");
    }

    #[test]
    fn test_compute_margin_required() {
        let margin = compute_margin_required(1000.0, 10);
        assert_eq!(margin, 100.0);
    }

    #[test]
    fn test_in_memory_executor_open_long() {
        let config = ExecutionConfig {
            fee_taker_bps: 0.0, // 0 手续费方便测试
            slippage_bps: 0.0,
            default_leverage: 1,
            ..Default::default()
        };
        let executor = InMemoryExecutor::new(config, 10000.0);

        // 先设置价格
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            executor.update_price("BTC", 100.0, Utc::now()).await;

            let order = Order {
                order_id: Uuid::new_v4(),
                symbol: "BTC".to_string(),
                exchange: None,
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity: 1.0,
                filled_quantity: 0.0,
                price: None,
                stop_price: None,
                leverage: 1,
                status: OrderStatus::Pending,
                client_order_id: None,
                strategy_id: None,
                agent_id: None,
                signal_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            let result = executor.submit_order(order).await;
            assert!(result.success, "应成功成交: {:?}", result.error);
            assert_eq!(result.fills.len(), 1);

            let pos = executor.get_position("BTC").await;
            assert!(pos.is_some(), "应有持仓");
            let pos = pos.unwrap();
            assert_eq!(pos.side, PositionSide::Long);
            assert_eq!(pos.quantity, 1.0);

            let acc = executor.get_account().await;
            // 买入 1 BTC @ 100 = 100 USDT 名义价值
            // 现金减少 100，权益 = 现金 + 未实现盈亏 = 9900 + 0 = 10000
            assert_eq!(acc.total_notional, 100.0);
            assert_eq!(acc.margin_used, 100.0);
        });
    }

    #[test]
    fn test_in_memory_executor_profit_and_loss() {
        let config = ExecutionConfig {
            fee_taker_bps: 0.0,
            slippage_bps: 0.0,
            default_leverage: 1,
            ..Default::default()
        };
        let executor = InMemoryExecutor::new(config, 10000.0);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            executor.update_price("BTC", 100.0, Utc::now()).await;

            let order = Order {
                order_id: Uuid::new_v4(),
                symbol: "BTC".to_string(),
                exchange: None,
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity: 1.0,
                filled_quantity: 0.0,
                price: None,
                stop_price: None,
                leverage: 1,
                status: OrderStatus::Pending,
                client_order_id: None,
                strategy_id: None,
                agent_id: None,
                signal_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            executor.submit_order(order).await;

            // 价格上涨
            executor.update_price("BTC", 110.0, Utc::now()).await;
            let acc = executor.get_account().await;
            assert_eq!(acc.unrealized_pnl, 10.0, "未实现盈亏应为 +10");
            assert_eq!(acc.total_equity, 10010.0, "权益应为 10010");

            // 平仓
            let fill = executor.close_position("BTC", 110.0, "test");
            assert!(fill.is_some());

            let acc = executor.get_account().await;
            assert_eq!(acc.realized_pnl, 10.0, "已实现盈亏应为 +10");
            assert_eq!(acc.unrealized_pnl, 0.0, "未实现盈亏应为 0");
            assert_eq!(acc.total_equity, 10010.0, "权益仍为 10010");
            assert_eq!(acc.total_notional, 0.0, "名义价值应为 0");

            let pos = executor.get_position("BTC").await;
            assert!(pos.is_none(), "平仓后应无持仓");
        });
    }

    #[test]
    fn test_in_memory_executor_insufficient_margin() {
        let config = ExecutionConfig {
            fee_taker_bps: 0.0,
            slippage_bps: 0.0,
            default_leverage: 1,
            ..Default::default()
        };
        let executor = InMemoryExecutor::new(config, 50.0); // 只有 50 USDT

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            executor.update_price("BTC", 100.0, Utc::now()).await;

            let order = Order {
                order_id: Uuid::new_v4(),
                symbol: "BTC".to_string(),
                exchange: None,
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity: 1.0, // 需要 100 USDT 保证金
                filled_quantity: 0.0,
                price: None,
                stop_price: None,
                leverage: 1,
                status: OrderStatus::Pending,
                client_order_id: None,
                strategy_id: None,
                agent_id: None,
                signal_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            let result = executor.submit_order(order).await;
            assert!(!result.success, "保证金不足应失败");
            assert!(result.error.unwrap().contains("Insufficient margin"));
        });
    }

    #[test]
    fn test_in_memory_executor_short_position() {
        let config = ExecutionConfig {
            fee_taker_bps: 0.0,
            slippage_bps: 0.0,
            default_leverage: 1,
            ..Default::default()
        };
        let executor = InMemoryExecutor::new(config, 10000.0);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            executor.update_price("BTC", 100.0, Utc::now()).await;

            let order = Order {
                order_id: Uuid::new_v4(),
                symbol: "BTC".to_string(),
                exchange: None,
                side: OrderSide::Sell,
                order_type: OrderType::Market,
                quantity: 1.0,
                filled_quantity: 0.0,
                price: None,
                stop_price: None,
                leverage: 1,
                status: OrderStatus::Pending,
                client_order_id: None,
                strategy_id: None,
                agent_id: None,
                signal_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            let result = executor.submit_order(order).await;
            assert!(result.success);

            // 价格下跌，做空盈利
            executor.update_price("BTC", 90.0, Utc::now()).await;
            let acc = executor.get_account().await;
            assert_eq!(acc.unrealized_pnl, 10.0, "做空下跌应盈利 10");

            let pos = executor.get_position("BTC").await.unwrap();
            assert_eq!(pos.side, PositionSide::Short);
        });
    }

    #[test]
    fn test_account_drawdown() {
        let mut acc = Account::new("test", 10000.0);
        // 先涨到 12000
        acc.cash = 12000.0;
        acc.recompute_equity();
        assert_eq!(acc.peak_equity, 12000.0);
        assert_eq!(acc.drawdown_pct, 0.0);

        // 回撤到 10000
        acc.cash = 10000.0;
        acc.recompute_equity();
        assert_eq!(acc.drawdown_pct, (12000.0 - 10000.0) / 12000.0);
    }

    #[test]
    fn test_order_side_opposite() {
        assert_eq!(OrderSide::Buy.opposite(), OrderSide::Sell);
        assert_eq!(OrderSide::Sell.opposite(), OrderSide::Buy);
    }

    #[test]
    fn test_position_side_from_order() {
        assert_eq!(PositionSide::from_order_side(OrderSide::Buy), PositionSide::Long);
        assert_eq!(PositionSide::from_order_side(OrderSide::Sell), PositionSide::Short);
    }

    #[test]
    fn test_multiple_positions() {
        let config = ExecutionConfig {
            fee_taker_bps: 0.0,
            slippage_bps: 0.0,
            default_leverage: 1,
            ..Default::default()
        };
        let executor = InMemoryExecutor::new(config, 10000.0);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            executor.update_price("BTC", 100.0, Utc::now()).await;
            executor.update_price("ETH", 50.0, Utc::now()).await;

            // 开 BTC 多单
            let order1 = Order {
                order_id: Uuid::new_v4(),
                symbol: "BTC".to_string(),
                exchange: None,
                side: OrderSide::Buy,
                order_type: OrderType::Market,
                quantity: 1.0,
                filled_quantity: 0.0,
                price: None,
                stop_price: None,
                leverage: 1,
                status: OrderStatus::Pending,
                client_order_id: None,
                strategy_id: None,
                agent_id: None,
                signal_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            executor.submit_order(order1).await;

            // 开 ETH 空单
            let order2 = Order {
                order_id: Uuid::new_v4(),
                symbol: "ETH".to_string(),
                exchange: None,
                side: OrderSide::Sell,
                order_type: OrderType::Market,
                quantity: 2.0,
                filled_quantity: 0.0,
                price: None,
                stop_price: None,
                leverage: 1,
                status: OrderStatus::Pending,
                client_order_id: None,
                strategy_id: None,
                agent_id: None,
                signal_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            executor.submit_order(order2).await;

            let positions = executor.get_all_positions().await;
            assert_eq!(positions.len(), 2);

            let acc = executor.get_account().await;
            // BTC: 100, ETH: 100，总名义价值 200
            assert_eq!(acc.total_notional, 200.0);
        });
    }
}
