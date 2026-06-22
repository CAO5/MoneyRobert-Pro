//! Feature Store
//! 特征存储模块
//!
//! 提供特征值的读写接口，支持：
//! - 按 symbol + 时间范围查询特征值
//! - 批量写入特征值（用于特征计算管线）
//! - 查询特征定义

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
