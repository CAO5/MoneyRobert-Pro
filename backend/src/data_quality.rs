//! Data Quality Monitor
//! 数据质量监控模块
//!
//! 依据《系统评估与演进规划》系统架构师视角：
//!   "错误大量用 .ok() 吞掉，缺少数据质量告警"
//!   "数据源、特征、模型和预测没有版本血缘"
//!
//! 本模块定期扫描各数据表，计算：
//! - 新鲜度（freshness_sec）：最近一条数据距现在的秒数
//! - 缺口率（gap_ratio）：时间序列中缺失点占比
//! - 覆盖率（coverage_ratio）：实际点数 / 期望点数
//! - 异常值比例（outlier_ratio）：价格跳变超过阈值的点占比
//! - 质量等级（quality_grade）：A/B/C/D 四级
//!
//! 写入 data_quality_snapshots 表，供 API 查询和告警使用。

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

/// 数据质量等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityGrade {
    /// 优秀：覆盖率 >= 99%，新鲜度 < 2 倍间隔
    A,
    /// 良好：覆盖率 >= 95%，新鲜度 < 3 倍间隔
    B,
    /// 警告：覆盖率 >= 80%，新鲜度 < 5 倍间隔
    C,
    /// 异常：覆盖率 < 80% 或新鲜度 >= 5 倍间隔
    D,
}

impl QualityGrade {
    pub fn as_str(&self) -> &'static str {
        match self {
            QualityGrade::A => "A",
            QualityGrade::B => "B",
            QualityGrade::C => "C",
            QualityGrade::D => "D",
        }
    }

    /// 根据覆盖率和新鲜度比率判定等级
    pub fn from_metrics(coverage_ratio: f64, freshness_ratio: f64) -> Self {
        if coverage_ratio >= 0.99 && freshness_ratio < 2.0 {
            QualityGrade::A
        } else if coverage_ratio >= 0.95 && freshness_ratio < 3.0 {
            QualityGrade::B
        } else if coverage_ratio >= 0.80 && freshness_ratio < 5.0 {
            QualityGrade::C
        } else {
            QualityGrade::D
        }
    }
}

/// 单个数据源的质量快照（用于内存聚合，不直接对应表结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
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
    pub quality_grade: String,
}

/// 数据源描述：表名、时间字段、期望采样间隔（秒）
#[derive(Debug, Clone)]
pub struct DataSourceSpec {
    pub name: &'static str,
    pub table: &'static str,
    pub time_column: &'static str,
    pub expected_interval_sec: i64,
    pub value_column: Option<&'static str>,
}

/// 预定义的数据源列表
pub fn default_data_sources() -> Vec<DataSourceSpec> {
    vec![
        DataSourceSpec {
            name: "ticker",
            table: "ticker_history",
            time_column: "created_at",
            expected_interval_sec: 10,
            value_column: Some("last"),
        },
        DataSourceSpec {
            name: "klines_1H",
            table: "klines",
            time_column: "open_time",
            expected_interval_sec: 3600,
            value_column: Some("close"),
        },
        DataSourceSpec {
            name: "funding_rate",
            table: "funding_rate_history",
            time_column: "created_at",
            expected_interval_sec: 300,
            value_column: Some("funding_rate"),
        },
        DataSourceSpec {
            name: "orderbook",
            table: "orderbook_snapshots",
            time_column: "timestamp",
            expected_interval_sec: 5,
            value_column: Some("mid_price"),
        },
        DataSourceSpec {
            name: "trade_ticks",
            table: "trade_ticks",
            time_column: "timestamp",
            expected_interval_sec: 10,
            value_column: Some("price"),
        },
        DataSourceSpec {
            name: "liquidations",
            table: "liquidation_events",
            time_column: "timestamp",
            expected_interval_sec: 60,
            value_column: Some("price"),
        },
        DataSourceSpec {
            name: "basis",
            table: "basis_data",
            time_column: "timestamp",
            expected_interval_sec: 60,
            value_column: Some("perp_basis"),
        },
    ]
}

/// 扫描单个数据源的单个 symbol，计算质量指标
pub async fn scan_data_source(
    pool: &PgPool,
    spec: &DataSourceSpec,
    symbol: &str,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<QualityReport, sqlx::Error> {
    let interval_secs = spec.expected_interval_sec;
    let expected_points = ((period_end - period_start).num_seconds() / interval_secs) as i32;

    // 查询实际数据点数和最新时间
    let row = sqlx::query(&format!(
        r#"SELECT COUNT(*) as cnt,
                  MAX({}) as last_time,
                  MIN({}) as first_time
           FROM {}
           WHERE symbol = $1 AND {} >= $2 AND {} <= $3"#,
        spec.time_column, spec.time_column, spec.table,
        spec.time_column, spec.time_column
    ))
    .bind(symbol)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await?;

    let actual_points: i64 = row.get("cnt");
    let last_time: Option<DateTime<Utc>> = row.get("last_time");
    let first_time: Option<DateTime<Utc>> = row.get("first_time");

    let freshness_sec = last_time.map(|t| (Utc::now() - t).num_seconds().max(0) as f64);

    let coverage_ratio = if expected_points > 0 {
        (actual_points as f64) / (expected_points as f64)
    } else {
        0.0
    };

    // 缺口检测：查询时间序列中的间隔
    let gap_count = detect_gaps(pool, spec, symbol, period_start, period_end, interval_secs).await?;

    let gap_ratio = if actual_points > 0 {
        gap_count as f64 / actual_points as f64
    } else {
        1.0
    };

    // 异常值检测（如果有值列）
    let (outlier_count, outlier_ratio) = if let Some(value_col) = spec.value_column {
        detect_outliers(pool, spec, value_col, symbol, period_start, period_end).await?
    } else {
        (0, 0.0)
    };

    let freshness_ratio = if let Some(fs) = freshness_sec {
        fs / interval_secs as f64
    } else {
        999.0
    };

    let grade = QualityGrade::from_metrics(coverage_ratio.min(1.0), freshness_ratio);

    Ok(QualityReport {
        symbol: symbol.to_string(),
        data_source: spec.name.to_string(),
        snapshot_time: Utc::now(),
        period_start,
        period_end,
        freshness_sec,
        gap_count,
        gap_ratio,
        outlier_count,
        outlier_ratio,
        expected_points,
        actual_points: actual_points as i32,
        coverage_ratio: coverage_ratio.min(1.0),
        quality_grade: grade.as_str().to_string(),
    })
}

/// 检测时间序列缺口：相邻数据点间隔超过 2 倍期望间隔的点数
async fn detect_gaps(
    pool: &PgPool,
    spec: &DataSourceSpec,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    interval_secs: i64,
) -> Result<i32, sqlx::Error> {
    let threshold = interval_secs * 2;

    let row = sqlx::query(&format!(
        r#"WITH ordered AS (
            SELECT {} as ts
            FROM {}
            WHERE symbol = $1 AND {} >= $2 AND {} <= $3
            ORDER BY {} ASC
           ),
           diffs AS (
            SELECT ts,
                   ts - LAG(ts) OVER (ORDER BY ts) as diff
            FROM ordered
           )
           SELECT COUNT(*) as gap_count
           FROM diffs
           WHERE diff IS NOT NULL AND EXTRACT(EPOCH FROM diff) > $4"#,
        spec.time_column, spec.table, spec.time_column, spec.time_column, spec.time_column
    ))
    .bind(symbol)
    .bind(start)
    .bind(end)
    .bind(threshold as f64)
    .fetch_one(pool)
    .await?;

    Ok(row.get::<i64, _>("gap_count") as i32)
}

/// 检测异常值：价格变化超过 5% 的点数
async fn detect_outliers(
    pool: &PgPool,
    spec: &DataSourceSpec,
    value_col: &str,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(i32, f64), sqlx::Error> {
    let row = sqlx::query(&format!(
        r#"WITH ordered AS (
            SELECT {} as val, {} as ts
            FROM {}
            WHERE symbol = $1 AND {} >= $2 AND {} <= $3 AND {} IS NOT NULL AND {} > 0
            ORDER BY {} ASC
           ),
           changes AS (
            SELECT val,
                   val / NULLIF(LAG(val) OVER (ORDER BY ts), 0) - 1.0 as pct_change
            FROM ordered
           )
           SELECT COUNT(*) as total,
                  COUNT(CASE WHEN ABS(pct_change) > 0.05 THEN 1 END) as outlier_count
           FROM changes
           WHERE pct_change IS NOT NULL"#,
        value_col, spec.time_column, spec.table,
        spec.time_column, spec.time_column, value_col, value_col, spec.time_column
    ))
    .bind(symbol)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await?;

    let total: i64 = row.get("total");
    let outlier_count: i64 = row.get("outlier_count");

    let ratio = if total > 0 {
        outlier_count as f64 / total as f64
    } else {
        0.0
    };

    Ok((outlier_count as i32, ratio))
}

/// 扫描所有数据源的所有 symbol，生成质量快照并写入数据库
pub async fn run_quality_scan(
    pool: &PgPool,
    symbols: &[String],
    lookback_hours: i64,
) -> Result<Vec<QualityReport>, sqlx::Error> {
    let sources = default_data_sources();
    let period_end = Utc::now();
    let period_start = period_end - Duration::hours(lookback_hours);
    let mut reports = Vec::new();

    for source in &sources {
        for symbol in symbols {
            let report = scan_data_source(pool, source, symbol, period_start, period_end).await?;

            // 写入 data_quality_snapshots 表
            let _ = crate::features::store::FeatureStore::upsert_data_quality_snapshot(
                pool,
                &report.symbol,
                &report.data_source,
                report.snapshot_time,
                report.period_start,
                report.period_end,
                report.freshness_sec,
                report.gap_count,
                report.gap_ratio,
                report.outlier_count,
                report.outlier_ratio,
                report.expected_points,
                report.actual_points,
                report.coverage_ratio,
                "none",
                &report.quality_grade,
                None,
            )
            .await;

            reports.push(report);
        }
    }

    Ok(reports)
}

/// 获取所有 symbol 的最新质量概览
pub async fn get_quality_overview(
    pool: &PgPool,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT ON (symbol, data_source)
                  symbol, data_source, snapshot_time, freshness_sec,
                  gap_ratio, outlier_ratio, coverage_ratio, quality_grade,
                  expected_points, actual_points
           FROM data_quality_snapshots
           ORDER BY symbol, data_source, snapshot_time DESC"#,
    )
    .fetch_all(pool)
    .await?;

    let overview: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "symbol": row.get::<String, _>("symbol"),
                "data_source": row.get::<String, _>("data_source"),
                "snapshot_time": row.get::<DateTime<Utc>, _>("snapshot_time").to_rfc3339(),
                "freshness_sec": row.get::<Option<f64>, _>("freshness_sec"),
                "gap_ratio": row.get::<f64, _>("gap_ratio"),
                "outlier_ratio": row.get::<f64, _>("outlier_ratio"),
                "coverage_ratio": row.get::<f64, _>("coverage_ratio"),
                "quality_grade": row.get::<String, _>("quality_grade"),
                "expected_points": row.get::<i32, _>("expected_points"),
                "actual_points": row.get::<i32, _>("actual_points"),
            })
        })
        .collect();

    Ok(overview)
}

/// 获取质量等级为 D（异常）的数据源列表，用于告警
pub async fn get_critical_data_sources(
    pool: &PgPool,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT ON (symbol, data_source)
                  symbol, data_source, snapshot_time, freshness_sec,
                  gap_ratio, coverage_ratio, quality_grade
           FROM data_quality_snapshots
           WHERE quality_grade IN ('C', 'D')
           ORDER BY symbol, data_source, snapshot_time DESC"#,
    )
    .fetch_all(pool)
    .await?;

    let alerts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "symbol": row.get::<String, _>("symbol"),
                "data_source": row.get::<String, _>("data_source"),
                "snapshot_time": row.get::<DateTime<Utc>, _>("snapshot_time").to_rfc3339(),
                "freshness_sec": row.get::<Option<f64>, _>("freshness_sec"),
                "gap_ratio": row.get::<f64, _>("gap_ratio"),
                "coverage_ratio": row.get::<f64, _>("coverage_ratio"),
                "quality_grade": row.get::<String, _>("quality_grade"),
                "severity": if row.get::<String, _>("quality_grade") == "D" { "critical" } else { "warning" },
            })
        })
        .collect();

    Ok(alerts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_grade_from_metrics() {
        // 优秀
        assert_eq!(QualityGrade::from_metrics(1.0, 1.0), QualityGrade::A);
        assert_eq!(QualityGrade::from_metrics(0.99, 1.5), QualityGrade::A);

        // 良好
        assert_eq!(QualityGrade::from_metrics(0.96, 2.5), QualityGrade::B);
        assert_eq!(QualityGrade::from_metrics(0.95, 2.0), QualityGrade::B);

        // 警告
        assert_eq!(QualityGrade::from_metrics(0.85, 4.0), QualityGrade::C);
        assert_eq!(QualityGrade::from_metrics(0.80, 3.0), QualityGrade::C);

        // 异常
        assert_eq!(QualityGrade::from_metrics(0.50, 1.0), QualityGrade::D);
        assert_eq!(QualityGrade::from_metrics(0.99, 6.0), QualityGrade::D);
        assert_eq!(QualityGrade::from_metrics(0.70, 10.0), QualityGrade::D);
    }

    #[test]
    fn test_quality_grade_as_str() {
        assert_eq!(QualityGrade::A.as_str(), "A");
        assert_eq!(QualityGrade::B.as_str(), "B");
        assert_eq!(QualityGrade::C.as_str(), "C");
        assert_eq!(QualityGrade::D.as_str(), "D");
    }

    #[test]
    fn test_default_data_sources() {
        let sources = default_data_sources();
        assert!(sources.len() >= 7);

        let names: Vec<&str> = sources.iter().map(|s| s.name).collect();
        assert!(names.contains(&"ticker"));
        assert!(names.contains(&"orderbook"));
        assert!(names.contains(&"trade_ticks"));
        assert!(names.contains(&"basis"));
    }

    #[test]
    fn test_data_source_spec_intervals() {
        let sources = default_data_sources();
        for s in &sources {
            assert!(s.expected_interval_sec > 0, "间隔必须为正数: {}", s.name);
            assert!(!s.table.is_empty());
            assert!(!s.time_column.is_empty());
        }
    }
}
