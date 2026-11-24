//! Forward-only OKX research task managed by the MoneyRobert backend.

mod okx;
pub(crate) mod report;
mod study;

use anyhow::{ensure, Result};
use chrono::Utc;
use sqlx::{Connection, PgConnection, PgPool};
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinSet, time};
use uuid::Uuid;

use study::{initial_labels, Changes, Sample, Study, GRACE};

pub const ASSETS: [&str; 3] = ["BTC", "ETH", "SOL"];
const MARKETS: [&str; 6] = [
    "BTC-USDT",
    "BTC-USDT-SWAP",
    "ETH-USDT",
    "ETH-USDT-SWAP",
    "SOL-USDT",
    "SOL-USDT-SWAP",
];
pub const VERSION: &str = "cdfp-v0.1-candidate";
const WORKER_LOCK: i64 = 11_291_692_624;

pub fn enabled() -> bool {
    std::env::var("CDFP_ENABLED").is_ok_and(|value| matches!(value.as_str(), "true" | "1"))
}

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn start(pool: PgPool) {
    if enabled() {
        tokio::spawn(async move {
            loop {
                if let Err(error) = run(&pool).await {
                    tracing::warn!(%error, "CDFP worker stopped");
                }
                time::sleep(Duration::from_secs(30)).await;
            }
        });
    }
}

async fn run(pool: &PgPool) -> Result<()> {
    // Use a dedicated session so a small application pool still serves HTTP requests.
    let mut lock = PgConnection::connect_with(&pool.connect_options()).await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(WORKER_LOCK)
        .fetch_one(&mut lock)
        .await?;
    if !acquired {
        return Ok(());
    }
    let (multipliers, specs) = okx::contracts().await?;
    let id = Uuid::new_v4();
    let started = now_ms();
    let mut tx = lock.begin().await?;
    sqlx::query("UPDATE cdfp_runs SET finished_at=$1 WHERE finished_at IS NULL")
        .bind(started)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO cdfp_runs (id, started_at, factor_version, contracts) VALUES ($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(started)
    .bind(VERSION)
    .bind(specs)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(run_id = %id, "CDFP research collection started");

    let result = collect(&mut lock, id, started, multipliers).await;
    let finish = sqlx::query("UPDATE cdfp_runs SET finished_at=$2 WHERE id=$1")
        .bind(id)
        .bind(now_ms())
        .execute(&mut lock)
        .await;
    result?;
    finish?;
    Ok(())
}

async fn collect(
    db: &mut PgConnection,
    id: Uuid,
    started: i64,
    multipliers: [f64; 6],
) -> Result<()> {
    let mut study = Study::new(started, multipliers);
    let (send, mut receive) = mpsc::channel(16_384);
    let mut feeds = JoinSet::new();
    for index in 0..MARKETS.len() {
        feeds.spawn(okx::feed(index, send.clone()));
    }
    drop(send);
    let mut ticker = time::interval(Duration::from_millis(200));
    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let clock = time::Instant::now();
    let wall = now_ms();
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let now = now_ms();
                ensure!((now - wall - clock.elapsed().as_millis() as i64).abs() <= 500,
                    "UTC clock shifted during CDFP capture");
                apply(db, id, study.expire_labels(now)).await?;
                while now >= study.end + GRACE {
                    let samples = study.seal(|| now_ms().max(now));
                    store(db, id, samples).await?;
                }
            }
            event = receive.recv() => {
                let Some((market, received, frame)) = event else {
                    anyhow::bail!("CDFP market streams closed");
                };
                apply(db, id, study.receive(market, received, frame)).await?;
            }
        }
    }
}

async fn store(db: &mut PgConnection, run: Uuid, samples: [Sample; 3]) -> Result<()> {
    let mut tx = db.begin().await?;
    for sample in samples {
        let labels = initial_labels(&sample);
        sqlx::query(
            "INSERT INTO cdfp_samples (
                run_id, asset, factor_version, factor_time, available_time, cdfp_value, issued_cdfp_value,
                spot_imbalance, perp_imbalance, same_direction_bins, volume_quality, market_regime,
                quality_status, momentum, abs_return_1m, realized_vol_15m, thresholds, bins, labels
            ) VALUES ($1,$2,$3,$4,$5,$6,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
        )
        .bind(run).bind(sample.asset).bind(sample.factor_version).bind(sample.factor_time)
        .bind(sample.available_time).bind(sample.cdfp_value).bind(sample.spot_imbalance)
        .bind(sample.perp_imbalance).bind(sample.same_direction_bins).bind(sample.volume_quality)
        .bind(sample.market_regime).bind(sample.quality_status).bind(sample.momentum)
        .bind(sample.abs_return_1m).bind(sample.realized_vol_15m).bind(sample.thresholds)
        .bind(sample.bins).bind(labels).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn apply(db: &mut PgConnection, run: Uuid, changes: Changes) -> Result<()> {
    if changes.labels.is_empty() && changes.gaps.is_empty() {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    for gap in changes.gaps {
        sqlx::query(
            "UPDATE cdfp_samples SET cdfp_value=NULL, spot_imbalance=NULL, perp_imbalance=NULL,
                same_direction_bins=NULL, quality_status='GAP', volume_quality='UNKNOWN',
                gap_detected_time=coalesce(gap_detected_time,$5)
             WHERE run_id=$1 AND asset=$2 AND factor_time>$3 AND factor_time<=$4",
        )
        .bind(run)
        .bind(ASSETS[gap.asset])
        .bind(gap.start)
        .bind(gap.through)
        .bind(gap.detected)
        .execute(&mut *tx)
        .await?;
    }
    for (asset, item) in changes.labels {
        sqlx::query(
            "UPDATE cdfp_samples SET price_time=$4, price_event_time=$5, start_price=$6, labels=$7
             WHERE run_id=$1 AND asset=$2 AND factor_time=$3",
        )
        .bind(run)
        .bind(ASSETS[asset])
        .bind(item.factor_time)
        .bind(item.price_time)
        .bind(item.price_event_time)
        .bind(item.start_price)
        .bind(item.labels())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
