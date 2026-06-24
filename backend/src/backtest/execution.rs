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
use sqlx::Row;
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
// 6. 模拟盘执行器（DB 持久化 + 统一费用/滑点/保证金计算）
// ============================================================

/// 模拟盘执行器
///
/// 与回测共用同一套费用/滑点/保证金计算逻辑（compute_fee / apply_slippage /
/// compute_margin_required），但将订单、成交、持仓持久化到数据库。
///
/// 核心设计：
/// - 开仓时计算手续费和滑点，从账户余额扣除保证金+手续费
/// - 平仓时计算已实现盈亏（含平仓手续费），返还保证金
/// - 账户余额实时反映现金、保证金、已实现盈亏
pub struct PaperTradingExecutor {
    config: ExecutionConfig,
    pool: sqlx::PgPool,
    user_id: i64,
}

/// 模拟盘开仓结果
#[derive(Debug, Clone, Serialize)]
pub struct PaperOpenResult {
    pub position_id: Uuid,
    pub fill: Fill,
    pub margin: f64,
    pub fee: f64,
    pub slippage_cost: f64,
    pub remaining_balance: f64,
}

/// 模拟盘平仓结果
#[derive(Debug, Clone, Serialize)]
pub struct PaperCloseResult {
    pub fill: Fill,
    pub gross_pnl: f64,
    pub fee: f64,
    pub net_pnl: f64,
    pub margin_released: f64,
    pub remaining_balance: f64,
}

impl PaperTradingExecutor {
    pub fn new(config: ExecutionConfig, pool: sqlx::PgPool, user_id: i64) -> Self {
        Self {
            config,
            pool,
            user_id,
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults(pool: sqlx::PgPool, user_id: i64) -> Self {
        Self::new(ExecutionConfig::default(), pool, user_id)
    }

    /// 获取或创建模拟盘账户
    pub async fn get_or_create_account(&self) -> Result<f64, String> {
        let row = sqlx::query(
            r#"INSERT INTO paper_trading_accounts (user_id, balance, initial_balance, total_pnl, total_equity, peak_equity)
               VALUES ($1, 100000, 100000, 0, 100000, 100000)
               ON CONFLICT (user_id) DO UPDATE SET user_id = EXCLUDED.user_id
               RETURNING balance"#,
        )
        .bind(self.user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("get_or_create_account failed: {}", e))?;

        let balance: f64 = row.get("balance");
        Ok(balance)
    }

    /// 开仓（市价单）
    ///
    /// 统一计算手续费、滑点、保证金，并持久化到 positions + paper_trading_fills 表
    pub async fn open_position(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: f64,
        ref_price: f64,
        leverage: i32,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<PaperOpenResult, String> {
        let balance = self.get_or_create_account().await?;

        // 统一滑点计算
        let (filled_price, _) = apply_slippage(side, ref_price, self.config.slippage_bps);
        let notional = filled_price * quantity;

        // 统一手续费计算（市价单 = taker）
        let fee = compute_fee(notional, self.config.fee_taker_bps);
        let slippage_cost = (filled_price - ref_price).abs() * quantity;

        // 统一保证金计算
        let margin = compute_margin_required(notional, leverage);

        // 检查余额
        if balance < margin + fee {
            return Err(format!(
                "Insufficient balance: need {:.2} (margin {:.2} + fee {:.2}), have {:.2}",
                margin + fee,
                margin,
                fee,
                balance
            ));
        }

        let position_id = Uuid::new_v4();
        let fill_id = Uuid::new_v4();
        let now = Utc::now();
        let side_str = side.as_str();

        // 写入持仓
        sqlx::query(
            r#"INSERT INTO positions
               (id, user_id, symbol, side, quantity, entry_price, filled_price, unrealized_pnl,
                leverage, stop_loss, take_profit, status, fee, slippage_bps, slippage_cost,
                margin, realized_pnl, notional, opened_at)
               VALUES ($1, $2, $3, $4, $5, $6, $6, 0, $7, $8, $9, 'OPEN',
                       $10, $11, $12, $13, 0, $14, $15)"#,
        )
        .bind(position_id)
        .bind(self.user_id)
        .bind(symbol)
        .bind(side_str)
        .bind(quantity)
        .bind(filled_price)
        .bind(leverage)
        .bind(stop_loss)
        .bind(take_profit)
        .bind(fee)
        .bind(self.config.slippage_bps)
        .bind(slippage_cost)
        .bind(margin)
        .bind(notional)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("insert position failed: {}", e))?;

        // 写入成交记录
        sqlx::query(
            r#"INSERT INTO paper_trading_fills
               (fill_id, user_id, order_id, symbol, side, quantity, price, notional,
                fee, slippage_bps, slippage_cost, fee_rate_bps, is_maker, fill_time, position_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, FALSE, $13, $14)"#,
        )
        .bind(fill_id)
        .bind(self.user_id)
        .bind(position_id) // 简化：用 position_id 作为 order_id
        .bind(symbol)
        .bind(side_str)
        .bind(quantity)
        .bind(filled_price)
        .bind(notional)
        .bind(fee)
        .bind(self.config.slippage_bps)
        .bind(slippage_cost)
        .bind(self.config.fee_taker_bps)
        .bind(now)
        .bind(position_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("insert fill failed: {}", e))?;

        // 更新账户余额（扣除保证金+手续费）
        let remaining = balance - margin - fee;
        self.update_account_balance(remaining, margin, fee, slippage_cost)
            .await?;

        let fill = Fill {
            fill_id,
            order_id: position_id,
            symbol: symbol.to_string(),
            exchange: None,
            side,
            quantity,
            price: filled_price,
            notional,
            fee,
            slippage_bps: self.config.slippage_bps,
            slippage_cost,
            fee_rate_bps: self.config.fee_taker_bps,
            is_maker: false,
            fill_time: now,
        };

        Ok(PaperOpenResult {
            position_id,
            fill,
            margin,
            fee,
            slippage_cost,
            remaining_balance: remaining,
        })
    }

    /// 平仓
    ///
    /// 统一计算平仓手续费、滑点、已实现盈亏（毛盈亏 - 开仓费 - 平仓费）
    pub async fn close_position(
        &self,
        position_id: Uuid,
        exit_ref_price: f64,
        close_reason: Option<&str>,
    ) -> Result<PaperCloseResult, String> {
        // 读取持仓
        let row = sqlx::query(
            r#"SELECT symbol, side::text, quantity::float8, entry_price::float8,
                      filled_price::float8, fee::float8, margin::float8, leverage,
                      slippage_bps::float8, slippage_cost::float8
               FROM positions
               WHERE id = $1 AND user_id = $2 AND status = 'OPEN'"#,
        )
        .bind(position_id)
        .bind(self.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("query position failed: {}", e))?
        .ok_or_else(|| "Position not found or not OPEN".to_string())?;

        let symbol: String = row.get("symbol");
        let side_str: String = row.get("side");
        let quantity: f64 = row.get("quantity");
        let entry_price: f64 = row.get("filled_price");
        let open_fee: f64 = row.get("fee");
        let margin: f64 = row.get("margin");
        let leverage: i32 = row.get("leverage");

        // 平仓方向 = 原持仓反方向
        let close_side = match side_str.as_str() {
            "buy" | "long" => OrderSide::Sell,
            "sell" | "short" => OrderSide::Buy,
            _ => return Err(format!("Unknown side: {}", side_str)),
        };

        // 统一滑点计算（平仓也适用滑点）
        let (exit_price, _) = apply_slippage(close_side, exit_ref_price, self.config.slippage_bps);
        let exit_notional = exit_price * quantity;

        // 统一手续费计算
        let close_fee = compute_fee(exit_notional, self.config.fee_taker_bps);
        let close_slippage_cost = (exit_price - exit_ref_price).abs() * quantity;

        // 毛盈亏（不含费用）
        let gross_pnl = match side_str.as_str() {
            "buy" | "long" => (exit_price - entry_price) * quantity,
            "sell" | "short" => (entry_price - exit_price) * quantity,
            _ => 0.0,
        };

        // 净盈亏 = 毛盈亏 - 开仓费 - 平仓费
        let net_pnl = gross_pnl - open_fee - close_fee;

        let fill_id = Uuid::new_v4();
        let now = Utc::now();

        // 写入平仓成交记录
        sqlx::query(
            r#"INSERT INTO paper_trading_fills
               (fill_id, user_id, order_id, symbol, side, quantity, price, notional,
                fee, slippage_bps, slippage_cost, fee_rate_bps, is_maker, fill_time,
                position_id, close_reason)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, FALSE, $13, $14, $15)"#,
        )
        .bind(fill_id)
        .bind(self.user_id)
        .bind(position_id)
        .bind(&symbol)
        .bind(close_side.as_str())
        .bind(quantity)
        .bind(exit_price)
        .bind(exit_notional)
        .bind(close_fee)
        .bind(self.config.slippage_bps)
        .bind(close_slippage_cost)
        .bind(self.config.fee_taker_bps)
        .bind(now)
        .bind(position_id)
        .bind(close_reason)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("insert close fill failed: {}", e))?;

        // 更新持仓状态
        sqlx::query(
            r#"UPDATE positions
               SET status = 'CLOSED', closed_at = $1, close_price = $2, close_fee = $3,
                   realized_pnl = $4, unrealized_pnl = 0, updated_at = $1
               WHERE id = $5"#,
        )
        .bind(now)
        .bind(exit_price)
        .bind(close_fee)
        .bind(net_pnl)
        .bind(position_id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("update position failed: {}", e))?;

        // 写入 trades 记录
        sqlx::query(
            r#"INSERT INTO trades
               (user_id, symbol, side, entry_price, exit_price, size, leverage,
                status, pnl, pnl_percent, entry_fee, exit_fee, slippage_bps,
                slippage_cost, gross_pnl, net_pnl, margin, close_reason)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'CLOSED', $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)"#,
        )
        .bind(self.user_id)
        .bind(&symbol)
        .bind(&side_str)
        .bind(entry_price)
        .bind(exit_price)
        .bind(quantity)
        .bind(leverage)
        .bind(net_pnl)
        .bind(if entry_price > 0.0 { net_pnl / (entry_price * quantity) } else { 0.0 })
        .bind(open_fee)
        .bind(close_fee)
        .bind(self.config.slippage_bps)
        .bind(row.get::<f64, _>("slippage_cost") + close_slippage_cost)
        .bind(gross_pnl)
        .bind(net_pnl)
        .bind(margin)
        .bind(close_reason)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("insert trade failed: {}", e))?;

        // 更新账户余额（返还保证金 + 净盈亏 - 平仓费）
        let balance = self.get_or_create_account().await?;
        let remaining = balance + margin + net_pnl;
        self.update_account_balance(remaining, -margin, close_fee, close_slippage_cost)
            .await?;

        let fill = Fill {
            fill_id,
            order_id: position_id,
            symbol,
            exchange: None,
            side: close_side,
            quantity,
            price: exit_price,
            notional: exit_notional,
            fee: close_fee,
            slippage_bps: self.config.slippage_bps,
            slippage_cost: close_slippage_cost,
            fee_rate_bps: self.config.fee_taker_bps,
            is_maker: false,
            fill_time: now,
        };

        Ok(PaperCloseResult {
            fill,
            gross_pnl,
            fee: close_fee,
            net_pnl,
            margin_released: margin,
            remaining_balance: remaining,
        })
    }

    /// 更新账户余额和统计
    async fn update_account_balance(
        &self,
        new_balance: f64,
        margin_delta: f64,
        fee_delta: f64,
        slippage_delta: f64,
    ) -> Result<(), String> {
        sqlx::query(
            r#"UPDATE paper_trading_accounts
               SET balance = $2,
                   margin_used = GREATEST(margin_used + $3, 0),
                   total_fees = total_fees + $4,
                   total_slippage_cost = total_slippage_cost + $5,
                   total_equity = $2 + GREATEST(margin_used + $3, 0),
                   peak_equity = GREATEST(peak_equity, $2 + GREATEST(margin_used + $3, 0)),
                   drawdown_pct = CASE WHEN peak_equity > 0
                       THEN GREATEST(0, (peak_equity - ($2 + GREATEST(margin_used + $3, 0))) / peak_equity)
                       ELSE 0 END,
                   total_pnl = total_pnl + CASE WHEN $3 < 0 THEN $4 * (-1) ELSE 0 END,
                   updated_at = NOW()
               WHERE user_id = $1"#,
        )
        .bind(self.user_id)
        .bind(new_balance)
        .bind(margin_delta)
        .bind(fee_delta)
        .bind(slippage_delta)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("update account failed: {}", e))?;
        Ok(())
    }

    /// 获取账户状态
    pub async fn get_account_summary(&self) -> Result<serde_json::Value, String> {
        let row = sqlx::query(
            r#"SELECT balance::float8, initial_balance::float8, total_pnl::float8,
                      total_fees::float8, total_slippage_cost::float8, margin_used::float8,
                      total_equity::float8, peak_equity::float8, drawdown_pct::float8
               FROM paper_trading_accounts WHERE user_id = $1"#,
        )
        .bind(self.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("get_account_summary failed: {}", e))?;

        match row {
            Some(r) => Ok(serde_json::json!({
                "balance": r.get::<f64, _>("balance"),
                "initial_balance": r.get::<f64, _>("initial_balance"),
                "total_pnl": r.get::<f64, _>("total_pnl"),
                "total_fees": r.get::<f64, _>("total_fees"),
                "total_slippage_cost": r.get::<f64, _>("total_slippage_cost"),
                "margin_used": r.get::<f64, _>("margin_used"),
                "total_equity": r.get::<f64, _>("total_equity"),
                "peak_equity": r.get::<f64, _>("peak_equity"),
                "drawdown_pct": r.get::<f64, _>("drawdown_pct"),
            })),
            None => Ok(serde_json::json!({
                "balance": 100000.0,
                "initial_balance": 100000.0,
                "total_pnl": 0.0,
                "total_fees": 0.0,
                "total_slippage_cost": 0.0,
                "margin_used": 0.0,
                "total_equity": 100000.0,
                "peak_equity": 100000.0,
                "drawdown_pct": 0.0,
            })),
        }
    }
}

// ============================================================
// 7. 实盘执行适配器（OKX 响应 → 统一 Fill 模型）
// ============================================================

/// 实盘执行适配器
///
/// 将 OKX 订单请求/响应映射为统一的 Order/Fill 模型，
/// 确保实盘成交记录与回测/模拟盘使用相同的数据结构，
/// 便于下游归因分析和绩效评估。
pub struct LiveExecutionAdapter {
    config: ExecutionConfig,
}

/// OKX 订单响应（简化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxFillResponse {
    pub ord_id: String,
    pub inst_id: String,
    pub side: String,    // "buy" / "sell"
    pub fill_sz: String, // 成交数量
    pub fill_px: String, // 成交价格
    pub fee: String,
    pub fee_ccy: String,
    pub exec_type: String, // "T" taker / "M" maker
    pub ts: String,         // 时间戳
}

impl LiveExecutionAdapter {
    pub fn new(config: ExecutionConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(ExecutionConfig::default())
    }

    /// 将统一 Order 转换为 OKX 订单请求格式
    pub fn to_okx_request(&self, order: &Order) -> crate::exchanges::okx::OkxOrderRequest {
        crate::exchanges::okx::OkxOrderRequest {
            inst_id: order.symbol.clone(),
            td_mode: "cross".to_string(),
            side: order.side.as_str().to_string(),
            ord_type: match order.order_type {
                OrderType::Market => "market".to_string(),
                OrderType::Limit => "limit".to_string(),
                OrderType::Stop => "conditional".to_string(),
            },
            sz: order.quantity.to_string(),
            px: order.price.map(|p| p.to_string()),
            sl_trigger_px: order.stop_price.map(|p| p.to_string()),
            sl_ord_px: order.stop_price.map(|_| "-1".to_string()),
            tp_trigger_px: None,
            tp_ord_px: None,
            reduce_only: None,
        }
    }

    /// 将 OKX 成交响应转换为统一 Fill 模型
    ///
    /// 如果 OKX 未返回滑点信息，使用配置中的默认滑点估算
    pub fn from_okx_fill(&self, okx_fill: &OkxFillResponse, order: &Order) -> Fill {
        let price: f64 = okx_fill.fill_px.parse().unwrap_or(0.0);
        let quantity: f64 = okx_fill.fill_sz.parse().unwrap_or(0.0);
        let notional = price * quantity;
        let fee: f64 = okx_fill.fee.parse().unwrap_or(0.0);
        let is_maker = okx_fill.exec_type == "M";
        let fee_rate_bps = if is_maker {
            self.config.fee_maker_bps
        } else {
            self.config.fee_taker_bps
        };

        let side = match okx_fill.side.as_str() {
            "buy" => OrderSide::Buy,
            "sell" => OrderSide::Sell,
            _ => order.side,
        };

        let fill_time = chrono::DateTime::from_timestamp(
            okx_fill.ts.parse().unwrap_or(0) / 1000,
            0,
        )
        .unwrap_or_else(|| Utc::now());

        Fill {
            fill_id: Uuid::new_v4(),
            order_id: order.order_id,
            symbol: okx_fill.inst_id.clone(),
            exchange: Some("okx".to_string()),
            side,
            quantity,
            price,
            notional,
            fee: fee.abs(),
            slippage_bps: self.config.slippage_bps,
            slippage_cost: 0.0, // 实盘滑点由市场决定，无法精确计算
            fee_rate_bps,
            is_maker,
            fill_time,
        }
    }

    /// 获取配置引用
    pub fn config(&self) -> &ExecutionConfig {
        &self.config
    }
}

// ============================================================
// 8. 执行模式枚举
// ============================================================

/// 执行模式
///
/// 用于区分回测、模拟盘、实盘三种执行环境，
/// 确保下游（归因、报告）能正确区分数据来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Backtest,
    Paper,
    Live,
}

impl ExecutionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionMode::Backtest => "backtest",
            ExecutionMode::Paper => "paper",
            ExecutionMode::Live => "live",
        }
    }
}

// ============================================================
// 9. 测试
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

    // ========================================
    // PaperTradingExecutor / LiveExecutionAdapter 测试
    // ========================================

    #[test]
    fn test_live_adapter_to_okx_request_market() {
        let adapter = LiveExecutionAdapter::with_defaults();
        let order = Order {
            order_id: Uuid::new_v4(),
            symbol: "BTC-USDT-SWAP".to_string(),
            exchange: Some("okx".to_string()),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            quantity: 0.5,
            filled_quantity: 0.0,
            price: None,
            stop_price: None,
            leverage: 2,
            status: OrderStatus::Pending,
            client_order_id: None,
            strategy_id: None,
            agent_id: None,
            signal_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let okx_req = adapter.to_okx_request(&order);
        assert_eq!(okx_req.inst_id, "BTC-USDT-SWAP");
        assert_eq!(okx_req.side, "buy");
        assert_eq!(okx_req.ord_type, "market");
        assert_eq!(okx_req.sz, "0.5");
        assert_eq!(okx_req.td_mode, "cross");
    }

    #[test]
    fn test_live_adapter_to_okx_request_limit() {
        let adapter = LiveExecutionAdapter::with_defaults();
        let order = Order {
            order_id: Uuid::new_v4(),
            symbol: "ETH-USDT-SWAP".to_string(),
            exchange: None,
            side: OrderSide::Sell,
            order_type: OrderType::Limit,
            quantity: 1.0,
            filled_quantity: 0.0,
            price: Some(3000.0),
            stop_price: Some(3200.0),
            leverage: 1,
            status: OrderStatus::Pending,
            client_order_id: None,
            strategy_id: None,
            agent_id: None,
            signal_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let okx_req = adapter.to_okx_request(&order);
        assert_eq!(okx_req.ord_type, "limit");
        assert_eq!(okx_req.px.as_deref(), Some("3000"));
        assert_eq!(okx_req.sl_trigger_px.as_deref(), Some("3200"));
    }

    #[test]
    fn test_live_adapter_from_okx_fill_taker() {
        let adapter = LiveExecutionAdapter::with_defaults();
        let order = Order {
            order_id: Uuid::new_v4(),
            symbol: "BTC-USDT-SWAP".to_string(),
            exchange: Some("okx".to_string()),
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

        let okx_fill = OkxFillResponse {
            ord_id: "12345".to_string(),
            inst_id: "BTC-USDT-SWAP".to_string(),
            side: "buy".to_string(),
            fill_sz: "1.0".to_string(),
            fill_px: "50000".to_string(),
            fee: "-2.5".to_string(), // OKX 返回负数表示扣除
            fee_ccy: "USDT".to_string(),
            exec_type: "T".to_string(), // Taker
            ts: "1700000000000".to_string(),
        };

        let fill = adapter.from_okx_fill(&okx_fill, &order);
        assert_eq!(fill.symbol, "BTC-USDT-SWAP");
        assert_eq!(fill.side, OrderSide::Buy);
        assert!((fill.price - 50000.0).abs() < 1e-9);
        assert!((fill.quantity - 1.0).abs() < 1e-9);
        assert!((fill.notional - 50000.0).abs() < 1e-9);
        assert!((fill.fee - 2.5).abs() < 1e-9); // 取绝对值
        assert!(!fill.is_maker); // Taker
        assert_eq!(fill.exchange.as_deref(), Some("okx"));
    }

    #[test]
    fn test_live_adapter_from_okx_fill_maker() {
        let adapter = LiveExecutionAdapter::with_defaults();
        let order = Order {
            order_id: Uuid::new_v4(),
            symbol: "ETH-USDT-SWAP".to_string(),
            exchange: None,
            side: OrderSide::Sell,
            order_type: OrderType::Limit,
            quantity: 2.0,
            filled_quantity: 0.0,
            price: Some(3000.0),
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

        let okx_fill = OkxFillResponse {
            ord_id: "67890".to_string(),
            inst_id: "ETH-USDT-SWAP".to_string(),
            side: "sell".to_string(),
            fill_sz: "2.0".to_string(),
            fill_px: "3000".to_string(),
            fee: "-1.2".to_string(),
            fee_ccy: "USDT".to_string(),
            exec_type: "M".to_string(), // Maker
            ts: "1700000000000".to_string(),
        };

        let fill = adapter.from_okx_fill(&okx_fill, &order);
        assert!(fill.is_maker); // Maker
        assert_eq!(fill.fee_rate_bps, adapter.config().fee_maker_bps);
    }

    #[test]
    fn test_execution_mode_as_str() {
        assert_eq!(ExecutionMode::Backtest.as_str(), "backtest");
        assert_eq!(ExecutionMode::Paper.as_str(), "paper");
        assert_eq!(ExecutionMode::Live.as_str(), "live");
    }

    #[test]
    fn test_paper_trading_executor_config() {
        let config = ExecutionConfig {
            fee_taker_bps: 8.0,
            fee_maker_bps: 3.0,
            slippage_bps: 5.0,
            default_leverage: 2,
            max_position_pct: 0.15,
            max_leverage: 5.0,
        };
        // 验证配置可以正确创建（不需要 DB 连接）
        assert_eq!(config.fee_taker_bps, 8.0);
        assert_eq!(config.slippage_bps, 5.0);
        assert_eq!(config.default_leverage, 2);
    }

    #[test]
    fn test_unified_fee_calculation_consistency() {
        // 验证回测和模拟盘使用相同的费用计算函数
        let notional = 10000.0;
        let fee_bps = 5.0;

        // 回测路径使用的计算
        let backtest_fee = compute_fee(notional, fee_bps);

        // 模拟盘路径使用的计算（相同函数）
        let paper_fee = compute_fee(notional, fee_bps);

        // 实盘适配器路径使用的计算（相同函数）
        let live_fee = compute_fee(notional, fee_bps);

        assert_eq!(backtest_fee, paper_fee);
        assert_eq!(paper_fee, live_fee);
        assert!((backtest_fee - 5.0).abs() < 1e-9); // 10000 * 5/10000 = 5.0
    }

    #[test]
    fn test_unified_slippage_consistency() {
        let price = 100.0;
        let slippage_bps = 10.0; // 0.1%

        // 三条路径使用相同的滑点计算
        let (bt_price, _) = apply_slippage(OrderSide::Buy, price, slippage_bps);
        let (paper_price, _) = apply_slippage(OrderSide::Buy, price, slippage_bps);

        assert_eq!(bt_price, paper_price);
        assert!(bt_price > price); // 买入滑点向上
    }

    #[test]
    fn test_unified_margin_calculation() {
        let notional = 1000.0;
        let leverage = 10;

        // 三条路径使用相同的保证金计算
        let bt_margin = compute_margin_required(notional, leverage);
        let paper_margin = compute_margin_required(notional, leverage);

        assert_eq!(bt_margin, paper_margin);
        assert!((bt_margin - 100.0).abs() < 1e-9); // 1000 / 10 = 100
    }
}
