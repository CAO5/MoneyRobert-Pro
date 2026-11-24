use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use super::{study::DAY, ASSETS, VERSION};

const GROUPS: [&str; 5] = [
    "[-1,-0.6)",
    "[-0.6,-0.2)",
    "[-0.2,0.2)",
    "[0.2,0.6)",
    "[0.6,1]",
];
const EDGES: [f64; 4] = [-0.6, -0.2, 0.2, 0.6];
const MODELS: [(&str, &[usize]); 6] = [
    ("cdfp_only", &[3]),
    ("momentum_only", &[0]),
    ("spot_only", &[1]),
    ("perp_only", &[2]),
    ("combined_controls", &[0, 1, 2]),
    ("combined_with_cdfp", &[0, 1, 2, 3]),
];
const MIN_TRAIN: usize = 30;

#[derive(sqlx::FromRow)]
struct Row {
    asset: String,
    available_time: i64,
    price_time: Option<i64>,
    label_time: Option<i64>,
    completed_time: Option<i64>,
    label_status: String,
    cdfp_value: Option<f64>,
    momentum: Option<f64>,
    spot_imbalance: Option<f64>,
    perp_imbalance: Option<f64>,
    log_return: Option<f64>,
    quality_status: String,
    volume_quality: String,
    market_regime: String,
}

struct Observation {
    available: i64,
    label_end: i64,
    completed: i64,
    input: [f64; 4],
    outcome: f64,
}

fn group(score: f64) -> usize {
    EDGES.partition_point(|edge| score >= *edge)
}

fn conditional(rows: &[&Observation]) -> Value {
    let mut groups = [0; 5];
    let mut sums = [0.0; 5];
    let mut positive = [0; 5];
    for row in rows {
        let index = group(row.input[3]);
        groups[index] += 1;
        sums[index] += row.outcome;
        positive[index] += usize::from(row.outcome > 0.0);
    }
    Value::Object(GROUPS.iter().enumerate().map(|(index, name)| {
        let count = groups[index];
        ((*name).into(), json!({
            "count": count,
            "mean_log_return": (count > 0).then(|| sums[index] / count as f64),
            "positive_return_fraction": (count > 0).then(|| positive[index] as f64 / count as f64)
        }))
    }).collect())
}

fn correlation(rows: &[&Observation]) -> Option<f64> {
    if rows.len() < 2 {
        return None;
    }
    let count = rows.len() as f64;
    let mean_x = rows.iter().map(|row| row.input[3]).sum::<f64>() / count;
    let mean_y = rows.iter().map(|row| row.outcome).sum::<f64>() / count;
    let [sx, sy, covariance] = rows.iter().fold([0.0; 3], |[sx, sy, covariance], row| {
        let x = row.input[3] - mean_x;
        let y = row.outcome - mean_y;
        [sx + x * x, sy + y * y, covariance + x * y]
    });
    let scale = (sx * sy).sqrt();
    (scale > 0.0).then(|| covariance / scale)
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

struct Term {
    field: usize,
    mean: f64,
    projections: Vec<f64>,
    norm: f64,
    target: f64,
}

fn fit(train: &[&Observation], fields: &[usize]) -> impl Fn(&Observation) -> f64 {
    // Center predictors and orthogonalize the columns before least squares.
    let count = train.len() as f64;
    let intercept = train.iter().map(|row| row.outcome).sum::<f64>() / count;
    let outcome: Vec<_> = train.iter().map(|row| row.outcome - intercept).collect();
    let mut basis: Vec<Vec<f64>> = Vec::new();
    let mut terms = Vec::new();
    for &field in fields {
        let mean = train.iter().map(|row| row.input[field]).sum::<f64>() / count;
        let mut column: Vec<_> = train.iter().map(|row| row.input[field] - mean).collect();
        let mut projections = Vec::new();
        for previous in &basis {
            let projection = dot(&column, previous);
            projections.push(projection);
            for (value, unit) in column.iter_mut().zip(previous) {
                *value -= projection * unit;
            }
        }
        let norm = dot(&column, &column).sqrt();
        if norm < 1e-10 {
            continue;
        }
        column.iter_mut().for_each(|value| *value /= norm);
        let target = dot(&column, &outcome);
        basis.push(column);
        terms.push(Term {
            field,
            mean,
            projections,
            norm,
            target,
        });
    }
    move |row| {
        let mut components = Vec::with_capacity(terms.len());
        let mut predicted = intercept;
        for term in &terms {
            let value = (row.input[term.field] - term.mean - dot(&term.projections, &components))
                / term.norm;
            components.push(value);
            predicted += value * term.target;
        }
        predicted
    }
}

fn reduction(candidate: f64, base: f64) -> Option<f64> {
    (base > 0.0).then(|| 1.0 - candidate / base)
}

fn comparison(errors: [f64; 6], count: usize) -> Value {
    if count == 0 {
        return json!({"count": 0, "mse": {}, "cdfp_only_mse_reduction_vs": {},
                      "incremental_cdfp_mse_reduction": null});
    }
    let mse: HashMap<_, _> = MODELS
        .iter()
        .zip(errors)
        .map(|((name, _), error)| (*name, error / count as f64))
        .collect();
    json!({
        "count": count, "mse": mse, "cdfp_only_mse_reduction_vs": {
            "momentum_only": reduction(errors[0], errors[1]),
            "spot_only": reduction(errors[0], errors[2]),
            "perp_only": reduction(errors[0], errors[3])
        }, "incremental_cdfp_mse_reduction": reduction(errors[5], errors[4])
    })
}

fn walk_forward(rows: &[&Observation]) -> Value {
    let mut days: BTreeMap<i64, Vec<&Observation>> = BTreeMap::new();
    for &row in rows {
        days.entry(row.available.div_euclid(DAY) * DAY)
            .or_default()
            .push(row);
    }
    let mut previous: Vec<&Observation> = Vec::new();
    let mut tested = Vec::new();
    let mut blocks = Vec::new();
    let mut totals = [0.0; 6];
    for (date, test) in days {
        let train: Vec<_> = previous
            .iter()
            .copied()
            .filter(|row| row.completed < date && row.label_end < date)
            .collect();
        if train.len() >= MIN_TRAIN {
            let errors = MODELS.map(|(_, fields)| {
                let predict = fit(&train, fields);
                test.iter()
                    .map(|row| (predict(row) - row.outcome).powi(2))
                    .sum::<f64>()
            });
            for (total, error) in totals.iter_mut().zip(errors) {
                *total += error;
            }
            let mut block = comparison(errors, test.len());
            block["utc_day"] =
                json!(chrono::DateTime::from_timestamp_millis(date).map(|time| time.date_naive()));
            block["train_count"] = json!(train.len());
            block["score_return_correlation"] = json!(correlation(&test));
            blocks.push(block);
            tested.extend(&test);
        }
        previous.extend(test);
    }
    let mut output = comparison(totals, tested.len());
    output["utc_day_blocks"] = json!(blocks);
    output["conditional_returns"] = conditional(&tested);
    output["score_return_correlation"] = json!(correlation(&tested));
    output
}

fn summarize(rows: &[&Row]) -> Value {
    let count = rows.len();
    let mut groups = [0; 5];
    let mut labels = HashMap::<_, usize>::new();
    let mut flags = [0; 7];
    let mut eligible = 0;
    let mut selected = Vec::new();
    let mut until = i64::MIN;
    for row in rows {
        *labels.entry(&row.label_status).or_default() += 1;
        let rejected = [
            row.quality_status == "GAP",
            row.quality_status == "WARMUP",
            row.volume_quality == "LOW",
            row.volume_quality == "UNKNOWN",
            row.market_regime == "WARMUP",
            row.market_regime == "EXTREME",
            row.cdfp_value.is_some() && row.label_status != "COMPLETE",
        ];
        for (flag, rejected) in flags.iter_mut().zip(rejected) {
            *flag += usize::from(rejected);
        }
        let Some(score) = row.cdfp_value else {
            continue;
        };
        groups[group(score)] += 1;
        if row.quality_status != "OK"
            || row.volume_quality != "OK"
            || row.market_regime != "NORMAL"
            || row.label_status != "COMPLETE"
        {
            continue;
        }
        let (
            Some(momentum),
            Some(spot),
            Some(perp),
            Some(outcome),
            Some(start),
            Some(end),
            Some(completed),
        ) = (
            row.momentum,
            row.spot_imbalance,
            row.perp_imbalance,
            row.log_return,
            row.price_time,
            row.label_time,
            row.completed_time,
        )
        else {
            continue;
        };
        eligible += 1;
        if start >= until {
            selected.push(Observation {
                available: row.available_time,
                label_end: end,
                completed,
                input: [momentum, spot, perp, score],
                outcome,
            });
            until = end;
        }
    }
    let distribution: HashMap<_, _> = GROUPS.into_iter().zip(groups).collect();
    let names = [
        "data_gap",
        "flow_warmup",
        "low_volume",
        "volume_unknown",
        "market_warmup",
        "extreme_market",
        "missing_five_minute_label",
    ];
    let exclusions: HashMap<_, _> = names.into_iter().zip(flags).map(|(name, flag)| {
        (name, json!({"count": flag, "fraction_of_all_samples": (count > 0).then(|| flag as f64 / count as f64)}))
    }).collect();
    let selected: Vec<_> = selected.iter().collect();
    json!({
        "samples": count, "scored_samples": groups.iter().sum::<usize>(),
        "score_distribution": distribution, "five_minute_label_status": labels, "exclusions": exclusions,
        "evaluation_eligible": eligible, "nonoverlapping_samples": selected.len(),
        "conditional_returns_nonoverlapping": conditional(&selected),
        "score_return_correlation_nonoverlapping": correlation(&selected), "out_of_sample": walk_forward(&selected)
    })
}

pub(crate) async fn build(pool: &PgPool, run: Uuid) -> Result<Value, sqlx::Error> {
    let rows = sqlx::query_as::<_, Row>(
        "SELECT asset, available_time, price_time, cdfp_value, momentum, spot_imbalance, perp_imbalance,
                quality_status, volume_quality, market_regime, labels->'5'->>'status' AS label_status,
                (labels->'5'->>'price_time')::bigint AS label_time,
                (labels->'5'->>'completed_time')::bigint AS completed_time,
                (labels->'5'->>'log_return')::double precision AS log_return
         FROM cdfp_samples WHERE run_id=$1 AND factor_version=$2 ORDER BY asset, available_time, factor_time",
    ).bind(run).bind(VERSION).fetch_all(pool).await?;
    let output = tokio::task::spawn_blocking(move || {
        let assets: HashMap<_, _> = ASSETS.into_iter().map(|asset| {
            let rows: Vec<_> = rows.iter().filter(|row| row.asset == asset).collect();
            (asset, summarize(&rows))
        }).collect();
        let reviewable = assets.values().all(|report| report["out_of_sample"]["count"].as_u64().unwrap_or(0) > 0);
        json!({"factor_version": VERSION, "primary_horizon_minutes": 5, "run_id": run,
               "status": if reviewable { "REVIEWABLE" } else { "AWAITING_SAMPLE" }, "assets": assets})
    }).await.map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_fit_excludes_training_labels_that_cross_the_boundary() {
        let observation = |stamp, index: i32| {
            let score = ((index % 11) - 5) as f64 / 5.0;
            Observation {
                available: stamp,
                label_end: stamp + 300_000,
                completed: stamp + 300_000,
                input: [
                    (index % 5) as f64,
                    (index % 3) as f64,
                    (index % 7) as f64,
                    score,
                ],
                outcome: score / 100.0,
            }
        };
        let mut rows: Vec<_> = (0..32)
            .map(|index| observation(index as i64 * 600_000, index))
            .collect();
        rows.push(observation(DAY - 100_000, 45));
        rows.extend((1..12).map(|index| observation(DAY + index as i64 * 600_000, index)));
        let refs: Vec<_> = rows.iter().collect();
        let report = walk_forward(&refs);
        assert_eq!(report["utc_day_blocks"][0]["train_count"], 32);
        assert_eq!(report["count"], 11);
        assert!((report["incremental_cdfp_mse_reduction"].as_f64().unwrap() - 1.0).abs() < 1e-12);
    }
}
