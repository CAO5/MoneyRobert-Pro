//! Ledger Service
//! 账本服务 — 统一交易执行入口
//!
//! 依据《系统 2.0 待补全问题清单》P1-4 / 8.3.2：
//! 统一开仓/加仓/减仓/平仓/反手；
//! 统一手续费、滑点、资金费；
//! 统一权益、保证金、杠杆；
//! 对接回测、模拟盘、实盘。

use crate::backtest::matching_engine::{MatchingEngine, MatchingConfig};
use crate::backtest::models::{
    AccountState, SimulatedFill, SimulatedOrder, SimulatedPosition, TradeAttribution,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// 账本错误
#[derive(Debug, Clone)]
pub enum LedgerError {
    /// 仓位不存在
    PositionNotFound(String),
    /// 仓位已平仓
    PositionAlreadyClosed(Uuid),
    /// 数量无效
    InvalidQuantity(f64),
    /// 价格无效
    InvalidPrice(f64),
    /// 保证金不足
    InsufficientMargin(f64, f64),
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PositionNotFound(msg) => write!(f, "position_not_found: {}", msg),
            Self::PositionAlreadyClosed(id) => write!(f, "position_already_closed: {}", id),
            Self::InvalidQuantity(q) => write!(f, "invalid_quantity: {}", q),
            Self::InvalidPrice(p) => write!(f, "invalid_price: {}", p),
            Self::InsufficientMargin(needed, available) => {
                write!(f, "insufficient_margin: needed={} available={}", needed, available)
            }
        }
    }
}

impl std::error::Error for LedgerError {}

/// 账本条目 — 记录一次交易操作的结果
#[derive(Debug, Clone)]
pub struct LedgerEntry {
    /// 操作 ID
    pub entry_id: Uuid,
    /// 关联的仓位 ID
    pub position_id: Option<Uuid>,
    /// 操作类型：open/increase/reduce/close/reverse
    pub operation: String,
    /// 资产
    pub asset: String,
    /// 方向：buy/sell
    pub side: String,
    /// 成交数量
    pub quantity: f64,
    /// 成交价格
    pub price: f64,
    /// 成交金额
    pub notional: f64,
    /// 手续费
    pub fee: f64,
    /// 滑点成本
    pub slippage_cost: f64,
    /// 已实现盈亏（仅平仓时非零）
    pub realized_pnl: f64,
    /// 释放的保证金（仅平仓时非零）
    pub released_margin: f64,
    /// 新增保证金（仅开仓/加仓时非零）
    pub added_margin: f64,
    /// 操作时间
    pub timestamp: DateTime<Utc>,
}

/// 账本服务
///
/// 封装 MatchingEngine，提供统一的交易执行接口。
/// 所有交易操作（开仓/加仓/减仓/平仓/反手）都通过此服务执行，
/// 确保手续费、滑点、保证金、权益的计算口径一致。
pub struct LedgerService {
    matching: MatchingEngine,
    /// 交易日志
    entries: Vec<LedgerEntry>,
    /// 交易归因记录
    attributions: Vec<TradeAttribution>,
}

impl LedgerService {
    pub fn new(config: MatchingConfig) -> Self {
        Self {
            matching: MatchingEngine::new(config),
            entries: Vec::new(),
            attributions: Vec::new(),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(MatchingConfig::default())
    }

    /// 开仓
    ///
    /// 创建一个新仓位。如果已有同方向仓位，将自动加仓。
    /// 如果已有反方向仓位，将触发反手操作。
    pub fn open_position(
        &mut self,
        asset: &str,
        side: &str,
        quantity: f64,
        price: f64,
        account: &mut AccountState,
        positions: &mut Vec<SimulatedPosition>,
        now: DateTime<Utc>,
    ) -> Result<LedgerEntry, LedgerError> {
        if quantity <= 0.0 {
            return Err(LedgerError::InvalidQuantity(quantity));
        }
        if price <= 0.0 {
            return Err(LedgerError::InvalidPrice(price));
        }

        let notional = price * quantity;
        let fee = self.matching.compute_fee(notional, false);
        let fill = SimulatedFill {
            fill_id: Uuid::new_v4(),
            order_id: Uuid::new_v4(),
            job_id: Some(account.job_id),
            asset: asset.to_string(),
            exchange: None,
            side: side.to_string(),
            filled_quantity: quantity,
            filled_price: price,
            notional: Some(notional),
            fee,
            slippage_bps: Some(0.0),
            slippage_cost: Some(0.0),
            maker_taker: "taker".into(),
            signal_id: None,
            strategy_id: None,
            agent_id: None,
            intent_type: Some("open_position".into()),
            fill_time: now,
        };

        let (new_pos, realized) = self.matching.apply_fill(&fill, positions, account);

        let entry = LedgerEntry {
            entry_id: Uuid::new_v4(),
            position_id: new_pos.as_ref().map(|p| p.position_id),
            operation: "open".into(),
            asset: asset.to_string(),
            side: side.to_string(),
            quantity,
            price,
            notional,
            fee,
            slippage_cost: 0.0,
            realized_pnl: realized,
            released_margin: 0.0,
            added_margin: notional, // 简化：使用 notional 作为保证金占用记录
            timestamp: now,
        };
        self.entries.push(entry.clone());
        Ok(entry)
    }

    /// 平仓
    ///
    /// 完全平掉指定仓位。使用撮合引擎计算手续费和滑点。
    pub fn close_position(
        &mut self,
        position: &SimulatedPosition,
        price: f64,
        account: &mut AccountState,
        positions: &mut Vec<SimulatedPosition>,
        now: DateTime<Utc>,
    ) -> Result<LedgerEntry, LedgerError> {
        if position.closed_at.is_some() {
            return Err(LedgerError::PositionAlreadyClosed(position.position_id));
        }
        if price <= 0.0 {
            return Err(LedgerError::InvalidPrice(price));
        }

        let fill = self.matching.close_position_at_price(position, price, now);
        let margin_before = account.margin_used;
        let (_, realized) = self.matching.apply_fill(&fill, positions, account);
        let margin_after = account.margin_used;
        let released_margin = (margin_before - margin_after).max(0.0);

        let entry = LedgerEntry {
            entry_id: Uuid::new_v4(),
            position_id: Some(position.position_id),
            operation: "close".into(),
            asset: position.asset.clone(),
            side: fill.side.clone(),
            quantity: fill.filled_quantity,
            price: fill.filled_price,
            notional: fill.notional.unwrap_or(0.0),
            fee: fill.fee,
            slippage_cost: fill.slippage_cost.unwrap_or(0.0),
            realized_pnl: realized,
            released_margin,
            added_margin: 0.0,
            timestamp: now,
        };
        self.entries.push(entry.clone());
        Ok(entry)
    }

    /// 获取所有交易日志
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    /// 获取交易归因记录
    pub fn attributions(&self) -> &[TradeAttribution] {
        &self.attributions
    }

    /// 记录交易归因
    pub fn record_attribution(&mut self, attribution: TradeAttribution) {
        self.attributions.push(attribution);
    }

    /// 获取匹配引擎引用（用于高级操作）
    pub fn matching(&self) -> &MatchingEngine {
        &self.matching
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_account() -> AccountState {
        AccountState::new(Uuid::new_v4(), 10000.0, Utc::now())
    }

    #[test]
    fn test_open_position_creates_entry() {
        let mut ledger = LedgerService::with_defaults();
        let mut account = make_account();
        let mut positions = Vec::new();

        let entry = ledger
            .open_position("BTC", "buy", 1.0, 100.0, &mut account, &mut positions, Utc::now())
            .unwrap();

        assert_eq!(entry.operation, "open");
        assert_eq!(entry.asset, "BTC");
        assert!(entry.fee > 0.0);
        assert_eq!(positions.len(), 1);
        // 合约口径：cash 只扣 fee
        assert!((account.cash - (10000.0 - entry.fee)).abs() < 1e-9);
        assert!(account.margin_used > 0.0);
    }

    #[test]
    fn test_close_position_releases_margin() {
        let mut ledger = LedgerService::with_defaults();
        let mut account = make_account();
        let mut positions = Vec::new();

        // 开仓
        ledger
            .open_position("BTC", "buy", 1.0, 100.0, &mut account, &mut positions, Utc::now())
            .unwrap();
        let margin_after_open = account.margin_used;
        assert!(margin_after_open > 0.0);

        // 平仓
        let pos = positions[0].clone();
        let entry = ledger
            .close_position(&pos, 110.0, &mut account, &mut positions, Utc::now())
            .unwrap();

        assert_eq!(entry.operation, "close");
        assert!(entry.realized_pnl > 0.0); // 110 > 100，盈利
        assert!(entry.released_margin > 0.0);
        // 全平后保证金归零
        assert!((account.margin_used - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_invalid_quantity_rejected() {
        let mut ledger = LedgerService::with_defaults();
        let mut account = make_account();
        let mut positions = Vec::new();

        let result = ledger.open_position("BTC", "buy", -1.0, 100.0, &mut account, &mut positions, Utc::now());
        assert!(matches!(result, Err(LedgerError::InvalidQuantity(_))));
    }

    #[test]
    fn test_invalid_price_rejected() {
        let mut ledger = LedgerService::with_defaults();
        let mut account = make_account();
        let mut positions = Vec::new();

        let result = ledger.open_position("BTC", "buy", 1.0, 0.0, &mut account, &mut positions, Utc::now());
        assert!(matches!(result, Err(LedgerError::InvalidPrice(_))));
    }

    #[test]
    fn test_close_already_closed_position() {
        let mut ledger = LedgerService::with_defaults();
        let mut account = make_account();
        let mut positions = Vec::new();

        // 开仓 + 平仓
        ledger
            .open_position("BTC", "buy", 1.0, 100.0, &mut account, &mut positions, Utc::now())
            .unwrap();
        let pos = positions[0].clone();
        ledger
            .close_position(&pos, 110.0, &mut account, &mut positions, Utc::now())
            .unwrap();

        // 再次平仓应失败（使用已更新的仓位状态）
        let closed_pos = positions.iter().find(|p| p.position_id == pos.position_id).unwrap().clone();
        let result = ledger.close_position(&closed_pos, 120.0, &mut account, &mut positions, Utc::now());
        assert!(matches!(result, Err(LedgerError::PositionAlreadyClosed(_))));
    }

    #[test]
    fn test_entries_recorded() {
        let mut ledger = LedgerService::with_defaults();
        let mut account = make_account();
        let mut positions = Vec::new();

        // 开仓 + 平仓 = 2 条记录
        ledger
            .open_position("BTC", "buy", 1.0, 100.0, &mut account, &mut positions, Utc::now())
            .unwrap();
        let pos = positions[0].clone();
        ledger
            .close_position(&pos, 110.0, &mut account, &mut positions, Utc::now())
            .unwrap();

        assert_eq!(ledger.entries().len(), 2);
        assert_eq!(ledger.entries()[0].operation, "open");
        assert_eq!(ledger.entries()[1].operation, "close");
    }
}
