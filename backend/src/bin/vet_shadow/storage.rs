use anyhow::Result;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use super::{
    now,
    study::{Fault, Outcomes, Sample},
    VERSION,
};

pub async fn save(
    db: &mut PgConnection,
    run: Uuid,
    asset: &str,
    row: &Sample,
    results: Outcomes,
) -> Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO vet_shadow_samples
         (run_id,asset,factor_version,exchange_ts,available_at,a,l_depth,vet,bid_depth,ask_depth,total_depth,
          mid_price,past_1m_rv,past_5m_rv,past_30m_rv,data_source,quality_status,diagnostics)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'OKX_PUBLIC_BOOKS_TRADES',$16,$17)"
    ).bind(run).bind(asset).bind(VERSION).bind(row.stamp).bind(row.available).bind(row.a).bind(row.l)
     .bind(row.vet).bind(row.depth.map(|d|d[0])).bind(row.depth.map(|d|d[1])).bind(row.depth.map(|d|d[2]))
     .bind(row.mid).bind(row.past[0]).bind(row.past[1]).bind(row.past[2]).bind(row.status)
     .bind(&row.diagnostics).execute(&mut *tx).await?;
    for anchor in results.anchors {
        sqlx::query(
            "UPDATE vet_shadow_samples SET origin_ts=$4, origin_price=$5
                     WHERE run_id=$1 AND asset=$2 AND exchange_ts=$3",
        )
        .bind(run)
        .bind(asset)
        .bind(anchor.stamp)
        .bind(anchor.origin)
        .bind(anchor.price)
        .execute(&mut *tx)
        .await?;
    }
    for label in results.labels {
        sqlx::query(
            "INSERT INTO vet_shadow_labels
             (run_id,asset,exchange_ts,minutes,target_ts,completed_at,realized_variance,realized_volatility,status)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        ).bind(run).bind(asset).bind(label.stamp).bind(label.minutes).bind(label.target).bind(now())
         .bind(label.variance).bind(label.variance.map(f64::sqrt)).bind(label.status).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn fault(db: &mut PgConnection, run: Uuid, asset: &str, event: Fault) -> Result<()> {
    let mut tx = db.begin().await?;
    let through = event.end + if event.book { 1_800_000 } else { 660_000 };
    sqlx::query(
        "UPDATE vet_shadow_samples SET quality_status='INPUT_REVOKED',
           diagnostics=diagnostics || jsonb_build_object('revision_reason',$5::text,'revision_at',$6::bigint)
         WHERE run_id=$1 AND asset=$2 AND exchange_ts>$3 AND exchange_ts<=$4 AND vet IS NOT NULL"
    ).bind(run).bind(asset).bind(event.start).bind(through).bind(event.reason).bind(now())
     .execute(&mut *tx).await?;
    if event.book {
        sqlx::query(
            "UPDATE vet_shadow_labels l SET status='QUOTE_REVOKED', realized_variance=NULL, realized_volatility=NULL
             FROM vet_shadow_samples s
             WHERE l.run_id=$1 AND l.asset=$2 AND l.target_ts>$3 AND l.target_ts<$4+900000
               AND l.status='COMPLETE' AND s.run_id=l.run_id AND s.asset=l.asset AND s.exchange_ts=l.exchange_ts
               AND s.origin_ts>$3-900000 AND s.origin_ts<=$4"
        ).bind(run).bind(asset).bind(event.start).bind(event.end).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn report(db: &mut PgConnection, run: Uuid, through: i64) -> Result<Value> {
    let summary = sqlx::query_scalar::<_, Value>(
        r#"WITH s AS (
            SELECT *, quality_status='OK' AS valid,
                (diagnostics->>'mid_move_bp_30s')::float8 AS move,
                (diagnostics->>'depth_log_ratio')::float8 AS log_depth
            FROM vet_shadow_samples WHERE run_id=$1 AND exchange_ts<=$2
        ), positive AS (
            SELECT asset, exchange_ts,
                lag(exchange_ts) OVER (PARTITION BY asset ORDER BY exchange_ts) AS last
            FROM s WHERE valid AND vet > 0
        ), clusters AS (
            SELECT asset, exchange_ts, sum(CASE WHEN exchange_ts-last=10000 THEN 0 ELSE 1 END)
                OVER (PARTITION BY asset ORDER BY exchange_ts) AS grp
            FROM positive
        ), cluster_sizes AS (
            SELECT asset, count(*) AS points, min(exchange_ts) AS first, max(exchange_ts) AS last
            FROM clusters GROUP BY asset,grp
        ), by_asset AS (
            SELECT asset, count(*) AS slots, count(vet) AS computed, count(*) FILTER (WHERE valid) AS valid,
                count(*) FILTER (WHERE valid AND vet > 0) AS positive,
                count(*) FILTER (WHERE diagnostics->>'book_10s_ok'='false') AS book_gap_bins,
                count(*) FILTER (WHERE diagnostics->>'trades_10s_ok'='false') AS trade_gap_bins,
                count(*) FILTER (WHERE diagnostics->>'book_10s_ok'='true' AND diagnostics->>'depth_10s_ok'='false') AS band_short_bins,
                percentile_disc(ARRAY[0.5,0.9,0.99]) WITHIN GROUP (ORDER BY vet) FILTER (WHERE valid) AS vet_p50_p90_p99,
                percentile_disc(ARRAY[0.5,0.9,0.99]) WITHIN GROUP (ORDER BY vet) FILTER (WHERE valid AND vet>0) AS positive_p50_p90_p99,
                percentile_disc(ARRAY[0.5,0.9,0.99]) WITHIN GROUP (ORDER BY abs(move))
                    FILTER (WHERE valid AND vet>0 AND move IS NOT NULL) AS positive_abs_mid_move_bp_30s_p50_p90_p99,
                corr(abs(move),log_depth) FILTER (WHERE valid) AS corr_band_abs_move_vs_log_depth,
                count(*) FILTER (WHERE valid AND l_depth>0 AND diagnostics->>'depth_pattern'='BOTH') AS l_positive_both,
                count(*) FILTER (WHERE valid AND l_depth>0 AND diagnostics->>'depth_pattern'='BID_ONLY') AS l_positive_bid_only,
                count(*) FILTER (WHERE valid AND l_depth>0 AND diagnostics->>'depth_pattern'='ASK_ONLY') AS l_positive_ask_only,
                count(*) FILTER (WHERE valid AND l_depth>0 AND diagnostics->>'depth_pattern'='NEITHER') AS l_positive_neither
            FROM s GROUP BY asset
        ), hour AS (
            SELECT asset, (exchange_ts-1)/3600000 AS utc_hour, count(*) AS slots,
                count(*) FILTER (WHERE valid) AS valid, count(*) FILTER (WHERE valid AND vet>0) AS positive,
                count(*) FILTER (WHERE diagnostics->>'book_10s_ok'='false') AS book_gap_bins,
                count(*) FILTER (WHERE diagnostics->>'trades_10s_ok'='false') AS trade_gap_bins,
                count(*) FILTER (WHERE diagnostics->>'book_10s_ok'='true' AND diagnostics->>'depth_10s_ok'='false') AS band_short_bins,
                percentile_disc(ARRAY[0.5,0.9,0.99]) WITHIN GROUP (ORDER BY vet) FILTER (WHERE valid) AS vet_p50_p90_p99
            FROM s GROUP BY 1,2
        ), quality AS (
            SELECT asset,quality_status,count(*) AS slots FROM s GROUP BY 1,2
        ), labels AS (
            SELECT l.asset,l.minutes,l.status,count(*) AS n,
                   min(l.completed_at-l.target_ts) AS min_completion_lag_ms
            FROM vet_shadow_labels l JOIN s USING(run_id,asset,exchange_ts)
            GROUP BY 1,2,3
        )
        SELECT jsonb_build_object(
            'run_id',$1,'through_exchange_ts',(SELECT max(exchange_ts) FROM s),
            'requested_end_ts',$2::bigint,'computed_at',extract(epoch from clock_timestamp())*1000,
            'assets',coalesce((SELECT jsonb_agg(to_jsonb(a) ORDER BY asset) FROM by_asset a),'[]'::jsonb),
            'utc_hours',coalesce((SELECT jsonb_agg(to_jsonb(h) ORDER BY asset,utc_hour) FROM hour h),'[]'::jsonb),
            'quality',coalesce((SELECT jsonb_agg(to_jsonb(q) ORDER BY asset,quality_status) FROM quality q),'[]'::jsonb),
            'labels',coalesce((SELECT jsonb_agg(to_jsonb(l) ORDER BY asset,minutes,status) FROM labels l),'[]'::jsonb),
            'clusters',coalesce((SELECT jsonb_agg(to_jsonb(x) ORDER BY asset) FROM (
                SELECT asset,count(*) AS events,sum(points) AS positive_points,
                    max(points) AS maximum_points, percentile_disc(0.5) WITHIN GROUP (ORDER BY points) AS median_points
                FROM cluster_sizes GROUP BY asset) x),'[]'::jsonb)
        )"#
    ).bind(run).bind(through).fetch_one(db).await?;
    Ok(summary)
}
