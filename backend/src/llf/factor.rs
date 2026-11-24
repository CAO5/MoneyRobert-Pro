use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sqlx::{FromRow, PgPool};
use tokio::time::{interval, Duration, MissedTickBehavior};
use uuid::Uuid;

const ASSETS: [&str; 3] = ["BTC", "ETH", "SOL"];
const MIN_HISTORY: usize = 38_880;

#[derive(FromRow)]
struct MinuteInputs {
    oi_level: Option<f64>,
    oi_growth: Option<f64>,
    funding: Option<f64>,
    basis: Option<f64>,
    flow: Option<f64>,
    liquidity: Option<f64>,
    evidence: Value,
}

#[derive(FromRow)]
struct HistoricalMinute {
    raw_values: Vec<Option<f64>>,
    score: Option<f64>,
}

fn percentile(value: f64, history: impl Iterator<Item = f64>) -> Option<f64> {
    let mut total = 0;
    let mut lower = 0;
    let mut ties = 0;
    for sample in history.filter(|sample| sample.is_finite()) {
        total += 1;
        lower += usize::from(sample < value);
        ties += usize::from(sample == value);
    }
    (total >= MIN_HISTORY).then(|| (lower as f64 + 0.5 * ties as f64 + 0.5) / (total as f64 + 1.0))
}

fn score(
    raw: &[Option<f64>; 6],
    history: &[HistoricalMinute],
) -> Option<(Value, f64, Option<f64>)> {
    let rank = |index: usize| {
        percentile(
            raw[index]?,
            history
                .iter()
                .filter_map(|row| row.raw_values.get(index).copied().flatten()),
        )
    };
    let oi_load = (rank(0)? * rank(1)?).sqrt();
    let carry = (rank(2)? * rank(3)?).sqrt();
    let flow = rank(4)?;
    let liquidity = rank(5)?;
    let value = 100.0 * (oi_load * carry * flow * liquidity).powf(0.25);
    let historical_rank = percentile(value, history.iter().filter_map(|row| row.score));
    Some((
        json!({"oi_load":oi_load,"carry":carry,"flow":flow,"liquidity":liquidity}),
        value,
        historical_rank,
    ))
}

// 这些是源数据新鲜度预算。因子窗口、归一化和有效样本口径将在公式冻结后单独确定。
const SPOT_CHANNELS: &[(&str, i64)] = &[("books5", 5), ("trades", 15), ("depth-25bp", 5)];
const SWAP_CHANNELS: &[(&str, i64)] = &[
    ("books5", 5),
    ("trades", 15),
    ("depth-25bp", 5),
    ("open-interest", 15),
    ("funding-rate", 180),
    ("mark-price", 15),
    ("instruments", 1_200),
];

#[derive(FromRow)]
struct Reading {
    symbol: String,
    channel: String,
    raw_id: i64,
    exchange_ts: DateTime<Utc>,
    received_at: DateTime<Utc>,
    session_id: Option<Uuid>,
    quality: String,
    payload: Value,
}

impl Reading {
    fn status(
        &self,
        now: DateTime<Utc>,
        max_age: i64,
        active_session: Option<Uuid>,
    ) -> &'static str {
        if self.quality != "ok" && self.quality != "insufficient_depth" {
            return "invalid";
        }
        let age = (now - self.exchange_ts).num_milliseconds();
        let receive_age = (now - self.received_at).num_milliseconds();
        let disconnected = self.channel != "instruments"
            && (active_session.is_none() || active_session != self.session_id);
        if disconnected
            || !(-2_000..=max_age * 1_000).contains(&age)
            || !(0..=max_age * 1_000).contains(&receive_age)
        {
            return "stale";
        }
        if self.channel == "depth-25bp"
            && (self.payload["bid_covered"].as_bool() != Some(true)
                || self.payload["ask_covered"].as_bool() != Some(true))
        {
            return "partial_depth";
        }
        "fresh"
    }

    fn input(&self, now: DateTime<Utc>, status: &str) -> Value {
        // 只复制诊断数据。完整盘口与原始消息由 raw_id 定位，避免每秒快照重复保存。
        let values = match self.channel.as_str() {
            "books5" => {
                json!({"bid": self.payload["bids"][0][0], "ask": self.payload["asks"][0][0]})
            }
            "trades" => {
                json!({"px": self.payload["px"], "sz": self.payload["sz"], "side": self.payload["side"]})
            }
            "open-interest" => {
                json!({"oi": self.payload["oi"], "oiCcy": self.payload["oiCcy"], "oiUsd": self.payload["oiUsd"]})
            }
            "funding-rate" => {
                json!({"fundingRate": self.payload["fundingRate"], "fundingTime": self.payload["fundingTime"]})
            }
            "mark-price" => json!({"markPx": self.payload["markPx"]}),
            _ => self.payload.clone(),
        };
        json!({
            "raw_id": self.raw_id,
            "exchange_ts": self.exchange_ts,
            "received_at": self.received_at,
            "age_ms": (now - self.exchange_ts).num_milliseconds(),
            "status": status,
            "source_quality": self.quality,
            "values": values,
        })
    }
}

pub async fn run(db: PgPool) {
    let mut timer = interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        match observe(&db).await {
            Ok(ids) => {
                for id in ids {
                    if let Err(error) = super::paper::on_snapshot(&db, id).await {
                        tracing::warn!(id, %error, "LLF paper accounting failed");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "LLF observation failed"),
        }
    }
}

async fn observe(db: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    // 留出归档批次落库时间，所有因子查询仍以整分钟为接收和行情时间上界。
    let cutoff_second = (Utc::now().timestamp() - 1).div_euclid(60) * 60;
    let cutoff = DateTime::from_timestamp(cutoff_second, 0).expect("current UTC minute");
    let cached = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM llf_snapshots WHERE factor_name='llf_v0_2' AND sample_second=$1 AND formula_status='confirmed'",
    ).bind(cutoff_second).fetch_all(db).await?;
    if !cached.is_empty() {
        return Ok(cached);
    }
    let mut tx = db.begin().await?;
    // 多实例可运行 API。写快照和覆盖日使用短事务串行化，按秒唯一键防止重复入账。
    let owns_tick = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock(1263291240)")
        .fetch_one(&mut *tx)
        .await?;
    if !owns_tick {
        return Ok(Vec::new());
    }
    let session = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        "SELECT session_id, connected_at FROM llf_market_session WHERE id = 1 AND connected",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let active_session = session.map(|(id, _)| id);
    let readings = sqlx::query_as::<_, Reading>(
        "SELECT symbol, channel, raw_id, exchange_ts, received_at, session_id, quality, payload
         FROM llf_market_latest WHERE channel <> 'books'",
    )
    .fetch_all(&mut *tx)
    .await?;
    let readings: HashMap<_, _> = readings
        .iter()
        .map(|row| ((row.symbol.as_str(), row.channel.as_str()), row))
        .collect();
    let now = Utc::now();
    let mut ids = Vec::with_capacity(ASSETS.len());
    for asset in ASSETS {
        let mut inputs = Map::new();
        let mut missing = false;
        let mut stale = false;
        for (suffix, channels) in [("USDT", SPOT_CHANNELS), ("USDT-SWAP", SWAP_CHANNELS)] {
            let symbol = format!("{asset}-{suffix}");
            let mut market = Map::new();
            for &(channel, max_age) in channels {
                let Some(reading) = readings.get(&(symbol.as_str(), channel)) else {
                    missing = true;
                    market.insert(channel.into(), json!({"status":"missing"}));
                    continue;
                };
                let status = reading.status(now, max_age, active_session);
                missing |= matches!(status, "invalid" | "partial_depth");
                stale |= status == "stale";
                market.insert(channel.into(), reading.input(now, status));
                if channel == "open-interest" && status == "fresh" {
                    sqlx::query(
                        "INSERT INTO llf_oi_coverage (asset, sample_day, first_at, last_at)
                         VALUES ($1, $2, $3, $3) ON CONFLICT (asset, sample_day) DO UPDATE
                         SET last_at = EXCLUDED.last_at
                         WHERE llf_oi_coverage.last_at < EXCLUDED.last_at",
                    )
                    .bind(asset)
                    .bind(reading.exchange_ts.date_naive())
                    .bind(reading.exchange_ts)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            // 交易所清算只在事件发生时推送，历史结算也没有秒级更新。两者归档但不参与实时缺数判断。
            for channel in ["liquidation-orders", "funding-rate-history"] {
                if let Some(reading) = readings.get(&(symbol.as_str(), channel)) {
                    market.insert(channel.into(), reading.input(now, "event"));
                }
            }
            inputs.insert(symbol, Value::Object(market));
        }
        let spot = format!("{asset}-USDT");
        let swap = format!("{asset}-USDT-SWAP");
        let mut raw = [None; 6];
        let mut evidence = Value::Null;
        if let Some((session_id, connected_at)) = session {
            if connected_at <= cutoff {
                let minute = sqlx::query_as::<_, MinuteInputs>(include_str!("minute_inputs.sql"))
                    .bind(&spot)
                    .bind(&swap)
                    .bind(cutoff)
                    .bind(session_id)
                    .fetch_one(&mut *tx)
                    .await?;
                // 重连以后，成交窗口必须重新走满 5 分钟。已连续覆盖的其他原始值仍可累积。
                raw = [
                    minute.oi_level,
                    minute.oi_growth,
                    minute.funding,
                    minute.basis,
                    (connected_at <= cutoff - chrono::Duration::minutes(5))
                        .then_some(minute.flow)
                        .flatten(),
                    minute.liquidity,
                ];
                raw.iter_mut()
                    .for_each(|value| *value = value.filter(|v| v.is_finite()));
                evidence = minute.evidence;
            }
        }
        let calculated = if raw.iter().all(Option::is_some) {
            let history = sqlx::query_as::<_, HistoricalMinute>(
                "SELECT raw_values,score FROM llf_snapshots WHERE asset=$1 AND formula_status='confirmed'
                 AND raw_values IS NOT NULL AND observed_at >= $2::timestamptz - interval '30 days' AND observed_at < $2",
            ).bind(asset).bind(cutoff).fetch_all(&mut *tx).await?;
            score(&raw, &history)
        } else {
            None
        };
        let (quality, reason) = if active_session.is_none() {
            ("waiting", "等待 OKX 行情连接")
        } else if raw.iter().any(Option::is_none) && stale && !missing {
            ("stale", "行情过期或来自上一次连接")
        } else if raw.iter().any(Option::is_none) {
            (
                "incomplete",
                "分钟窗口缺少有效的 OI、Funding、Basis、成交或盘口输入",
            )
        } else if calculated.is_none() {
            ("ready", "正在积累原始统计量，各项至少需要 38880 条分钟样本")
        } else if calculated
            .as_ref()
            .is_some_and(|(_, _, rank)| rank.is_none())
        {
            ("ready", "分数可用，正在积累 LLF 分位所需的有效分数")
        } else {
            ("ready", "分钟分数和历史分位可用")
        };
        let (components, score, rank) = calculated.map_or(
            (
                json!({"oi_load":null,"carry":null,"flow":null,"liquidity":null}),
                None,
                None,
            ),
            |(parts, score, rank)| (parts, Some(score), rank.map(|value| value * 100.0)),
        );
        // 百分位列使用百分数供现有模拟盘与展示读取，公式和对外 Rank 字段使用 0–1。
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO llf_snapshots (asset,sample_second,observed_at,quality,reason,inputs,raw_values,components,score,percentile)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (factor_name,asset,sample_second,formula_status) DO NOTHING RETURNING id",
        )
        .bind(asset)
        .bind(cutoff_second)
        .bind(cutoff)
        .bind(quality)
        .bind(reason)
        .bind(json!({"markets":inputs,"evidence":evidence}))
        .bind(raw.as_slice()).bind(components).bind(score).bind(rank)
        .fetch_optional(&mut *tx)
        .await?;
        if let (Some(snapshot_id), Some(score)) = (id, score) {
            sqlx::query(
                "INSERT INTO feature_values(feature_id,symbol,timestamp,value,metadata)
                 SELECT feature_id,$1,$2,$3,jsonb_build_object('llf_snapshot_id',$4::bigint)
                 FROM feature_definitions WHERE name='llf_v0_2'
                 ON CONFLICT (feature_id,symbol,timestamp) DO NOTHING",
            )
            .bind(&swap)
            .bind(cutoff)
            .bind(score)
            .bind(snapshot_id)
            .execute(&mut *tx)
            .await?;
        }
        ids.extend(id);
    }
    tx.commit().await?;
    Ok(ids)
}

pub async fn dashboard(db: &PgPool) -> Result<Value, sqlx::Error> {
    // 只读取各标的最近一条快照和覆盖日聚合，面板轮询不扫描逐笔行情。
    sqlx::query_scalar(
        r#"SELECT jsonb_agg(jsonb_build_object(
            'asset', a.asset, 'observed_at', s.observed_at, 'components', COALESCE(s.components,
                '{"oi_load":null,"carry":null,"flow":null,"liquidity":null}'::jsonb),
            'score', s.score, 'percentile', s.percentile, 'llf_rank_30d',s.percentile/100.0,
            'quality', CASE WHEN s.id IS NULL THEN 'waiting'
                            WHEN s.observed_at < now() - interval '70 seconds' THEN 'stale' ELSE s.quality END,
            'reason', CASE WHEN s.id IS NULL THEN '等待首个采集快照'
                           WHEN s.observed_at < now() - interval '70 seconds' THEN '采集快照已停止更新' ELSE s.reason END,
            'inputs', COALESCE(s.inputs->'markets', '{}'::jsonb), 'evidence',s.inputs->'evidence',
            'oi_history', jsonb_build_object('first_at', c.first_at, 'last_at', c.last_at, 'sample_days', c.days)
         ) ORDER BY a.idx)
         FROM unnest($1::text[]) WITH ORDINALITY AS a(asset, idx)
         LEFT JOIN LATERAL (
             SELECT * FROM llf_snapshots WHERE factor_name = 'llf_v0_2' AND asset = a.asset AND formula_status='confirmed'
             ORDER BY observed_at DESC LIMIT 1
         ) s ON true
         LEFT JOIN LATERAL (
             SELECT min(first_at) AS first_at, max(last_at) AS last_at, count(*) AS days
             FROM llf_oi_coverage WHERE asset = a.asset AND sample_day >= (now() AT TIME ZONE 'UTC')::date - 29
         ) c ON true"#,
    ).bind(ASSETS.as_slice()).fetch_one(db).await
}
