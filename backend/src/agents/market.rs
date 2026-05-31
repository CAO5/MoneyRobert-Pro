use crate::agents::config::AgentConfig;
use crate::agents::errors::{AgentError, AgentResult};
use crate::agents::models::MarketSnapshot;
use crate::exchanges::okx::OkxClient;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    pub open_time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalIndicators {
    pub rsi: Option<f64>,
    pub macd: Option<MacdData>,
    pub bollinger_bands: Option<BollingerBands>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdData {
    pub macd_line: f64,
    pub signal_line: f64,
    pub histogram: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerBands {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
}

#[async_trait::async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn get_market_snapshot(&self, symbol: &str) -> AgentResult<MarketSnapshot>;
    async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> AgentResult<Vec<KlineData>>;
    async fn get_funding_rate(&self, symbol: &str) -> AgentResult<Option<f64>>;
    async fn get_open_interest(&self, symbol: &str) -> AgentResult<Option<f64>>;
    async fn calculate_technical_indicators(
        &self,
        symbol: &str,
        interval: &str,
        config: &AgentConfig,
    ) -> AgentResult<TechnicalIndicators>;
}

pub struct DatabaseMarketDataProvider {
    db: PgPool,
}

impl DatabaseMarketDataProvider {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub fn calculate_rsi(prices: &[f64], period: usize) -> Option<f64> {
        if prices.len() <= period {
            return None;
        }

        let mut gains = Vec::with_capacity(prices.len() - 1);
        let mut losses = Vec::with_capacity(prices.len() - 1);

        for i in 1..prices.len() {
            let change = prices[i] - prices[i - 1];
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(-change);
            }
        }

        let mut avg_gain = gains[..period].iter().sum::<f64>() / period as f64;
        let mut avg_loss = losses[..period].iter().sum::<f64>() / period as f64;

        for i in period..gains.len() {
            avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;
        }

        if avg_loss == 0.0 {
            return Some(100.0);
        }

        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    pub fn calculate_sma(data: &[f64], period: usize) -> Option<Vec<f64>> {
        if data.len() < period {
            return None;
        }

        let mut sma = Vec::with_capacity(data.len() - period + 1);
        for i in period..=data.len() {
            let sum: f64 = data[i - period..i].iter().sum();
            sma.push(sum / period as f64);
        }
        Some(sma)
    }

    pub fn calculate_ema(data: &[f64], period: usize) -> Option<Vec<f64>> {
        if data.len() < period {
            return None;
        }

        let mut ema = Vec::with_capacity(data.len());
        let multiplier = 2.0 / (period as f64 + 1.0);

        let initial_sma: f64 = data[..period].iter().sum::<f64>() / period as f64;
        ema.push(initial_sma);

        for i in period..data.len() {
            let next_ema = (data[i] - ema[ema.len() - 1]) * multiplier + ema[ema.len() - 1];
            ema.push(next_ema);
        }

        Some(ema)
    }

    pub fn calculate_macd(
        prices: &[f64],
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
    ) -> Option<MacdData> {
        let fast_ema = Self::calculate_ema(prices, fast_period)?;
        let slow_ema = Self::calculate_ema(prices, slow_period)?;

        let fast_ema_len = fast_ema.len();
        let slow_ema_len = slow_ema.len();
        let start_idx = slow_ema_len - fast_ema_len;

        let mut macd_line = Vec::with_capacity(fast_ema_len);
        for i in 0..fast_ema_len {
            macd_line.push(fast_ema[i] - slow_ema[start_idx + i]);
        }

        let signal_line = Self::calculate_ema(&macd_line, signal_period)?;
        let last_signal = signal_line.last()?;
        let last_macd = macd_line.last()?;

        Some(MacdData {
            macd_line: *last_macd,
            signal_line: *last_signal,
            histogram: last_macd - last_signal,
        })
    }

    pub fn calculate_bollinger_bands(
        prices: &[f64],
        period: usize,
        std_dev_multiplier: f64,
    ) -> Option<BollingerBands> {
        if prices.len() < period {
            return None;
        }

        let recent_prices = &prices[prices.len() - period..];
        let sma = recent_prices.iter().sum::<f64>() / period as f64;

        let variance = recent_prices
            .iter()
            .map(|&p| (p - sma).powi(2))
            .sum::<f64>()
            / period as f64;
        let std_dev = variance.sqrt();

        Some(BollingerBands {
            upper: sma + std_dev_multiplier * std_dev,
            middle: sma,
            lower: sma - std_dev_multiplier * std_dev,
        })
    }
}

#[async_trait::async_trait]
impl MarketDataProvider for DatabaseMarketDataProvider {
    async fn get_market_snapshot(&self, symbol: &str) -> AgentResult<MarketSnapshot> {
        let ticker_row = sqlx::query(
            r#"SELECT last, open_24h, high_24h, low_24h, volume_24h
               FROM ticker_history
               WHERE symbol = $1
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(symbol)
        .fetch_optional(&self.db)
        .await?;

        let (current_price, open_24h, high_24h, low_24h, volume_24h) = match ticker_row {
            Some(row) => (
                row.get::<f64, _>("last"),
                row.get::<f64, _>("open_24h"),
                row.get::<f64, _>("high_24h"),
                row.get::<f64, _>("low_24h"),
                row.get::<f64, _>("volume_24h"),
            ),
            None => (0.0, 0.0, 0.0, 0.0, 0.0),
        };

        let price_change_percent_24h = if open_24h > 0.0 {
            (current_price - open_24h) / open_24h * 100.0
        } else {
            0.0
        };

        let funding_rate = self.get_funding_rate(symbol).await?;
        let open_interest = self.get_open_interest(symbol).await?;

        Ok(MarketSnapshot {
            symbol: symbol.to_string(),
            current_price,
            open_24h,
            high_24h,
            low_24h,
            close_24h: current_price,
            volume_24h,
            price_change_percent_24h,
            funding_rate,
            open_interest,
            long_short_ratio: None,
            rsi_14: None,
            macd_signal: None,
            timestamp: Utc::now(),
        })
    }

    async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> AgentResult<Vec<KlineData>> {
        let rows = sqlx::query(
            r#"SELECT open_time, open, high, low, close, volume
               FROM klines
               WHERE symbol = $1 AND "interval" = $2 AND is_closed = true
               ORDER BY open_time DESC
               LIMIT $3"#,
        )
        .bind(symbol)
        .bind(interval)
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        let mut klines = Vec::with_capacity(rows.len());
        for row in rows {
            klines.push(KlineData {
                open_time: row.get::<DateTime<Utc>, _>("open_time"),
                open: row.get::<f64, _>("open"),
                high: row.get::<f64, _>("high"),
                low: row.get::<f64, _>("low"),
                close: row.get::<f64, _>("close"),
                volume: row.get::<f64, _>("volume"),
            });
        }

        klines.reverse();
        Ok(klines)
    }

    async fn get_funding_rate(&self, symbol: &str) -> AgentResult<Option<f64>> {
        let row = sqlx::query(
            r#"SELECT funding_rate
               FROM funding_rate_history
               WHERE symbol = $1
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(symbol)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.map(|r| r.get::<f64, _>("funding_rate")))
    }

    async fn get_open_interest(&self, _symbol: &str) -> AgentResult<Option<f64>> {
        Ok(None)
    }

    async fn calculate_technical_indicators(
        &self,
        symbol: &str,
        interval: &str,
        config: &AgentConfig,
    ) -> AgentResult<TechnicalIndicators> {
        let max_period = *[
            config.rsi_period,
            config.macd_slow_period,
            config.bollinger_bands_period,
        ]
        .iter()
        .max()
        .unwrap();
        let klines = self
            .get_klines(symbol, interval, max_period + 50)
            .await?;

        if klines.is_empty() {
            return Ok(TechnicalIndicators {
                rsi: None,
                macd: None,
                bollinger_bands: None,
            });
        }

        let prices: Vec<f64> = klines.iter().map(|k| k.close).collect();

        let rsi = Self::calculate_rsi(&prices, config.rsi_period);
        let macd = Self::calculate_macd(
            &prices,
            config.macd_fast_period,
            config.macd_slow_period,
            config.macd_signal_period,
        );
        let bollinger_bands = Self::calculate_bollinger_bands(
            &prices,
            config.bollinger_bands_period,
            config.bollinger_bands_std_dev,
        );

        Ok(TechnicalIndicators {
            rsi,
            macd,
            bollinger_bands,
        })
    }
}

pub struct OkxMarketDataProvider {
    db: PgPool,
    okx: OkxClient,
}

impl OkxMarketDataProvider {
    pub fn new(db: PgPool, okx: OkxClient) -> Self {
        Self { db, okx }
    }

    async fn fetch_snapshot_from_okx(&self, symbol: &str) -> AgentResult<MarketSnapshot> {
        let ticker = self
            .okx
            .get_ticker(symbol)
            .await
            .map_err(|e| AgentError::ExternalApiError(e.to_string()))?;

        let current_price = ticker.last.parse::<f64>().unwrap_or(0.0);
        let open_24h = ticker.open_24h.parse::<f64>().unwrap_or(0.0);
        let high_24h = ticker.high_24h.parse::<f64>().unwrap_or(0.0);
        let low_24h = ticker.low_24h.parse::<f64>().unwrap_or(0.0);
        let volume_24h = ticker.vol_24h.parse::<f64>().unwrap_or(0.0);

        let price_change_percent_24h = if open_24h > 0.0 {
            (current_price - open_24h) / open_24h * 100.0
        } else {
            0.0
        };

        let funding_rate = self.get_funding_rate(symbol).await?;
        let open_interest = self.get_open_interest(symbol).await?;

        Ok(MarketSnapshot {
            symbol: symbol.to_string(),
            current_price,
            open_24h,
            high_24h,
            low_24h,
            close_24h: current_price,
            volume_24h,
            price_change_percent_24h,
            funding_rate,
            open_interest,
            long_short_ratio: None,
            rsi_14: None,
            macd_signal: None,
            timestamp: Utc::now(),
        })
    }

    async fn fetch_snapshot_from_db(&self, symbol: &str) -> AgentResult<MarketSnapshot> {
        let db_provider = DatabaseMarketDataProvider::new(self.db.clone());
        db_provider.get_market_snapshot(symbol).await
    }

    async fn fetch_klines_from_okx(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> AgentResult<Vec<KlineData>> {
        let candles = self
            .okx
            .get_candles(symbol, interval, Some(limit))
            .await
            .map_err(|e| AgentError::ExternalApiError(e.to_string()))?;

        let mut klines = Vec::with_capacity(candles.len());
        for candle in &candles {
            let ts_millis = candle.ts.parse::<i64>().unwrap_or(0);
            let open_time = Utc.timestamp_millis_opt(ts_millis).single().unwrap_or(Utc::now());

            klines.push(KlineData {
                open_time,
                open: candle.o.parse::<f64>().unwrap_or(0.0),
                high: candle.h.parse::<f64>().unwrap_or(0.0),
                low: candle.l.parse::<f64>().unwrap_or(0.0),
                close: candle.c.parse::<f64>().unwrap_or(0.0),
                volume: candle.vol.parse::<f64>().unwrap_or(0.0),
            });
        }

        klines.sort_by_key(|k| k.open_time);
        Ok(klines)
    }

    async fn fetch_klines_from_db(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> AgentResult<Vec<KlineData>> {
        let db_provider = DatabaseMarketDataProvider::new(self.db.clone());
        db_provider.get_klines(symbol, interval, limit).await
    }
}

#[async_trait::async_trait]
impl MarketDataProvider for OkxMarketDataProvider {
    async fn get_market_snapshot(&self, symbol: &str) -> AgentResult<MarketSnapshot> {
        match self.fetch_snapshot_from_okx(symbol).await {
            Ok(snapshot) => Ok(snapshot),
            Err(e) => {
                tracing::warn!(
                    "OKX API failed for symbol={}, falling back to DB: {}",
                    symbol,
                    e
                );
                self.fetch_snapshot_from_db(symbol).await
            }
        }
    }

    async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> AgentResult<Vec<KlineData>> {
        match self.fetch_klines_from_okx(symbol, interval, limit).await {
            Ok(klines) => Ok(klines),
            Err(e) => {
                tracing::warn!(
                    "OKX candles API failed for symbol={}, falling back to DB: {}",
                    symbol,
                    e
                );
                self.fetch_klines_from_db(symbol, interval, limit).await
            }
        }
    }

    async fn get_funding_rate(&self, symbol: &str) -> AgentResult<Option<f64>> {
        let row = sqlx::query(
            r#"SELECT funding_rate
               FROM funding_rate_history
               WHERE symbol = $1
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(symbol)
        .fetch_optional(&self.db)
        .await?;

        Ok(row.map(|r| r.get::<f64, _>("funding_rate")))
    }

    async fn get_open_interest(&self, _symbol: &str) -> AgentResult<Option<f64>> {
        Ok(None)
    }

    async fn calculate_technical_indicators(
        &self,
        symbol: &str,
        interval: &str,
        config: &AgentConfig,
    ) -> AgentResult<TechnicalIndicators> {
        let max_period = *[
            config.rsi_period,
            config.macd_slow_period,
            config.bollinger_bands_period,
        ]
        .iter()
        .max()
        .unwrap();
        let klines = self
            .get_klines(symbol, interval, max_period + 50)
            .await?;

        if klines.is_empty() {
            return Ok(TechnicalIndicators {
                rsi: None,
                macd: None,
                bollinger_bands: None,
            });
        }

        let prices: Vec<f64> = klines.iter().map(|k| k.close).collect();

        let rsi = DatabaseMarketDataProvider::calculate_rsi(&prices, config.rsi_period);
        let macd = DatabaseMarketDataProvider::calculate_macd(
            &prices,
            config.macd_fast_period,
            config.macd_slow_period,
            config.macd_signal_period,
        );
        let bollinger_bands = DatabaseMarketDataProvider::calculate_bollinger_bands(
            &prices,
            config.bollinger_bands_period,
            config.bollinger_bands_std_dev,
        );

        Ok(TechnicalIndicators {
            rsi,
            macd,
            bollinger_bands,
        })
    }
}

pub struct CombinedMarketDataProvider {
    okx_provider: OkxMarketDataProvider,
    db_provider: DatabaseMarketDataProvider,
    config: AgentConfig,
}

impl CombinedMarketDataProvider {
    pub fn new(db: PgPool, okx: OkxClient, config: AgentConfig) -> Self {
        Self {
            okx_provider: OkxMarketDataProvider::new(db.clone(), okx),
            db_provider: DatabaseMarketDataProvider::new(db),
            config,
        }
    }
}

#[async_trait::async_trait]
impl MarketDataProvider for CombinedMarketDataProvider {
    async fn get_market_snapshot(&self, symbol: &str) -> AgentResult<MarketSnapshot> {
        let mut snapshot = self.okx_provider.get_market_snapshot(symbol).await?;

        let indicators = self
            .db_provider
            .calculate_technical_indicators(symbol, "1H", &self.config)
            .await?;

        if let Some(rsi) = indicators.rsi {
            snapshot.rsi_14 = Some(rsi);
        }
        if let Some(macd) = indicators.macd {
            snapshot.macd_signal = Some(macd.signal_line);
        }

        Ok(snapshot)
    }

    async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> AgentResult<Vec<KlineData>> {
        self.okx_provider.get_klines(symbol, interval, limit).await
    }

    async fn get_funding_rate(&self, symbol: &str) -> AgentResult<Option<f64>> {
        self.db_provider.get_funding_rate(symbol).await
    }

    async fn get_open_interest(&self, symbol: &str) -> AgentResult<Option<f64>> {
        self.db_provider.get_open_interest(symbol).await
    }

    async fn calculate_technical_indicators(
        &self,
        symbol: &str,
        interval: &str,
        config: &AgentConfig,
    ) -> AgentResult<TechnicalIndicators> {
        self.db_provider
            .calculate_technical_indicators(symbol, interval, config)
            .await
    }
}
