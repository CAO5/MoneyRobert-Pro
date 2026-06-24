//! Historical Data Backfill Service
//! 历史数据回填服务
//!
//! 依据《系统评估与演进规划》第二阶段任务 1：
//! 建立长期历史数据和特征仓库
//!
//! 设计目标：
//! - 使用 OKX /api/v5/market/history-candles 接口获取历史 K 线
//! - 自动检测 klines 表中的数据缺口并回填
//! - 支持指定时间范围的批量回填
//! - 速率限制（OKX 限制：20 次/2 秒）
//! - 断点续传：记录回填进度，支持中断后恢复

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::sleep;

/// OKX history-candles 接口单次最大返回数量
const OKX_HISTORY_LIMIT: usize = 100;

/// 速率限制：每次请求后等待 100ms（约 10 次/秒，远低于 OKX 的 20 次/2 秒限制）
const RATE_LIMIT_DELAY_MS: u64 = 100;

/// 回填支持的 K 线周期及其对应的毫秒间隔
fn bar_to_millis(bar: &str) -> Option<i64> {
    match bar {
        "1m" => Some(60_000),
        "5m" => Some(300_000),
        "15m" => Some(900_000),
        "30m" => Some(1_800_000),
        "1H" => Some(3_600_000),
        "4H" => Some(14_400_000),
        "1D" => Some(86_400_000),
        "1W" => Some(604_800_000),
        _ => None,
    }
}

/// 回填请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillRequest {
    pub symbol: String,
    pub bar: String,
    /// 回填起始时间（ISO 8601 或毫秒时间戳）
    pub from: String,
    /// 回填结束时间（ISO 8601 或毫秒时间戳）
    pub to: String,
}

/// 回填结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillResult {
    pub symbol: String,
    pub bar: String,
    pub fetched: usize,
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub gaps_detected: usize,
    pub gaps_filled: usize,
    pub from_time: String,
    pub to_time: String,
    pub elapsed_secs: f64,
    pub errors: Vec<String>,
}

impl BackfillResult {
    fn new(symbol: &str, bar: &str, from: &str, to: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            bar: bar.to_string(),
            fetched: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            gaps_detected: 0,
            gaps_filled: 0,
            from_time: from.to_string(),
            to_time: to.to_string(),
            elapsed_secs: 0.0,
            errors: Vec::new(),
        }
    }
}

/// 缺口检测结果
#[derive(Debug, Clone, Serialize)]
pub struct GapInfo {
    pub symbol: String,
    pub bar: String,
    pub gap_start: String,
    pub gap_end: String,
    pub missing_count: usize,
}

/// 历史数据回填器
pub struct HistoryBackfiller {
    db: PgPool,
    http_client: reqwest::Client,
}

impl HistoryBackfiller {
    pub fn new(db: PgPool) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client for backfiller");
        Self { db, http_client }
    }

    /// 解析时间字符串为毫秒时间戳
    pub fn parse_timestamp(s: &str) -> Result<i64, String> {
        // 尝试解析为毫秒时间戳
        if let Ok(ts) = s.parse::<i64>() {
            return Ok(ts);
        }
        // 尝试解析为 ISO 8601
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(dt.timestamp_millis());
        }
        // 尝试解析为 "YYYY-MM-DD HH:MM:SS"
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            return Ok(dt.and_utc().timestamp_millis());
        }
        // 尝试解析为 "YYYY-MM-DD"
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            let dt = date.and_hms_opt(0, 0, 0).unwrap();
            return Ok(dt.and_utc().timestamp_millis());
        }
        Err(format!("Cannot parse timestamp: {}", s))
    }

    /// 从 OKX history-candles 接口获取历史 K 线
    ///
    /// OKX 接口说明：
    /// - 端点：/api/v5/market/history-candles
    /// - 参数：instId, bar, before（此时间戳之前）, after（此时间戳之后）, limit（最大100）
    /// - 返回：按时间倒序排列（最新在前）
    async fn fetch_history_candles(
        &self,
        symbol: &str,
        bar: &str,
        before: Option<i64>,
        after: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Vec<String>>, String> {
        let limit = limit.min(OKX_HISTORY_LIMIT);
        let mut url = format!(
            "https://www.okx.com/api/v5/market/history-candles?instId={}&bar={}&limit={}",
            symbol, bar, limit
        );
        if let Some(b) = before {
            url.push_str(&format!("&before={}", b));
        }
        if let Some(a) = after {
            url.push_str(&format!("&after={}", a));
        }

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse failed: {}", e))?;

        let code = body
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");
        if code != "0" {
            let msg = body
                .get("msg")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(format!("OKX API error: code={}, msg={}", code, msg));
        }

        let data = body
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or("No data array in response")?;

        // 将 JSON 数组转换为 Vec<Vec<String>>
        let result: Vec<Vec<String>> = data
            .iter()
            .filter_map(|candle| {
                candle.as_array().map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
            })
            .collect();

        Ok(result)
    }

    /// 将 OKX 返回的 K 线数据写入 klines 表
    async fn upsert_klines(
        &self,
        symbol: &str,
        db_interval: &str,
        candles: &[Vec<String>],
    ) -> Result<(usize, usize, usize), String> {
        let mut inserted = 0;
        let mut updated = 0;
        let mut skipped = 0;

        for candle in candles {
            if candle.len() < 7 {
                skipped += 1;
                continue;
            }

            let ts_ms: i64 = candle[0].parse().unwrap_or(0);
            if ts_ms == 0 {
                skipped += 1;
                continue;
            }

            let open_time = NaiveDateTime::from_timestamp_opt(ts_ms / 1000, 0)
                .unwrap_or_default();

            let open: f64 = candle[1].parse().unwrap_or(0.0);
            let high: f64 = candle[2].parse().unwrap_or(0.0);
            let low: f64 = candle[3].parse().unwrap_or(0.0);
            let close: f64 = candle[4].parse().unwrap_or(0.0);
            let volume: f64 = candle[5].parse().unwrap_or(0.0);
            let vol_ccy: f64 = if candle.len() > 6 {
                candle[6].parse().unwrap_or(0.0)
            } else {
                0.0
            };
            let is_closed = if candle.len() > 8 {
                candle[8] == "1"
            } else {
                true
            };

            let result = sqlx::query(
                r#"INSERT INTO klines (symbol, "interval", open_time, open, high, low, close, volume, quote_volume, is_closed)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (symbol, "interval", open_time) DO UPDATE SET
                    high = EXCLUDED.high, low = EXCLUDED.low, close = EXCLUDED.close,
                    volume = EXCLUDED.volume, quote_volume = EXCLUDED.quote_volume,
                    is_closed = EXCLUDED.is_closed, updated_at = NOW()
                RETURNING (xmax = 0) AS inserted"#,
            )
            .bind(symbol)
            .bind(db_interval)
            .bind(open_time)
            .bind(open)
            .bind(high)
            .bind(low)
            .bind(close)
            .bind(volume)
            .bind(vol_ccy)
            .bind(is_closed)
            .fetch_one(&self.db)
            .await;

            match result {
                Ok(row) => {
                    let is_insert: bool = sqlx::Row::get(&row, "inserted");
                    if is_insert {
                        inserted += 1;
                    } else {
                        updated += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to upsert kline for {} {} {}: {}",
                        symbol,
                        db_interval,
                        open_time,
                        e
                    );
                    skipped += 1;
                }
            }
        }

        Ok((inserted, updated, skipped))
    }

    /// 检测指定 symbol + interval 的数据缺口
    ///
    /// 返回缺失的时间段列表
    pub async fn detect_gaps(
        &self,
        symbol: &str,
        bar: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<GapInfo>, String> {
        let interval_ms = bar_to_millis(bar)
            .ok_or_else(|| format!("Unsupported bar: {}", bar))?;

        // 查询已有数据的时间范围
        let rows = sqlx::query(
            r#"SELECT open_time FROM klines
               WHERE symbol = $1 AND "interval" = $2
                 AND open_time >= $3 AND open_time <= $4
               ORDER BY open_time ASC"#,
        )
        .bind(symbol)
        .bind(bar)
        .bind(NaiveDateTime::from_timestamp_opt(from_ms / 1000, 0).unwrap_or_default())
        .bind(NaiveDateTime::from_timestamp_opt(to_ms / 1000, 0).unwrap_or_default())
        .fetch_all(&self.db)
        .await
        .map_err(|e| format!("Query klines failed: {}", e))?;

        if rows.is_empty() {
            // 整个范围都是缺口
            let total_bars = ((to_ms - from_ms) / interval_ms) as usize;
            return Ok(vec![GapInfo {
                symbol: symbol.to_string(),
                bar: bar.to_string(),
                gap_start: from_ms.to_string(),
                gap_end: to_ms.to_string(),
                missing_count: total_bars,
            }]);
        }

        let mut gaps = Vec::new();
        let mut prev_time = from_ms;

        for row in &rows {
            let open_time: NaiveDateTime = sqlx::Row::get(row, "open_time");
            let current_ms = open_time.and_utc().timestamp_millis();

            if current_ms - prev_time > interval_ms {
                // 发现缺口
                let missing = ((current_ms - prev_time) / interval_ms - 1) as usize;
                if missing > 0 {
                    gaps.push(GapInfo {
                        symbol: symbol.to_string(),
                        bar: bar.to_string(),
                        gap_start: prev_time.to_string(),
                        gap_end: current_ms.to_string(),
                        missing_count: missing,
                    });
                }
            }
            prev_time = current_ms + interval_ms;
        }

        // 检查最后一段
        if to_ms - prev_time > interval_ms {
            let missing = ((to_ms - prev_time) / interval_ms) as usize;
            if missing > 0 {
                gaps.push(GapInfo {
                    symbol: symbol.to_string(),
                    bar: bar.to_string(),
                    gap_start: prev_time.to_string(),
                    gap_end: to_ms.to_string(),
                    missing_count: missing,
                });
            }
        }

        Ok(gaps)
    }

    /// 执行回填
    ///
    /// 从 `from` 到 `to` 时间范围，分页获取历史 K 线并写入数据库
    pub async fn backfill(&self, req: &BackfillRequest) -> Result<BackfillResult, String> {
        let start = std::time::Instant::now();
        let mut result = BackfillResult::new(&req.symbol, &req.bar, &req.from, &req.to);

        let from_ms = Self::parse_timestamp(&req.from)?;
        let to_ms = Self::parse_timestamp(&req.to)?;
        let interval_ms = bar_to_millis(&req.bar)
            .ok_or_else(|| format!("Unsupported bar: {}", req.bar))?;

        if from_ms >= to_ms {
            return Err("from must be earlier than to".to_string());
        }

        // 先检测缺口
        let gaps = self.detect_gaps(&req.symbol, &req.bar, from_ms, to_ms).await?;
        result.gaps_detected = gaps.iter().map(|g| g.missing_count).sum();

        tracing::info!(
            "Backfilling {} {}: {} gaps detected, {} total missing bars",
            req.symbol,
            req.bar,
            gaps.len(),
            result.gaps_detected
        );

        // OKX history-candles 返回按时间倒序（最新在前）
        // 使用 before 参数从最新时间向前翻页
        let mut before = to_ms;
        let after = from_ms;

        loop {
            let candles = match self
                .fetch_history_candles(&req.symbol, &req.bar, Some(before), Some(after), OKX_HISTORY_LIMIT)
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    result.errors.push(format!("Fetch error at before={}: {}", before, e));
                    break;
                }
            };

            if candles.is_empty() {
                break;
            }

            result.fetched += candles.len();

            // 获取本批次最早的时间戳（candles 按倒序，最后一个是最早的）
            let oldest_ts: i64 = candles.last()
                .and_then(|c| c.get(0).and_then(|s| s.parse().ok()))
                .unwrap_or(0);

            // 写入数据库
            let (ins, upd, skip) = self
                .upsert_klines(&req.symbol, &req.bar, &candles)
                .await?;
            result.inserted += ins;
            result.updated += upd;
            result.skipped += skip;

            // 如果返回的数据少于 limit，说明已经到头了
            if candles.len() < OKX_HISTORY_LIMIT {
                break;
            }

            // 更新 before 为本批次最早的时间戳，继续向前翻页
            if oldest_ts <= after {
                break;
            }
            before = oldest_ts;

            // 速率限制
            sleep(Duration::from_millis(RATE_LIMIT_DELAY_MS)).await;
        }

        // 重新检测缺口，计算实际填充的数量
        let remaining_gaps = self.detect_gaps(&req.symbol, &req.bar, from_ms, to_ms).await?;
        let remaining_missing: usize = remaining_gaps.iter().map(|g| g.missing_count).sum();
        result.gaps_filled = result.gaps_detected.saturating_sub(remaining_missing);

        result.elapsed_secs = start.elapsed().as_secs_f64();

        tracing::info!(
            "Backfill complete: {} {} fetched={} inserted={} updated={} gaps_filled={}/{} in {:.1}s",
            req.symbol,
            req.bar,
            result.fetched,
            result.inserted,
            result.updated,
            result.gaps_filled,
            result.gaps_detected,
            result.elapsed_secs
        );

        Ok(result)
    }

    /// 批量回填多个 symbol + interval
    pub async fn backfill_batch(
        &self,
        symbols: &[&str],
        bars: &[&str],
        from: &str,
        to: &str,
    ) -> Vec<BackfillResult> {
        let mut results = Vec::new();

        for symbol in symbols {
            for bar in bars {
                let req = BackfillRequest {
                    symbol: symbol.to_string(),
                    bar: bar.to_string(),
                    from: from.to_string(),
                    to: to.to_string(),
                };

                match self.backfill(&req).await {
                    Ok(r) => results.push(r),
                    Err(e) => {
                        let mut r = BackfillResult::new(symbol, bar, from, to);
                        r.errors.push(e);
                        results.push(r);
                    }
                }
            }
        }

        results
    }

    /// 获取指定 symbol + interval 的数据覆盖范围
    pub async fn get_data_coverage(
        &self,
        symbol: &str,
        bar: &str,
    ) -> Result<serde_json::Value, String> {
        let row = sqlx::query(
            r#"SELECT
                MIN(open_time) as earliest,
                MAX(open_time) as latest,
                COUNT(*) as total_count
               FROM klines
               WHERE symbol = $1 AND "interval" = $2"#,
        )
        .bind(symbol)
        .bind(bar)
        .fetch_one(&self.db)
        .await
        .map_err(|e| format!("Query coverage failed: {}", e))?;

        let earliest: Option<NaiveDateTime> = sqlx::Row::get(&row, "earliest");
        let latest: Option<NaiveDateTime> = sqlx::Row::get(&row, "latest");
        let total: i64 = sqlx::Row::get(&row, "total_count");

        Ok(serde_json::json!({
            "symbol": symbol,
            "bar": bar,
            "earliest": earliest.map(|d| d.to_string()),
            "latest": latest.map(|d| d.to_string()),
            "total_count": total,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_to_millis() {
        assert_eq!(bar_to_millis("1m"), Some(60_000));
        assert_eq!(bar_to_millis("5m"), Some(300_000));
        assert_eq!(bar_to_millis("15m"), Some(900_000));
        assert_eq!(bar_to_millis("30m"), Some(1_800_000));
        assert_eq!(bar_to_millis("1H"), Some(3_600_000));
        assert_eq!(bar_to_millis("4H"), Some(14_400_000));
        assert_eq!(bar_to_millis("1D"), Some(86_400_000));
        assert_eq!(bar_to_millis("1W"), Some(604_800_000));
        assert_eq!(bar_to_millis("2m"), None);
        assert_eq!(bar_to_millis("invalid"), None);
    }

    #[test]
    fn test_parse_timestamp_millis() {
        let ts = "1700000000000";
        let result = HistoryBackfiller::parse_timestamp(ts).unwrap();
        assert_eq!(result, 1700000000000);
    }

    #[test]
    fn test_parse_timestamp_rfc3339() {
        let ts = "2023-11-14T22:13:20Z";
        let result = HistoryBackfiller::parse_timestamp(ts).unwrap();
        assert_eq!(result, 1700000000000);
    }

    #[test]
    fn test_parse_timestamp_date() {
        let ts = "2023-11-15";
        let result = HistoryBackfiller::parse_timestamp(ts).unwrap();
        // 2023-11-15 00:00:00 UTC
        assert_eq!(result, 1700006400000);
    }

    #[test]
    fn test_parse_timestamp_invalid() {
        let ts = "invalid";
        assert!(HistoryBackfiller::parse_timestamp(ts).is_err());
    }

    #[test]
    fn test_backfill_result_new() {
        let result = BackfillResult::new("BTC-USDT-SWAP", "1H", "2023-01-01", "2023-12-31");
        assert_eq!(result.symbol, "BTC-USDT-SWAP");
        assert_eq!(result.bar, "1H");
        assert_eq!(result.fetched, 0);
        assert_eq!(result.inserted, 0);
        assert_eq!(result.errors.len(), 0);
    }

    #[test]
    fn test_backfill_request_serialization() {
        let req = BackfillRequest {
            symbol: "BTC-USDT-SWAP".to_string(),
            bar: "1H".to_string(),
            from: "2023-01-01".to_string(),
            to: "2023-12-31".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: BackfillRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, "BTC-USDT-SWAP");
        assert_eq!(parsed.bar, "1H");
    }

    #[test]
    fn test_gap_info_serialization() {
        let gap = GapInfo {
            symbol: "BTC-USDT-SWAP".to_string(),
            bar: "1H".to_string(),
            gap_start: "1700000000000".to_string(),
            gap_end: "1700003600000".to_string(),
            missing_count: 1,
        };
        let json = serde_json::to_string(&gap).unwrap();
        assert!(json.contains("BTC-USDT-SWAP"));
        assert!(json.contains("missing_count"));
    }
}
