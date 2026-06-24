pub mod binance;
pub mod okx;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 统一行情数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedTicker {
    pub exchange: String,
    pub symbol: String,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub volume_24h: f64,
    pub quote_volume_24h: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 统一 K 线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedKline {
    pub exchange: String,
    pub symbol: String,
    pub interval: String,
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub quote_volume: f64,
    pub is_closed: bool,
}

/// 交易所行情接口 trait
///
/// 抽象不同交易所的公开市场数据 API，用于跨交易所数据采集和比较
#[async_trait]
pub trait ExchangeMarketData: Send + Sync {
    /// 交易所名称
    fn exchange_name(&self) -> &str;

    /// 获取单个交易对的 ticker
    async fn get_ticker(&self, symbol: &str) -> Result<UnifiedTicker, String>;

    /// 获取 K 线数据
    async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<UnifiedKline>, String>;

    /// 将统一 symbol 转换为交易所特定格式
    /// 例如：BTC-USDT-SWAP -> BTCUSDT (Binance) 或 BTC-USDT-SWAP (OKX)
    fn normalize_symbol(&self, symbol: &str) -> String;
}

/// 跨交易所价格比较
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossExchangePrice {
    pub symbol: String,
    pub prices: Vec<ExchangePrice>,
    pub spread_pct: f64,
    pub best_bid_exchange: String,
    pub best_ask_exchange: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 单个交易所的价格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangePrice {
    pub exchange: String,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub volume_24h: f64,
}
