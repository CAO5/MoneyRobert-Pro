//! Replay engine: load historical K-line data + signals, emit sorted events.
//! 数据回放引擎

use crate::backtest::models::{AlphaSignal, Kline, ReplayEvent};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};

pub struct ReplayConfig {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub symbols: Vec<String>,
    pub interval: String,
}

pub struct ReplayEngine {
    config: ReplayConfig,
    events: Vec<ReplayEvent>,
    cursor: usize,
}

impl ReplayEngine {
    /// Create a new engine, loading kline data from the `market_data` table.
    /// Signals are loaded from `alpha_signals` table for the given job_id (if present).
    pub async fn load(
        pool: &PgPool,
        config: ReplayConfig,
        job_id: Option<uuid::Uuid>,
    ) -> Result<Self, String> {
        // 1) Klines - use a simpler query with ANY array
        let sql = r#"SELECT symbol, interval, open_time, open, high, low, close, volume, COALESCE(quote_volume, 0)
               FROM market_data
               WHERE open_time >= $1 AND open_time <= $2
                 AND interval = $3
                 AND symbol = ANY($4::varchar[])
               ORDER BY open_time ASC"#;
        let rows = sqlx::query(sql)
            .bind(config.start_time.naive_utc())
            .bind(config.end_time.naive_utc())
            .bind(&config.interval)
            .bind(&config.symbols)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("market_data query failed: {}", e))?;

        let mut events: Vec<ReplayEvent> = rows
            .into_iter()
            .map(|r| {
                let symbol: String = r.get(0);
                let interval: String = r.get(1);
                let open_time_naive: chrono::NaiveDateTime = r.get(2);
                let open_time = DateTime::<Utc>::from_naive_utc_and_offset(open_time_naive, Utc);
                let open: f64 = r.get(3);
                let high: f64 = r.get(4);
                let low: f64 = r.get(5);
                let close: f64 = r.get(6);
                let volume: f64 = r.get(7);
                let quote_volume: f64 = r.get(8);
                ReplayEvent::Kline {
                    kline: Kline {
                        symbol,
                        interval,
                        open_time,
                        open,
                        high,
                        low,
                        close,
                        volume,
                        quote_volume: Some(quote_volume),
                    },
                }
            })
            .collect();

        // 2) Signals (optional, only if job_id is provided).
        if let Some(jid) = job_id {
            let signal_rows = sqlx::query(
                r#"SELECT signal_id, strategy_id, agent_id, asset, exchange, timeframe,
                          event_time, valid_until, direction, signal_strength, confidence,
                          expected_return_bps, expected_holding_period_sec, market_regime,
                          features_used, risk_flags, explanation
                   FROM alpha_signals
                   WHERE job_id = $1 AND event_time >= $2 AND event_time <= $3
                   ORDER BY event_time ASC"#,
            )
            .bind(jid)
            .bind(config.start_time.naive_utc())
            .bind(config.end_time.naive_utc())
            .fetch_all(pool)
            .await
            .map_err(|e| format!("alpha_signals query failed: {}", e))?;

            for r in signal_rows {
                let signal_id: uuid::Uuid = r.get(0);
                let strategy_id: Option<String> = r.get(1);
                let agent_id: Option<String> = r.get(2);
                let asset: String = r.get(3);
                let exchange: Option<String> = r.get(4);
                let timeframe: Option<String> = r.get(5);
                let event_time_naive: chrono::NaiveDateTime = r.get(6);
                let event_time = DateTime::<Utc>::from_naive_utc_and_offset(event_time_naive, Utc);
                let valid_until: Option<chrono::NaiveDateTime> = r.get(7);
                let direction: String = r.get(8);
                let signal_strength: Option<f64> = r.get(9);
                let confidence: Option<f64> = r.get(10);
                let expected_return_bps: Option<f64> = r.get(11);
                let expected_holding_period_sec: Option<i64> = r.get(12);
                let market_regime: Option<String> = r.get(13);
                let features_used: Option<serde_json::Value> = r.get(14);
                let risk_flags: Option<serde_json::Value> = r.get(15);
                let explanation: Option<String> = r.get(16);

                events.push(ReplayEvent::Signal {
                    signal: AlphaSignal {
                        signal_id,
                        job_id: Some(jid),
                        strategy_id,
                        agent_id,
                        asset,
                        exchange,
                        timeframe,
                        event_time,
                        valid_until: valid_until
                            .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc)),
                        direction,
                        signal_strength,
                        confidence,
                        expected_return_bps,
                        expected_holding_period_sec,
                        market_regime,
                        features_used,
                        risk_flags,
                        explanation,
                    },
                });
            }
        }

        // 3) Sort by timestamp.
        events.sort_by(|a, b| a.timestamp().cmp(&b.timestamp()));

        Ok(Self { config, events, cursor: 0 })
    }

    pub fn total_events(&self) -> usize {
        self.events.len()
    }

    pub fn next(&mut self) -> Option<ReplayEvent> {
        if self.cursor >= self.events.len() {
            return None;
        }
        let ev = self.events[self.cursor].clone();
        self.cursor += 1;
        Some(ev)
    }

    pub fn progress(&self) -> f64 {
        if self.events.is_empty() {
            return 0.0;
        }
        self.cursor as f64 / self.events.len() as f64
    }

    pub fn config(&self) -> &ReplayConfig {
        &self.config
    }
}

/// Generate synthetic signals based on simple RSI-like divergence.
/// Useful for testing when no real signals are provided.
pub fn generate_synthetic_signals(
    job_id: uuid::Uuid,
    symbols: &[String],
    klines: &[Kline],
    now: DateTime<Utc>,
) -> Vec<AlphaSignal> {
    let mut out = Vec::new();
    for symbol in symbols {
        let prices: Vec<f64> = klines
            .iter()
            .filter(|k| &k.symbol == symbol)
            .map(|k| k.close)
            .collect();
        if prices.len() < 15 {
            continue;
        }
        // Simple momentum: recent 3-bar return vs long-term trend.
        let last = prices[prices.len() - 1];
        let prev3 = prices[prices.len() - 4];
        let ret3 = (last - prev3) / prev3;
        let prev20 = prices[prices.len() - 21];
        let ret20 = (last - prev20) / prev20;

        let (direction, strength) = if ret3 > 0.01 && ret20 > 0.0 {
            ("long".into(), 0.7_f64)
        } else if ret3 < -0.01 && ret20 < 0.0 {
            ("short".into(), 0.7_f64)
        } else {
            continue;
        };

        out.push(AlphaSignal {
            signal_id: uuid::Uuid::new_v4(),
            job_id: Some(job_id),
            strategy_id: Some("synthetic_momentum".into()),
            agent_id: Some("synthetic_agent".into()),
            asset: symbol.clone(),
            exchange: Some("binance".into()),
            timeframe: Some("1h".into()),
            event_time: now,
            valid_until: Some(now + Duration::hours(4)),
            direction,
            signal_strength: Some(strength),
            confidence: Some(0.6),
            expected_return_bps: Some(20.0),
            expected_holding_period_sec: Some(3600 * 4),
            market_regime: None,
            features_used: None,
            risk_flags: None,
            explanation: Some("synthetic signal".into()),
        });
    }
    out
}
