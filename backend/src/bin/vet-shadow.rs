#[path = "vet_shadow/storage.rs"]
mod storage;
#[path = "vet_shadow/study.rs"]
mod study;
#[path = "vet_shadow/wire.rs"]
mod wire;

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinSet, time};
use uuid::Uuid;

const ASSETS: [&str; 3] = ["BTC", "ETH", "SOL"];
const MARKETS: [&str; 3] = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"];
const VERSION: &str = "vet-v0.2-shadow";
const LOCK: i64 = 14_462_344_202;
const DAY: i64 = 86_400_000;

fn now() -> i64 {
    Utc::now().timestamp_millis()
}

fn export(path: &Path, value: &Value) -> Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

async fn run(db: &mut PgConnection, output: &Path) -> Result<()> {
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(LOCK)
        .fetch_one(&mut *db)
        .await?;
    ensure!(
        locked,
        "Another VET shadow worker holds the PostgreSQL lock"
    );
    sqlx::raw_sql(include_str!("../../migrations/035_vet_shadow.sql"))
        .execute(&mut *db)
        .await?;
    let (contracts, metadata) = wire::specs().await?;
    let id = Uuid::new_v4();
    let start = now();
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE vet_shadow_runs SET finished_at=$1 WHERE finished_at IS NULL")
        .bind(start)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO vet_shadow_runs(id,factor_version,started_at,contracts) VALUES($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(VERSION)
    .bind(start)
    .bind(metadata)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(%id, started_at = start, "VET shadow collection started");

    let result = collect(db, output, id, start, contracts).await;
    let finish = sqlx::query("UPDATE vet_shadow_runs SET finished_at=$2 WHERE id=$1")
        .bind(id)
        .bind(now())
        .execute(db)
        .await;
    result?;
    finish?;
    Ok(())
}

async fn collect(
    db: &mut PgConnection,
    output: &Path,
    id: Uuid,
    start: i64,
    specs: [(f64, f64); 3],
) -> Result<()> {
    let mut books = ASSETS.map(|_| study::Market::new(start));
    let (send, mut incoming) = mpsc::channel(16_384);
    let mut tasks = JoinSet::new();
    for (index, spec) in specs.into_iter().enumerate() {
        tasks.spawn(wire::feed(index, send.clone(), spec));
    }
    drop(send);
    let mut tick = time::interval(Duration::from_millis(200));
    tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut end = study::bucket(start) + study::STEP;
    let checkpoint = end - study::STEP + DAY;
    let mut saved = false;
    let clock = time::Instant::now();
    let clock_start = now();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = tick.tick() => {
                let current = now();
                ensure!((current-clock_start-clock.elapsed().as_millis() as i64).abs() <= 500, "UTC clock stepped");
                while current >= end + study::GRACE {
                    // Apply frames already observed by the receiver before closing the cutoff.
                    while let Ok((index, received, frame)) = incoming.try_recv() {
                        for event in books[index].receive(received, frame) {
                            storage::fault(db, id, ASSETS[index], event).await?;
                        }
                    }
                    for (index, market) in books.iter_mut().enumerate() {
                        market.close(end);
                        let mut sample = market.sample();
                        sample.available = now().max(end + study::GRACE);
                        let outcomes = market.outcomes(end);
                        storage::save(db, id, ASSETS[index], &sample, outcomes).await?;
                        market.arm(&sample, now());
                    }
                    if !saved && end >= checkpoint + 920_000 {
                        let result = storage::report(db,id,checkpoint).await?;
                        sqlx::query("INSERT INTO vet_shadow_checkpoints(run_id,hours,created_at,report) VALUES($1,24,$2,$3)")
                            .bind(id).bind(now()).bind(&result).execute(&mut *db).await?;
                        if let Err(error) = export(&output.join(format!("{id}-24h.json")), &result) {
                            tracing::warn!(%id, %error, "VET checkpoint is stored in PostgreSQL; file export failed");
                        }
                        tracing::info!(%id, "VET 24-hour checkpoint sealed");
                        saved = true;
                    }
                    end += study::STEP;
                }
            }
            frame = incoming.recv() => {
                let (index, received, frame) = frame.context("VET receivers exited")?;
                for event in books[index].receive(received,frame) {
                    storage::fault(db,id,ASSETS[index],event).await?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_ansi(false).init();
    let mut args = std::env::args_os().skip(1);
    let mode = args
        .next()
        .context("usage: vet-shadow run|report OUTPUT_DIRECTORY")?;
    let dir = PathBuf::from(args.next().context("Missing output directory")?);
    ensure!(args.next().is_none(), "Unexpected trailing arguments");
    let url = std::env::var("VET_DATABASE_URL")
        .context("Set VET_DATABASE_URL to the project PostgreSQL connection")?;
    std::fs::create_dir_all(&dir)?;
    let mut db = PgConnection::connect(&url).await?;
    if mode == "run" {
        run(&mut db, &dir).await?;
    } else if mode == "report" {
        let run: Uuid =
            sqlx::query_scalar("SELECT id FROM vet_shadow_runs ORDER BY started_at DESC LIMIT 1")
                .fetch_one(&mut db)
                .await?;
        let result = storage::report(&mut db, run, now()).await?;
        export(&dir.join("live_status.json"), &result)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        anyhow::bail!("Unknown mode. Use run or report.");
    }
    Ok(())
}
