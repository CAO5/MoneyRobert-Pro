use chrono::NaiveDateTime;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

use crate::websocket::WebSocketManager;

const SYMBOLS: &[&str] = &[
    "BTC-USDT-SWAP",
    "ETH-USDT-SWAP",
    "SOL-USDT-SWAP",
    "DOGE-USDT-SWAP",
    "XRP-USDT-SWAP",
    "ADA-USDT-SWAP",
    "AVAX-USDT-SWAP",
    "DOT-USDT-SWAP",
    "LINK-USDT-SWAP",
    "MATIC-USDT-SWAP",
    "UNI-USDT-SWAP",
    "ATOM-USDT-SWAP",
    "LTC-USDT-SWAP",
    "FIL-USDT-SWAP",
    "APT-USDT-SWAP",
    "ARB-USDT-SWAP",
    "OP-USDT-SWAP",
    "NEAR-USDT-SWAP",
    "SUI-USDT-SWAP",
    "PEPE-USDT-SWAP",
];

const TICKER_INTERVAL_SECS: u64 = 10;
const KLINES_INTERVAL_SECS: u64 = 60;
const FUNDING_INTERVAL_SECS: u64 = 300;
const ORDERBOOK_INTERVAL_SECS: u64 = 5;
const TRADES_INTERVAL_SECS: u64 = 10;
const LIQUIDATION_INTERVAL_SECS: u64 = 30;
const BASIS_INTERVAL_SECS: u64 = 60;

pub struct MarketCollector {
    db: PgPool,
    ws: Arc<WebSocketManager>,
    http_client: std::sync::Mutex<Option<reqwest::Client>>,
    direct_client: reqwest::Client,
    last_proxy_url: std::sync::Mutex<Option<String>>,
}

impl MarketCollector {
    pub fn new(db: PgPool, ws: Arc<WebSocketManager>) -> Self {
        let direct_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to create direct HTTP client");
        Self { db, ws, http_client: std::sync::Mutex::new(None), direct_client, last_proxy_url: std::sync::Mutex::new(None) }
    }

    /// Get or create a reqwest::Client, rebuilding only when proxy config changes
    async fn get_http_client(&self) -> reqwest::Client {
        let proxy_url = crate::state::get_proxy_config_from_db(&self.db).await;
        let proxy_str = proxy_url.as_deref().unwrap_or("");

        // Check if proxy config changed
        let needs_rebuild = {
            let last = self.last_proxy_url.lock().unwrap();
            last.as_deref() != Some(proxy_str)
        };

        if !needs_rebuild {
            if let Some(client) = self.http_client.lock().unwrap().clone() {
                return client;
            }
        }

        // Build new client with native-tls for better CONNECT proxy support
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(std::time::Duration::from_secs(60));

        if let Some(url) = &proxy_url {
            if !url.is_empty() {
                // Try HTTPS-specific proxy first (better for CONNECT tunneling)
                let https_proxy = reqwest::Proxy::https(url.as_str());
                let all_proxy = reqwest::Proxy::all(url.as_str());

                match https_proxy {
                    Ok(proxy) => {
                        tracing::info!("Market collector using HTTPS proxy from DB: {}", url);
                        builder = builder.proxy(proxy);
                        // Also set HTTP proxy for non-HTTPS URLs
                        if let Ok(http_proxy) = reqwest::Proxy::http(url.as_str()) {
                            builder = builder.proxy(http_proxy);
                        }
                    }
                    Err(_) => match all_proxy {
                        Ok(proxy) => {
                            tracing::info!("Market collector using ALL proxy from DB: {}", url);
                            builder = builder.proxy(proxy);
                        }
                        Err(e) => {
                            tracing::error!("Market collector failed to create proxy '{}': {}", url, e);
                        }
                    }
                }
            }
        } else {
            if let Ok(env_proxy) = std::env::var("ALL_PROXY")
                .or_else(|_| std::env::var("HTTPS_PROXY"))
                .or_else(|_| std::env::var("HTTP_PROXY"))
            {
                let env_proxy = env_proxy.replace("socks5h://", "socks5://").replace("https://", "http://");
                if let Ok(proxy) = reqwest::Proxy::all(&env_proxy) {
                    tracing::info!("Market collector using proxy from env: {}", env_proxy);
                    builder = builder.proxy(proxy);
                }
            }
        }

        let client = builder.build().expect("Failed to create HTTP client");
        *self.http_client.lock().unwrap() = Some(client.clone());
        *self.last_proxy_url.lock().unwrap() = Some(proxy_str.to_string());
        client
    }

    /// Send GET request: try direct first, fall back to proxy if direct fails
    async fn http_get(&self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        // Try direct connection first (works in Docker without proxy for OKX)
        match self.direct_client.get(url).send().await {
            Ok(resp) => Ok(resp),
            Err(direct_err) => {
                tracing::debug!("Direct request failed for {}, trying proxy: {:?}", url, direct_err);
                let client = self.get_http_client().await;
                match client.get(url).send().await {
                    Ok(resp) => Ok(resp),
                    Err(proxy_err) => {
                        tracing::warn!("Both direct and proxy failed for {}: direct={:?}, proxy={:?}", url, direct_err, proxy_err);
                        Err(proxy_err)
                    }
                }
            }
        }
    }

    pub async fn start(self: Arc<Self>) {
        let ticker_collector = self.clone();
        let kline_collector = self.clone();
        let funding_collector = self.clone();
        let orderbook_collector = self.clone();
        let trades_collector = self.clone();
        let liquidation_collector = self.clone();
        let basis_collector = self.clone();

        tokio::spawn(async move {
            loop {
                if let Err(e) = ticker_collector.fetch_tickers().await {
                    tracing::warn!("Ticker fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(TICKER_INTERVAL_SECS)).await;
            }
        });

        tokio::spawn(async move {
            loop {
                if let Err(e) = kline_collector.fetch_klines().await {
                    tracing::warn!("Kline fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(KLINES_INTERVAL_SECS)).await;
            }
        });

        tokio::spawn(async move {
            loop {
                if let Err(e) = funding_collector.fetch_funding_rates().await {
                    tracing::warn!("Funding rate fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(FUNDING_INTERVAL_SECS)).await;
            }
        });

        // 微结构数据采集（第二阶段任务2）
        tokio::spawn(async move {
            loop {
                if let Err(e) = orderbook_collector.fetch_orderbooks().await {
                    tracing::warn!("Orderbook fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(ORDERBOOK_INTERVAL_SECS)).await;
            }
        });

        tokio::spawn(async move {
            loop {
                if let Err(e) = trades_collector.fetch_trades().await {
                    tracing::warn!("Trades fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(TRADES_INTERVAL_SECS)).await;
            }
        });

        tokio::spawn(async move {
            loop {
                if let Err(e) = liquidation_collector.fetch_liquidations().await {
                    tracing::warn!("Liquidation fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(LIQUIDATION_INTERVAL_SECS)).await;
            }
        });

        tokio::spawn(async move {
            loop {
                if let Err(e) = basis_collector.fetch_basis_data().await {
                    tracing::warn!("Basis fetch error: {:?}", e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(BASIS_INTERVAL_SECS)).await;
            }
        });

        tracing::info!("Market data collector started (ticker, kline, funding, orderbook, trades, liquidations, basis)");
    }

    async fn fetch_tickers(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for symbol in SYMBOLS {
            let url = format!(
                "https://www.okx.com/api/v5/market/ticker?instId={}",
                symbol
            );

            let resp = self.http_get(&url).await?;
            let body: serde_json::Value = resp.json().await?;

            if let Some(data) = body.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                let last: f64 = data.get("last").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let open_24h: f64 = data.get("open24h").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let high_24h: f64 = data.get("high24h").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let low_24h: f64 = data.get("low24h").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let vol_24h: f64 = data.get("vol24h").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let bid: f64 = data.get("bidPx").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let ask: f64 = data.get("askPx").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);

                sqlx::query(
                    r#"INSERT INTO ticker_history (symbol, last, open_24h, high_24h, low_24h, volume_24h, best_bid, best_ask)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind(symbol)
                .bind(last)
                .bind(open_24h)
                .bind(high_24h)
                .bind(low_24h)
                .bind(vol_24h)
                .bind(bid)
                .bind(ask)
                .execute(&self.db)
                .await
                .ok();

                let change_percent = if open_24h > 0.0 {
                    (last - open_24h) / open_24h * 100.0
                } else {
                    0.0
                };

                let msg = json!({
                    "type": "ticker",
                    "data": {
                        "symbol": symbol,
                        "last": last,
                        "open_24h": open_24h,
                        "high_24h": high_24h,
                        "low_24h": low_24h,
                        "volume_24h": vol_24h,
                        "best_bid": bid,
                        "best_ask": ask,
                        "change_percent_24h": change_percent,
                    },
                    "timestamp": chrono::Utc::now().timestamp(),
                });

                self.ws.broadcast_to_all(axum::extract::ws::Message::Text(
                    axum::extract::ws::Utf8Bytes::from(msg.to_string()),
                ));
            }
        }

        self.cleanup_old_data("ticker_history", "24 hours").await;
        Ok(())
    }

    async fn fetch_klines(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let intervals = vec![("1H", "1H"), ("5m", "5m"), ("15m", "15m"), ("30m", "30m"), ("4H", "4H"), ("1D", "1D")];

        for symbol in SYMBOLS {
            for (bar, db_interval) in &intervals {
                let url = format!(
                    "https://www.okx.com/api/v5/market/candles?instId={}&bar={}&limit=2",
                    symbol, bar
                );

                let resp = self.http_get(&url).await?;
                let body: serde_json::Value = resp.json().await?;

                if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                    for candle in data {
                        let arr = match candle.as_array() {
                            Some(a) => a,
                            None => continue,
                        };
                        if arr.len() < 7 {
                            continue;
                        }

                        let ts_ms: i64 = arr[0].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                        let open_time = NaiveDateTime::from_timestamp_opt(ts_ms / 1000, 0)
                            .unwrap_or_default();
                        let open: f64 = arr[1].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        let high: f64 = arr[2].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        let low: f64 = arr[3].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        let close: f64 = arr[4].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        let volume: f64 = arr[5].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                        let is_closed = arr[8].as_str().map(|s| s == "1").unwrap_or(true);

                        sqlx::query(
                            r#"INSERT INTO klines (symbol, "interval", open_time, open, high, low, close, volume, is_closed)
                            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                            ON CONFLICT (symbol, "interval", open_time) DO UPDATE SET
                                high = EXCLUDED.high, low = EXCLUDED.low, close = EXCLUDED.close,
                                volume = EXCLUDED.volume, is_closed = EXCLUDED.is_closed,
                                updated_at = NOW()"#,
                        )
                        .bind(symbol)
                        .bind(db_interval)
                        .bind(open_time)
                        .bind(open)
                        .bind(high)
                        .bind(low)
                        .bind(close)
                        .bind(volume)
                        .bind(is_closed)
                        .execute(&self.db)
                        .await
                        .ok();
                    }
                }
            }

            let msg = json!({
                "type": "kline_update",
                "data": { "symbol": symbol },
                "timestamp": chrono::Utc::now().timestamp(),
            });

            self.ws.broadcast_to_all(axum::extract::ws::Message::Text(
                axum::extract::ws::Utf8Bytes::from(msg.to_string()),
            ));
        }

        Ok(())
    }

    async fn fetch_funding_rates(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for symbol in SYMBOLS {
            let url = format!(
                "https://www.okx.com/api/v5/public/funding-rate?instId={}",
                symbol
            );

            let resp = self.http_get(&url).await?;
            let body: serde_json::Value = resp.json().await?;

            if let Some(data) = body.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                let funding_rate: f64 = data.get("fundingRate").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let funding_time_str = data.get("fundingTime").and_then(|v| v.as_str()).unwrap_or("0");
                let funding_time_ms: i64 = funding_time_str.parse().unwrap_or(0);
                let funding_time = NaiveDateTime::from_timestamp_opt(funding_time_ms / 1000, 0).unwrap_or_default();
                let next_funding_time_str = data.get("nextFundingTime").and_then(|v| v.as_str()).unwrap_or("0");
                let next_funding_ms: i64 = next_funding_time_str.parse().unwrap_or(0);

                sqlx::query(
                    r#"INSERT INTO funding_rate_history (symbol, funding_rate, funding_time, created_at)
                    VALUES ($1, $2, $3, NOW())"#,
                )
                .bind(symbol)
                .bind(funding_rate)
                .bind(funding_time)
                .execute(&self.db)
                .await
                .ok();

                let msg = json!({
                    "type": "funding_rate",
                    "data": {
                        "symbol": symbol,
                        "funding_rate": funding_rate,
                        "funding_time": funding_time.to_string(),
                        "next_funding_time": next_funding_ms,
                    },
                    "timestamp": chrono::Utc::now().timestamp(),
                });

                self.ws.broadcast_to_all(axum::extract::ws::Message::Text(
                    axum::extract::ws::Utf8Bytes::from(msg.to_string()),
                ));
            }
        }

        self.cleanup_old_data("funding_rate_history", "7 days").await;
        Ok(())
    }

    /// 采集订单簿快照（第二阶段任务2 - 微结构数据）
    /// OKX API: /api/v5/market/books?instId={symbol}&sz=20
    async fn fetch_orderbooks(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for symbol in SYMBOLS {
            let url = format!(
                "https://www.okx.com/api/v5/market/books?instId={}&sz=20",
                symbol
            );

            let resp = self.http_get(&url).await?;
            let body: serde_json::Value = resp.json().await?;

            if let Some(data) = body.get("data").and_then(|d| d.as_array()).and_then(|a| a.first()) {
                let bids_raw = data.get("bids").and_then(|v| v.as_array());
                let asks_raw = data.get("asks").and_then(|v| v.as_array());

                let parse_levels = |levels: Option<&Vec<serde_json::Value>>| -> Vec<(f64, f64)> {
                    levels
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|lvl| {
                                    let a = lvl.as_array()?;
                                    if a.len() < 2 {
                                        return None;
                                    }
                                    let price: f64 = a[0].as_str()?.parse().ok()?;
                                    let size: f64 = a[1].as_str()?.parse().ok()?;
                                    Some((price, size))
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                };

                let bids = parse_levels(bids_raw);
                let asks = parse_levels(asks_raw);

                if bids.is_empty() || asks.is_empty() {
                    continue;
                }

                let snap = crate::microstructure::build_orderbook_snapshot(
                    symbol,
                    "okx",
                    &bids,
                    &asks,
                    chrono::Utc::now(),
                );

                let _ = crate::microstructure::save_orderbook_snapshot(&self.db, &snap).await;
            }
        }

        self.cleanup_old_data("orderbook_snapshots", "7 days").await;
        Ok(())
    }

    /// 采集逐笔成交（第二阶段任务2 - 微结构数据）
    /// OKX API: /api/v5/market/trades?instId={symbol}&limit=50
    async fn fetch_trades(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for symbol in SYMBOLS {
            let url = format!(
                "https://www.okx.com/api/v5/market/trades?instId={}&limit=50",
                symbol
            );

            let resp = self.http_get(&url).await?;
            let body: serde_json::Value = resp.json().await?;

            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                for trade in data {
                    let arr = match trade.as_array() {
                        Some(a) => a,
                        None => continue,
                    };
                    // OKX trades: [instId, tradeId, price, sz, side, ts, ...]
                    // 实际字段顺序: tradeId, px, sz, side, ts
                    let trade_id = arr.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let price: f64 = arr.get(2).and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let size: f64 = arr.get(3).and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let side_str = arr.get(4).and_then(|v| v.as_str()).unwrap_or("buy");
                    let ts_ms: i64 = arr.get(5).and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);

                    if price <= 0.0 || size <= 0.0 {
                        continue;
                    }

                    let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms)
                        .unwrap_or_else(chrono::Utc::now);

                    // OKX side: "buy" 表示主动买入（taker 买入），is_buyer_maker = false
                    // "sell" 表示主动卖出（taker 卖出），is_buyer_maker = true
                    let (side, is_buyer_maker) = match side_str {
                        "buy" => ("buy".to_string(), false),
                        "sell" => ("sell".to_string(), true),
                        _ => ("buy".to_string(), false),
                    };

                    let tick = crate::microstructure::TradeTick {
                        tick_id: 0,
                        symbol: symbol.to_string(),
                        exchange: "okx".into(),
                        trade_id,
                        timestamp,
                        price,
                        size,
                        notional: price * size,
                        side,
                        is_buyer_maker,
                        created_at: chrono::Utc::now(),
                    };

                    let _ = crate::microstructure::save_trade_tick(&self.db, &tick).await;
                }
            }
        }

        self.cleanup_old_data("trade_ticks", "3 days").await;
        Ok(())
    }

    /// 采集清算事件（第二阶段任务2 - 微结构数据）
    /// OKX API: /api/v5/public/liquidation-orders?instType=SWAP&state=filled
    async fn fetch_liquidations(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = "https://www.okx.com/api/v5/public/liquidation-orders?instType=SWAP&state=filled&limit=100";

        let resp = self.http_get(url).await?;
        let body: serde_json::Value = resp.json().await?;

        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            for item in data {
                // liquidation-orders 返回结构: { instId, ... details: [{side, px, sz, ...}] }
                let symbol = item.get("instId").and_then(|v| v.as_str()).unwrap_or("");
                if symbol.is_empty() {
                    continue;
                }

                let details = item.get("details").and_then(|v| v.as_array());
                if let Some(details_arr) = details {
                    for detail in details_arr {
                        let side_str = detail.get("side").and_then(|v| v.as_str()).unwrap_or("");
                        // OKX: side "long" 表示多仓被强平，"short" 表示空仓被强平
                        let side = match side_str {
                            "long" => "long".to_string(),
                            "short" => "short".to_string(),
                            _ => continue,
                        };

                        let price: f64 = detail.get("px")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let size: f64 = detail.get("sz")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let ts_ms: i64 = detail.get("ts")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);

                        if price <= 0.0 || size <= 0.0 {
                            continue;
                        }

                        let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms)
                            .unwrap_or_else(chrono::Utc::now);

                        let event = crate::microstructure::LiquidationEvent {
                            event_id: 0,
                            symbol: symbol.to_string(),
                            exchange: "okx".into(),
                            timestamp,
                            side,
                            price,
                            size,
                            notional: price * size,
                            liquidation_type: Some("forced".into()),
                            created_at: chrono::Utc::now(),
                        };

                        let _ = crate::microstructure::save_liquidation_event(&self.db, &event).await;
                    }
                }
            }
        }

        self.cleanup_old_data("liquidation_events", "30 days").await;
        Ok(())
    }

    /// 采集基差数据（第二阶段任务2 - 微结构数据）
    /// 永续合约价格来自 ticker，现货价格来自 spot ticker，资金费率来自 funding-rate
    async fn fetch_basis_data(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for symbol in SYMBOLS {
            // 永续合约 symbol: BTC-USDT-SWAP
            // 现货 symbol: BTC-USDT
            let spot_symbol = symbol.replace("-SWAP", "");

            // 获取永续合约最新价格
            let perp_url = format!(
                "https://www.okx.com/api/v5/market/ticker?instId={}",
                symbol
            );
            let perp_resp = self.http_get(&perp_url).await?;
            let perp_body: serde_json::Value = perp_resp.json().await?;
            let perp_price = perp_body
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .and_then(|d| d.get("last"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            // 获取现货最新价格
            let spot_url = format!(
                "https://www.okx.com/api/v5/market/ticker?instId={}",
                spot_symbol
            );
            let spot_resp = self.http_get(&spot_url).await?;
            let spot_body: serde_json::Value = spot_resp.json().await?;
            let spot_price = spot_body
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .and_then(|d| d.get("last"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            // 获取资金费率
            let funding_url = format!(
                "https://www.okx.com/api/v5/public/funding-rate?instId={}",
                symbol
            );
            let funding_resp = self.http_get(&funding_url).await?;
            let funding_body: serde_json::Value = funding_resp.json().await?;
            let funding_rate = funding_body
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .and_then(|d| d.get("fundingRate"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok());

            if perp_price <= 0.0 || spot_price <= 0.0 {
                continue;
            }

            let (perp_basis, perp_basis_pct, _fut_basis, _fut_basis_pct) =
                crate::microstructure::compute_basis(spot_price, perp_price, None, funding_rate);

            let funding_annualized = funding_rate.map(|r| r * 3.0 * 365.0);

            let data = crate::microstructure::BasisData {
                basis_id: 0,
                symbol: symbol.to_string(),
                exchange: "okx".into(),
                timestamp: chrono::Utc::now(),
                spot_price: Some(spot_price),
                perp_price: Some(perp_price),
                futures_price: None,
                futures_expiry: None,
                perp_basis: Some(perp_basis),
                perp_basis_pct: Some(perp_basis_pct),
                futures_basis: None,
                futures_basis_pct: None,
                funding_rate,
                funding_rate_annualized: funding_annualized,
                created_at: chrono::Utc::now(),
            };

            let _ = crate::microstructure::save_basis_data(&self.db, &data).await;
        }

        self.cleanup_old_data("basis_data", "30 days").await;
        Ok(())
    }

    async fn cleanup_old_data(&self, table: &str, retention: &str) {
        let query = format!(
            "DELETE FROM {} WHERE created_at < NOW() - INTERVAL '{}'",
            table, retention
        );
        sqlx::query(&query)
            .execute(&self.db)
            .await
            .ok();
    }
}
