//! Binance Futures Market Data Client
//! 币安合约市场数据客户端
//!
//! 实现交易所行情接口 trait，提供与 OKX 统一的行情数据访问。
//! 仅使用公开接口（无需 API Key），用于跨交易所数据比较。

use async_trait::async_trait;
use serde::Deserialize;

use super::{ExchangeMarketData, UnifiedKline, UnifiedTicker};

const BINANCE_FAPI_BASE: &str = "https://fapi.binance.com";

/// Binance 合约客户端（公开市场数据）
pub struct BinanceClient {
    http_client: reqwest::Client,
}

impl BinanceClient {
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create Binance HTTP client");
        Self { http_client }
    }

    /// 将 OKX 格式 symbol 转换为 Binance 格式
    /// BTC-USDT-SWAP -> BTCUSDT
    /// ETH-USDT-SWAP -> ETHUSDT
    fn to_binance_symbol(symbol: &str) -> String {
        // 移除 -SWAP 后缀，移除所有连字符
        symbol
            .trim_end_matches("-SWAP")
            .replace('-', "")
    }

    /// 将 Binance interval 格式转换为统一格式
    /// Binance: 1m, 5m, 15m, 30m, 1h, 4h, 1d, 1w
    /// 统一: 1m, 5m, 15m, 30m, 1H, 4H, 1D, 1W
    fn from_binance_interval(interval: &str) -> String {
        match interval {
            "1h" => "1H".to_string(),
            "4h" => "4H".to_string(),
            "1d" => "1D".to_string(),
            "1w" => "1W".to_string(),
            _ => interval.to_string(),
        }
    }

    /// 将统一 interval 转换为 Binance 格式
    fn to_binance_interval(interval: &str) -> String {
        match interval {
            "1H" => "1h".to_string(),
            "4H" => "4h".to_string(),
            "1D" => "1d".to_string(),
            "1W" => "1w".to_string(),
            _ => interval.to_string(),
        }
    }
}

impl Default for BinanceClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Binance 24hr ticker 响应
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceTicker24hr {
    symbol: String,
    last_price: String,
    bid_price: String,
    ask_price: String,
    volume: String,
    quote_volume: String,
    high_price: String,
    low_price: String,
    close_time: i64,
}

/// Binance K 线响应（数组格式）
/// [open_time, open, high, low, close, volume, close_time, quote_volume, trades, taker_buy_base, taker_buy_quote, ignore]

#[async_trait]
impl ExchangeMarketData for BinanceClient {
    fn exchange_name(&self) -> &str {
        "binance"
    }

    fn normalize_symbol(&self, symbol: &str) -> String {
        Self::to_binance_symbol(symbol)
    }

    async fn get_ticker(&self, symbol: &str) -> Result<UnifiedTicker, String> {
        let binance_symbol = Self::to_binance_symbol(symbol);
        let url = format!(
            "{}/fapi/v1/ticker/24hr?symbol={}",
            BINANCE_FAPI_BASE, binance_symbol
        );

        let resp: BinanceTicker24hr = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Binance ticker request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Binance ticker parse failed: {}", e))?;

        let timestamp = chrono::DateTime::from_timestamp(resp.close_time / 1000, 0)
            .unwrap_or_else(chrono::Utc::now);

        Ok(UnifiedTicker {
            exchange: "binance".to_string(),
            symbol: symbol.to_string(),
            last_price: resp.last_price.parse().unwrap_or(0.0),
            bid_price: resp.bid_price.parse().unwrap_or(0.0),
            ask_price: resp.ask_price.parse().unwrap_or(0.0),
            volume_24h: resp.volume.parse().unwrap_or(0.0),
            quote_volume_24h: resp.quote_volume.parse().unwrap_or(0.0),
            high_24h: resp.high_price.parse().unwrap_or(0.0),
            low_24h: resp.low_price.parse().unwrap_or(0.0),
            timestamp,
        })
    }

    async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<UnifiedKline>, String> {
        let binance_symbol = Self::to_binance_symbol(symbol);
        let binance_interval = Self::to_binance_interval(interval);
        let url = format!(
            "{}/fapi/v1/klines?symbol={}&interval={}&limit={}",
            BINANCE_FAPI_BASE, binance_symbol, binance_interval, limit
        );

        let resp: Vec<serde_json::Value> = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Binance klines request failed: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Binance klines parse failed: {}", e))?;

        let unified_interval = Self::from_binance_interval(interval);
        let result: Vec<UnifiedKline> = resp
            .iter()
            .filter_map(|kline| {
                let arr = kline.as_array()?;
                if arr.len() < 9 {
                    return None;
                }
                Some(UnifiedKline {
                    exchange: "binance".to_string(),
                    symbol: symbol.to_string(),
                    interval: unified_interval.clone(),
                    open_time: arr[0].as_i64().unwrap_or(0),
                    open: arr[1].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    high: arr[2].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    low: arr[3].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    close: arr[4].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    volume: arr[5].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    quote_volume: arr[7].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    is_closed: true,
                })
            })
            .collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_binance_symbol() {
        assert_eq!(BinanceClient::to_binance_symbol("BTC-USDT-SWAP"), "BTCUSDT");
        assert_eq!(BinanceClient::to_binance_symbol("ETH-USDT-SWAP"), "ETHUSDT");
        assert_eq!(BinanceClient::to_binance_symbol("SOL-USDT-SWAP"), "SOLUSDT");
    }

    #[test]
    fn test_to_binance_symbol_no_swap_suffix() {
        // 即使没有 -SWAP 后缀也能处理
        assert_eq!(BinanceClient::to_binance_symbol("BTC-USDT"), "BTCUSDT");
    }

    #[test]
    fn test_interval_conversion() {
        assert_eq!(BinanceClient::to_binance_interval("1H"), "1h");
        assert_eq!(BinanceClient::to_binance_interval("4H"), "4h");
        assert_eq!(BinanceClient::to_binance_interval("1D"), "1d");
        assert_eq!(BinanceClient::to_binance_interval("1W"), "1w");
        assert_eq!(BinanceClient::to_binance_interval("1m"), "1m");
        assert_eq!(BinanceClient::to_binance_interval("5m"), "5m");

        assert_eq!(BinanceClient::from_binance_interval("1h"), "1H");
        assert_eq!(BinanceClient::from_binance_interval("4h"), "4H");
        assert_eq!(BinanceClient::from_binance_interval("1d"), "1D");
        assert_eq!(BinanceClient::from_binance_interval("1w"), "1W");
        assert_eq!(BinanceClient::from_binance_interval("1m"), "1m");
    }

    #[test]
    fn test_exchange_name() {
        let client = BinanceClient::new();
        assert_eq!(client.exchange_name(), "binance");
    }

    #[test]
    fn test_normalize_symbol() {
        let client = BinanceClient::new();
        assert_eq!(client.normalize_symbol("BTC-USDT-SWAP"), "BTCUSDT");
        assert_eq!(client.normalize_symbol("ETH-USDT-SWAP"), "ETHUSDT");
    }

    #[test]
    fn test_default() {
        let client = BinanceClient::default();
        assert_eq!(client.exchange_name(), "binance");
    }
}
