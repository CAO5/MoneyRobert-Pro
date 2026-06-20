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

        tracing::info!("Market data collector started");
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
