use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{cdfp, error::Result, extractors::CurrentUser, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/report", get(report))
}

#[derive(sqlx::FromRow)]
struct Run {
    id: Uuid,
    started_at: i64,
    finished_at: Option<i64>,
}

async fn latest_run(pool: &PgPool) -> Result<Option<Run>> {
    Ok(sqlx::query_as::<_, Run>(
        "SELECT id, started_at, finished_at FROM cdfp_runs WHERE factor_version=$1
         ORDER BY started_at DESC LIMIT 1",
    )
    .bind(cdfp::VERSION)
    .fetch_optional(pool)
    .await?)
}

async fn status(_user: CurrentUser, State(state): State<AppState>) -> Result<Json<Value>> {
    let run = latest_run(&state.db_pool).await?;
    let mut assets = Vec::new();
    let mut last = None;
    for asset in cdfp::ASSETS {
        let (sample, label) = if let Some(run) = &run {
            let sample = sqlx::query_scalar::<_, Value>(
                "SELECT to_jsonb(s) - 'run_id' FROM cdfp_samples s
                 WHERE run_id=$1 AND asset=$2 ORDER BY factor_time DESC LIMIT 1",
            )
            .bind(run.id)
            .bind(asset)
            .fetch_optional(&state.db_pool)
            .await?;
            let label = sqlx::query_scalar::<_, Value>(
                "SELECT (labels->'5') || jsonb_build_object('factor_time', factor_time) FROM cdfp_samples
                 WHERE run_id=$1 AND asset=$2 AND labels->'5'->>'status'='COMPLETE'
                 ORDER BY factor_time DESC LIMIT 1",
            )
            .bind(run.id).bind(asset).fetch_optional(&state.db_pool).await?;
            (sample, label)
        } else {
            (None, None)
        };
        if let Some(stamp) = sample
            .as_ref()
            .and_then(|data| data["available_time"].as_i64())
        {
            last = Some(last.map_or(stamp, |previous: i64| previous.max(stamp)));
        }
        assets.push(json!({"asset": asset, "sample": sample, "five_minute": label}));
    }
    let phase = match (&run, last) {
        _ if !cdfp::enabled() => "DISABLED",
        (None, _) => "STARTING",
        (Some(run), _) if run.finished_at.is_some() => "FINISHED",
        (Some(run), None) if cdfp::now_ms() - run.started_at < 45_000 => "STARTING",
        (_, Some(t)) if cdfp::now_ms() - t < 45_000 => "COLLECTING",
        _ => "STALE",
    };
    Ok(Json(json!({
        "state": phase, "factor_version": cdfp::VERSION, "assets": assets,
        "run_id": run.as_ref().map(|row| row.id), "started_time": run.map(|row| row.started_at)
    })))
}

async fn report(_user: CurrentUser, State(state): State<AppState>) -> Result<Json<Value>> {
    let Some(run) = latest_run(&state.db_pool).await? else {
        return Ok(Json(json!({"status": "AWAITING_SAMPLE", "assets": {}})));
    };
    Ok(Json(cdfp::report::build(&state.db_pool, run.id).await?))
}
