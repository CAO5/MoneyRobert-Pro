//! Feature Store
//! 特征存储模块
//!
//! 提供特征值的读写接口，支持：
//! - 按 symbol + 时间范围查询特征值
//! - 批量写入特征值（用于特征计算管线）
//! - 查询特征定义
//! - 特征血缘记录与查询（feature_lineage）
//! - 数据质量快照记录与查询（data_quality_snapshots）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::regime::RegimeSnapshot;

/// 特征值记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureValue {
    pub feature_id: Uuid,
    pub feature_name: String,
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub metadata: Option<serde_json::Value>,
}

/// 特征定义记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDefinition {
    pub feature_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub version: String,
    pub parameters: serde_json::Value,
    pub unit: Option<String>,
    pub is_active: bool,
}

/// 特征血缘记录
/// 记录每个特征值的数据源、计算版本、参数 hash，确保可追溯可复现
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureLineage {
    pub lineage_id: i64,
    pub feature_id: Uuid,
    pub symbol: String,
    pub timestamp: DateTime<Utc>,
    pub data_source: String,
    pub source_time_start: Option<DateTime<Utc>>,
    pub source_time_end: Option<DateTime<Utc>>,
    pub calc_version: String,
    pub parameters_hash: String,
    pub parameters: serde_json::Value,
    pub upstream_feature_ids: Vec<Uuid>,
    pub raw_data_refs: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// 数据质量快照
/// 记录数据新鲜度、缺口率、异常值率、覆盖率
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQualitySnapshot {
    pub snapshot_id: i64,
    pub symbol: String,
    pub data_source: String,
    pub snapshot_time: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub freshness_sec: Option<f64>,
    pub gap_count: i32,
    pub gap_ratio: f64,
    pub outlier_count: i32,
    pub outlier_ratio: f64,
    pub expected_points: i32,
    pub actual_points: i32,
    pub coverage_ratio: f64,
    pub backfill_status: String,
    pub last_backfill_time: Option<DateTime<Utc>>,
    pub quality_grade: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// 数据质量等级
/// 依据覆盖率、缺口率、异常值率综合评定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityGrade {
    Excellent,
    Good,
    Fair,
    Poor,
    Unknown,
}

impl QualityGrade {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Excellent => "excellent",
            Self::Good => "good",
            Self::Fair => "fair",
            Self::Poor => "poor",
            Self::Unknown => "unknown",
        }
    }

    /// 根据覆盖率、缺口率、异常值率综合评定质量等级
    pub fn from_metrics(coverage: f64, gap_ratio: f64, outlier_ratio: f64) -> Self {
        // 覆盖率 >= 99% 且缺口率 < 1% 且异常值率 < 1%
        if coverage >= 0.99 && gap_ratio < 0.01 && outlier_ratio < 0.01 {
            Self::Excellent
        // 覆盖率 >= 95% 且缺口率 < 5% 且异常值率 < 3%
        } else if coverage >= 0.95 && gap_ratio < 0.05 && outlier_ratio < 0.03 {
            Self::Good
        // 覆盖率 >= 80% 且缺口率 < 20% 且异常值率 < 5%
        } else if coverage >= 0.80 && gap_ratio < 0.20 && outlier_ratio < 0.05 {
            Self::Fair
        // 其他情况
        } else {
            Self::Poor
        }
    }
}

/// 特征存储
pub struct FeatureStore;

impl FeatureStore {
    /// 查询所有特征定义
    pub async fn list_definitions(pool: &PgPool) -> Result<Vec<FeatureDefinition>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT feature_id, name, description, category, version, parameters, unit, is_active
               FROM feature_definitions
               WHERE is_active = true
               ORDER BY category, name"#,
        )
        .fetch_all(pool)
        .await?;

        let defs = rows
            .into_iter()
            .map(|row| FeatureDefinition {
                feature_id: row.get("feature_id"),
                name: row.get("name"),
                description: row.get("description"),
                category: row.get("category"),
                version: row.get("version"),
                parameters: row.get("parameters"),
                unit: row.get("unit"),
                is_active: row.get("is_active"),
            })
            .collect();
        Ok(defs)
    }

    /// 按名称查询特征定义
    pub async fn get_definition_by_name(
        pool: &PgPool,
        name: &str,
    ) -> Result<Option<FeatureDefinition>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT feature_id, name, description, category, version, parameters, unit, is_active
               FROM feature_definitions
               WHERE name = $1 AND is_active = true"#,
        )
        .bind(name)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|row| FeatureDefinition {
            feature_id: row.get("feature_id"),
            name: row.get("name"),
            description: row.get("description"),
            category: row.get("category"),
            version: row.get("version"),
            parameters: row.get("parameters"),
            unit: row.get("unit"),
            is_active: row.get("is_active"),
        }))
    }

    /// 写入单个特征值（upsert）
    pub async fn upsert_feature_value(
        pool: &PgPool,
        feature_id: Uuid,
        symbol: &str,
        timestamp: DateTime<Utc>,
        value: f64,
        metadata: Option<&serde_json::Value>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO feature_values (feature_id, symbol, timestamp, value, metadata)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (feature_id, symbol, timestamp)
               DO UPDATE SET value = EXCLUDED.value, metadata = EXCLUDED.metadata"#,
        )
        .bind(feature_id)
        .bind(symbol)
        .bind(timestamp)
        .bind(value)
        .bind(metadata)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 批量写入特征值（upsert）
    pub async fn batch_upsert_feature_values(
        pool: &PgPool,
        values: &[FeatureValue],
    ) -> Result<u64, sqlx::Error> {
        let mut affected = 0u64;
        for v in values {
            Self::upsert_feature_value(
                pool,
                v.feature_id,
                &v.symbol,
                v.timestamp,
                v.value,
                v.metadata.as_ref(),
            )
            .await?;
            affected += 1;
        }
        Ok(affected)
    }

    /// 查询特征值时间序列
    pub async fn query_feature_values(
        pool: &PgPool,
        feature_name: &str,
        symbol: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<FeatureValue>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT fv.feature_id, fd.name as feature_name, fv.symbol, fv.timestamp, fv.value, fv.metadata
               FROM feature_values fv
               JOIN feature_definitions fd ON fv.feature_id = fd.feature_id
               WHERE fd.name = $1 AND fv.symbol = $2
                 AND fv.timestamp >= $3 AND fv.timestamp <= $4
               ORDER BY fv.timestamp ASC"#,
        )
        .bind(feature_name)
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(pool)
        .await?;

        let values = rows
            .into_iter()
            .map(|row| FeatureValue {
                feature_id: row.get("feature_id"),
                feature_name: row.get("feature_name"),
                symbol: row.get("symbol"),
                timestamp: row.get("timestamp"),
                value: row.get("value"),
                metadata: row.get("metadata"),
            })
            .collect();
        Ok(values)
    }

    /// 写入市场状态快照
    pub async fn upsert_regime(
        pool: &PgPool,
        symbol: &str,
        timestamp: DateTime<Utc>,
        snapshot: &RegimeSnapshot,
    ) -> Result<(), sqlx::Error> {
        let metadata = serde_json::json!({
            "adx": snapshot.adx,
            "volatility_percentile": snapshot.volatility_percentile,
            "return_percentile": snapshot.return_percentile,
        });
        sqlx::query(
            r#"INSERT INTO market_regimes
               (symbol, timestamp, regime, confidence, adx, volatility_percentile, return_percentile, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (symbol, timestamp)
               DO UPDATE SET regime = EXCLUDED.regime,
                             confidence = EXCLUDED.confidence,
                             adx = EXCLUDED.adx,
                             volatility_percentile = EXCLUDED.volatility_percentile,
                             return_percentile = EXCLUDED.return_percentile,
                             metadata = EXCLUDED.metadata"#,
        )
        .bind(symbol)
        .bind(timestamp)
        .bind(snapshot.regime.as_str())
        .bind(snapshot.confidence)
        .bind(snapshot.adx)
        .bind(snapshot.volatility_percentile)
        .bind(snapshot.return_percentile)
        .bind(&metadata)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 查询市场状态历史
    pub async fn query_regimes(
        pool: &PgPool,
        symbol: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<RegimeSnapshot>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT regime, confidence, adx, volatility_percentile, return_percentile, timestamp
               FROM market_regimes
               WHERE symbol = $1 AND timestamp >= $2 AND timestamp <= $3
               ORDER BY timestamp ASC"#,
        )
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(pool)
        .await?;

        let snapshots = rows
            .into_iter()
            .map(|row| RegimeSnapshot {
                regime: super::regime::MarketRegime::from_str(row.get::<String, _>("regime").as_str())
                    .unwrap_or(super::regime::MarketRegime::Ranging),
                confidence: row.get("confidence"),
                adx: row.get("adx"),
                volatility_percentile: row.get("volatility_percentile"),
                return_percentile: row.get("return_percentile"),
                timestamp: row.get("timestamp"),
            })
            .collect();
        Ok(snapshots)
    }

    /// 聚合每日 OHLCV（从 K 线表聚合到 ohlcv_daily）
    pub async fn aggregate_daily_ohlcv(
        pool: &PgPool,
        symbol: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"INSERT INTO ohlcv_daily (symbol, date, open, high, low, close, volume, quote_volume, trade_count)
               SELECT
                   symbol,
                   DATE(open_time) as date,
                   (array_agg(open ORDER BY open_time ASC))[1] as open,
                   MAX(high) as high,
                   MIN(low) as low,
                   (array_agg(close ORDER BY open_time DESC))[1] as close,
                   SUM(volume) as volume,
                   SUM(COALESCE(quote_volume, 0)) as quote_volume,
                   COUNT(*) as trade_count
               FROM klines
               WHERE symbol = $1 AND interval = '1H'
               GROUP BY symbol, DATE(open_time)
               ON CONFLICT (symbol, date)
               DO UPDATE SET
                   open = EXCLUDED.open,
                   high = EXCLUDED.high,
                   low = EXCLUDED.low,
                   close = EXCLUDED.close,
                   volume = EXCLUDED.volume,
                   quote_volume = EXCLUDED.quote_volume,
                   trade_count = EXCLUDED.trade_count"#,
        )
        .bind(symbol)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    // =========================================================
    // 特征血缘（feature_lineage）
    // =========================================================

    /// 写入特征血缘记录（upsert）
    /// 每个特征值记录数据源、计算版本、参数 hash，确保可追溯可复现
    pub async fn upsert_feature_lineage(
        pool: &PgPool,
        feature_id: Uuid,
        symbol: &str,
        timestamp: DateTime<Utc>,
        data_source: &str,
        source_time_start: Option<DateTime<Utc>>,
        source_time_end: Option<DateTime<Utc>>,
        calc_version: &str,
        parameters_hash: &str,
        parameters: &serde_json::Value,
        upstream_feature_ids: &[Uuid],
        raw_data_refs: Option<&serde_json::Value>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO feature_lineage
               (feature_id, symbol, timestamp, data_source, source_time_start, source_time_end,
                calc_version, parameters_hash, parameters, upstream_feature_ids, raw_data_refs)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
               ON CONFLICT (feature_id, symbol, timestamp)
               DO UPDATE SET data_source = EXCLUDED.data_source,
                             source_time_start = EXCLUDED.source_time_start,
                             source_time_end = EXCLUDED.source_time_end,
                             calc_version = EXCLUDED.calc_version,
                             parameters_hash = EXCLUDED.parameters_hash,
                             parameters = EXCLUDED.parameters,
                             upstream_feature_ids = EXCLUDED.upstream_feature_ids,
                             raw_data_refs = EXCLUDED.raw_data_refs"#,
        )
        .bind(feature_id)
        .bind(symbol)
        .bind(timestamp)
        .bind(data_source)
        .bind(source_time_start)
        .bind(source_time_end)
        .bind(calc_version)
        .bind(parameters_hash)
        .bind(parameters)
        .bind(upstream_feature_ids)
        .bind(raw_data_refs)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// 查询特征血缘
    pub async fn query_feature_lineage(
        pool: &PgPool,
        feature_id: Uuid,
        symbol: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<FeatureLineage>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT lineage_id, feature_id, symbol, timestamp, data_source,
                      source_time_start, source_time_end, calc_version, parameters_hash,
                      parameters, upstream_feature_ids, raw_data_refs, created_at
               FROM feature_lineage
               WHERE feature_id = $1 AND symbol = $2
                 AND timestamp >= $3 AND timestamp <= $4
               ORDER BY timestamp ASC"#,
        )
        .bind(feature_id)
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(pool)
        .await?;

        let lineages = rows
            .into_iter()
            .map(|row| FeatureLineage {
                lineage_id: row.get("lineage_id"),
                feature_id: row.get("feature_id"),
                symbol: row.get("symbol"),
                timestamp: row.get("timestamp"),
                data_source: row.get("data_source"),
                source_time_start: row.get("source_time_start"),
                source_time_end: row.get("source_time_end"),
                calc_version: row.get("calc_version"),
                parameters_hash: row.get("parameters_hash"),
                parameters: row.get("parameters"),
                upstream_feature_ids: row.get("upstream_feature_ids"),
                raw_data_refs: row.get("raw_data_refs"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok(lineages)
    }

    // =========================================================
    // 数据质量快照（data_quality_snapshots）
    // =========================================================

    /// 写入数据质量快照
    pub async fn upsert_data_quality_snapshot(
        pool: &PgPool,
        symbol: &str,
        data_source: &str,
        snapshot_time: DateTime<Utc>,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
        freshness_sec: Option<f64>,
        gap_count: i32,
        gap_ratio: f64,
        outlier_count: i32,
        outlier_ratio: f64,
        expected_points: i32,
        actual_points: i32,
        coverage_ratio: f64,
        backfill_status: &str,
        quality_grade: &str,
        metadata: Option<&serde_json::Value>,
    ) -> Result<i64, sqlx::Error> {
        let row = sqlx::query(
            r#"INSERT INTO data_quality_snapshots
               (symbol, data_source, snapshot_time, period_start, period_end,
                freshness_sec, gap_count, gap_ratio, outlier_count, outlier_ratio,
                expected_points, actual_points, coverage_ratio,
                backfill_status, quality_grade, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
               RETURNING snapshot_id"#,
        )
        .bind(symbol)
        .bind(data_source)
        .bind(snapshot_time)
        .bind(period_start)
        .bind(period_end)
        .bind(freshness_sec)
        .bind(gap_count)
        .bind(gap_ratio)
        .bind(outlier_count)
        .bind(outlier_ratio)
        .bind(expected_points)
        .bind(actual_points)
        .bind(coverage_ratio)
        .bind(backfill_status)
        .bind(quality_grade)
        .bind(metadata)
        .fetch_one(pool)
        .await?;

        Ok(row.get::<i64, _>("snapshot_id"))
    }

    /// 查询数据质量快照
    pub async fn query_data_quality(
        pool: &PgPool,
        symbol: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<DataQualitySnapshot>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT snapshot_id, symbol, data_source, snapshot_time, period_start, period_end,
                      freshness_sec, gap_count, gap_ratio, outlier_count, outlier_ratio,
                      expected_points, actual_points, coverage_ratio, backfill_status,
                      last_backfill_time, quality_grade, metadata, created_at
               FROM data_quality_snapshots
               WHERE symbol = $1 AND snapshot_time >= $2 AND snapshot_time <= $3
               ORDER BY snapshot_time DESC"#,
        )
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(pool)
        .await?;

        let snapshots = rows
            .into_iter()
            .map(|row| DataQualitySnapshot {
                snapshot_id: row.get("snapshot_id"),
                symbol: row.get("symbol"),
                data_source: row.get("data_source"),
                snapshot_time: row.get("snapshot_time"),
                period_start: row.get("period_start"),
                period_end: row.get("period_end"),
                freshness_sec: row.get("freshness_sec"),
                gap_count: row.get("gap_count"),
                gap_ratio: row.get("gap_ratio"),
                outlier_count: row.get("outlier_count"),
                outlier_ratio: row.get("outlier_ratio"),
                expected_points: row.get("expected_points"),
                actual_points: row.get("actual_points"),
                coverage_ratio: row.get("coverage_ratio"),
                backfill_status: row.get("backfill_status"),
                last_backfill_time: row.get("last_backfill_time"),
                quality_grade: row.get("quality_grade"),
                metadata: row.get("metadata"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok(snapshots)
    }

    /// 查询最新数据质量快照
    pub async fn get_latest_data_quality(
        pool: &PgPool,
        symbol: &str,
    ) -> Result<Option<DataQualitySnapshot>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT snapshot_id, symbol, data_source, snapshot_time, period_start, period_end,
                      freshness_sec, gap_count, gap_ratio, outlier_count, outlier_ratio,
                      expected_points, actual_points, coverage_ratio, backfill_status,
                      last_backfill_time, quality_grade, metadata, created_at
               FROM data_quality_snapshots
               WHERE symbol = $1
               ORDER BY snapshot_time DESC
               LIMIT 1"#,
        )
        .bind(symbol)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|row| DataQualitySnapshot {
            snapshot_id: row.get("snapshot_id"),
            symbol: row.get("symbol"),
            data_source: row.get("data_source"),
            snapshot_time: row.get("snapshot_time"),
            period_start: row.get("period_start"),
            period_end: row.get("period_end"),
            freshness_sec: row.get("freshness_sec"),
            gap_count: row.get("gap_count"),
            gap_ratio: row.get("gap_ratio"),
            outlier_count: row.get("outlier_count"),
            outlier_ratio: row.get("outlier_ratio"),
            expected_points: row.get("expected_points"),
            actual_points: row.get("actual_points"),
            coverage_ratio: row.get("coverage_ratio"),
            backfill_status: row.get("backfill_status"),
            last_backfill_time: row.get("last_backfill_time"),
            quality_grade: row.get("quality_grade"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_value_serialization() {
        let fv = FeatureValue {
            feature_id: Uuid::new_v4(),
            feature_name: "rsi_14".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            timestamp: Utc::now(),
            value: 55.3,
            metadata: Some(serde_json::json!({"period": 14})),
        };
        let json = serde_json::to_string(&fv).unwrap();
        assert!(json.contains("rsi_14"));
        assert!(json.contains("BTC-USDT-SWAP"));
    }

    #[test]
    fn test_feature_definition_serialization() {
        let def = FeatureDefinition {
            feature_id: Uuid::new_v4(),
            name: "macd_signal".to_string(),
            description: Some("MACD signal line".to_string()),
            category: "momentum".to_string(),
            version: "1.0".to_string(),
            parameters: serde_json::json!({"fast": 12, "slow": 26}),
            unit: Some("ratio".to_string()),
            is_active: true,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("macd_signal"));
        assert!(json.contains("momentum"));
    }
}
