//! LLF 的公共 OKX 行情来源。数据库保存证据，Redis 仅缓存已经归档的最新行。
use crate::collector::MarketCollector;
use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use redis::aio::ConnectionManager;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::{interval, sleep, timeout, Instant, MissedTickBehavior};
use tokio_tungstenite::{
    tungstenite::{
        handshake::{client::generate_key, derive_accept_key},
        protocol::Role,
        Message,
    },
    WebSocketStream,
};
use uuid::Uuid;

const SYMBOLS: [&str; 6] = [
    "BTC-USDT",
    "ETH-USDT",
    "SOL-USDT",
    "BTC-USDT-SWAP",
    "ETH-USDT-SWAP",
    "SOL-USDT-SWAP",
];
const SWAPS: [&str; 3] = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"];
const PUBLIC_WS: &str = "https://ws.okx.com/ws/v5/public";

#[derive(Serialize)]
struct Observation {
    symbol: String,
    channel: String,
    exchange_ts: DateTime<Utc>,
    received_at: DateTime<Utc>,
    payload: Value,
    session_id: Option<Uuid>,
    event_key: String,
    quality: &'static str,
}

fn field_num(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)?
        .as_str()?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn timestamp(value: &Value, key: &str) -> Result<DateTime<Utc>> {
    value[key]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|ms| *ms > 0)
        .and_then(DateTime::from_timestamp_millis)
        .with_context(|| format!("missing OKX timestamp {key}"))
}

fn seq(value: &Value, key: &str) -> Result<i64> {
    value[key]
        .as_i64()
        .or_else(|| value[key].as_str()?.parse().ok())
        .with_context(|| format!("missing OKX sequence {key}"))
}

/// 多实例部署只允许一个采集者。单独持有的 PostgreSQL 连接断开即释放锁；
/// 不把持有会话锁的连接还给连接池。每 5 秒核对连接，数据库断链时终止该订阅。
pub async fn run(collector: Arc<MarketCollector>, db: PgPool, redis: Option<ConnectionManager>) {
    loop {
        if let Err(error) = run_leader(&collector, &db, &redis).await {
            tracing::warn!("LLF public market collector: {error:#}");
        }
        sleep(Duration::from_secs(5)).await;
    }
}

async fn run_leader(
    collector: &MarketCollector,
    db: &PgPool,
    redis: &Option<ConnectionManager>,
) -> Result<()> {
    let mut leader = db.acquire().await?.detach();
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(41349601)")
        .fetch_one(&mut leader)
        .await?;
    if !acquired {
        return Ok(());
    }
    sqlx::query("UPDATE llf_market_session SET connected=false, last_error='collector restarted' WHERE id=1")
        .execute(&mut leader).await?;
    let streams = async {
        loop {
            let session = Uuid::new_v4();
            let result = subscribe(collector, db, redis, session).await;
            let reason = result
                .err()
                .map(|e| format!("{e:#}"))
                .unwrap_or_else(|| "stream closed".into());
            tracing::warn!("LLF OKX public stream disconnected: {reason}");
            sqlx::query(
                "UPDATE llf_market_session SET connected=false, last_error=$2 WHERE session_id=$1",
            )
            .bind(session)
            .bind(reason)
            .execute(db)
            .await?;
            sleep(Duration::from_secs(3)).await;
        }
        #[allow(unreachable_code)]
        Ok(())
    };
    tokio::select! {
        result = streams => result,
        result = leader_heartbeat(&mut leader) => result,
        _ = rest_loop(collector, db, redis, "instruments", 600) => Ok(()),
        _ = rest_loop(collector, db, redis, "funding-rate-history", 60) => Ok(()),
    }
}

async fn leader_heartbeat(conn: &mut PgConnection) -> Result<()> {
    loop {
        sleep(Duration::from_secs(5)).await;
        timeout(
            Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(&mut *conn),
        )
        .await??;
    }
}

/// 合约信息无行情时间戳，以 received_at 记录实际观测时间。资金费只归档交易所
/// 已公布且已到结算时间的 realizedRate，避免将预测 fundingRate 当成账户现金流。
async fn rest_loop(
    collector: &MarketCollector,
    db: &PgPool,
    redis: &Option<ConnectionManager>,
    channel: &str,
    seconds: u64,
) {
    let mut tick = interval(Duration::from_secs(seconds));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        for symbol in SWAPS {
            let result: Result<()> = async {
                let params = if channel == "instruments" {
                    "instType=SWAP"
                } else {
                    "limit=100"
                };
                let url = format!(
                    "https://openapi.okx.com/api/v5/public/{channel}?instId={symbol}&{params}"
                );
                let body: Value = collector
                    .http_get(&url)
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                ensure!(
                    body["code"].as_str() == Some("0"),
                    "OKX response code {}",
                    body["code"]
                );
                let received_at = Utc::now();
                let mut observations = Vec::new();
                for value in body["data"].as_array().context("no OKX response data")? {
                    ensure!(value["instId"] == symbol, "unexpected instrument");
                    let (exchange_ts, valid) = if channel == "instruments" {
                        (
                            received_at,
                            field_num(value, "ctVal").is_some_and(|n| n > 0.0)
                                && field_num(value, "lotSz").is_some_and(|n| n > 0.0)
                                && field_num(value, "minSz").is_some_and(|n| n > 0.0)
                                && value["ctValCcy"].as_str() == symbol.split('-').next()
                                && value["ctType"] == "linear"
                                && value["state"] == "live",
                        )
                    } else {
                        let settled = timestamp(value, "fundingTime")?;
                        if settled > received_at || field_num(value, "realizedRate").is_none() {
                            continue;
                        }
                        (settled, true)
                    };
                    observations.push(Observation {
                        symbol: symbol.into(),
                        channel: channel.into(),
                        exchange_ts,
                        received_at,
                        payload: value.clone(),
                        session_id: None,
                        event_key: exchange_ts.timestamp_millis().to_string(),
                        quality: if valid { "ok" } else { "invalid_payload" },
                    });
                }
                persist(db, redis, &observations).await
            }
            .await;
            if let Err(e) = result {
                tracing::warn!("LLF {symbol} {channel}: {e:#}");
            }
        }
    }
}

/// 复用系统代理、TLS 验证和连接配置；升级公开通道无需交易凭据。
async fn subscribe(
    collector: &MarketCollector,
    db: &PgPool,
    redis: &Option<ConnectionManager>,
    session: Uuid,
) -> Result<()> {
    sqlx::query("INSERT INTO llf_market_session(id,session_id,connected,connected_at) VALUES(1,$1,false,now())
        ON CONFLICT(id) DO UPDATE SET session_id=$1,connected=false,connected_at=now()")
        .bind(session).execute(db).await?;
    let key = generate_key();
    let response = collector
        .get_http_client()
        .await
        .get(PUBLIC_WS)
        .version(reqwest::Version::HTTP_11)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", &key)
        .send()
        .await?;
    let expected = derive_accept_key(key.as_bytes());
    ensure!(
        response.status() == reqwest::StatusCode::SWITCHING_PROTOCOLS
            && response
                .headers()
                .get("Sec-WebSocket-Accept")
                .and_then(|v| v.to_str().ok())
                == Some(expected.as_str()),
        "public WebSocket upgrade failed, status {}",
        response.status()
    );
    let mut socket =
        WebSocketStream::from_raw_socket(response.upgrade().await?, Role::Client, None).await;

    let mut args = vec![json!({"channel":"liquidation-orders", "instType":"SWAP"})];
    for symbol in SYMBOLS {
        for channel in ["trades", "books5", "books"] {
            args.push(json!({"channel":channel,"instId":symbol}));
        }
        if symbol.ends_with("-SWAP") {
            for channel in ["open-interest", "funding-rate", "mark-price"] {
                args.push(json!({"channel":channel,"instId":symbol}));
            }
        }
    }
    let mut pending = args.clone();
    socket
        .send(Message::Text(
            json!({"op":"subscribe","args":args}).to_string().into(),
        ))
        .await?;
    let mut state = MarketSession::default();
    let mut batch = Vec::new();
    let mut flush = interval(Duration::from_millis(200));
    flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let start = Instant::now();
    let mut last_frame = start;
    let mut last_ping = start;
    let result: Result<()> = async { loop {
        tokio::select! {
            _ = flush.tick() => {
                persist(db, redis, &batch).await?;
                batch.clear();
                ensure!(pending.is_empty() || start.elapsed() < Duration::from_secs(25), "subscription acknowledgement timeout");
                ensure!(last_frame.elapsed() < Duration::from_secs(30), "OKX public stream timed out");
                if last_frame.elapsed() >= Duration::from_secs(12) && last_ping.elapsed() >= Duration::from_secs(10) {
                    socket.send(Message::Text("ping".into())).await?;
                    last_ping = Instant::now();
                }
            }
            message = socket.next() => {
                let message = message.context("OKX public stream ended")??;
                last_frame = Instant::now();
                match message {
                    Message::Text(text) if text.as_str() == "pong" => {},
                    Message::Text(text) => {
                        let frame: Value = serde_json::from_str(&text)?;
                        match frame["event"].as_str() {
                            Some("subscribe") => {
                                // 服务可在 ack 中增加元数据，只比较原订阅参数。
                                pending.retain(|request| !request.as_object().unwrap().iter()
                                    .all(|(key,val)| frame["arg"].get(key) == Some(val)));
                                if pending.is_empty() {
                                    sqlx::query("UPDATE llf_market_session SET connected=true,connected_at=now(),last_error=NULL WHERE session_id=$1 AND NOT connected")
                                        .bind(session).execute(db).await?;
                                }
                            }
                            Some("error" | "notice" | "channel-conn-count-error") => bail!("OKX stream control {}", frame),
                            Some(_) => {},
                            None => state.collect(&frame, session, Utc::now(), &mut batch)?,
                        }
                    }
                    Message::Ping(p) => socket.send(Message::Pong(p)).await?,
                    Message::Close(_) => bail!("OKX closed public stream"),
                    _ => {},
                }
            }
        }
        if batch.len() >= 1000 {
            persist(db, redis, &batch).await?;
            batch.clear();
        }
    }}.await;
    // 断线、订阅错误或序列错误前已收到的最后一批数据也必须入档。
    persist(db, redis, &batch).await?;
    result
}

#[derive(Default)]
struct MarketSession {
    books: HashMap<String, DepthBook>,
    last_trade: HashMap<String, i64>,
}

impl MarketSession {
    fn collect(
        &mut self,
        frame: &Value,
        session: Uuid,
        received_at: DateTime<Utc>,
        out: &mut Vec<Observation>,
    ) -> Result<()> {
        let channel = frame["arg"]["channel"]
            .as_str()
            .context("missing OKX channel")?;
        for value in frame["data"].as_array().context("missing OKX data")? {
            let symbol = frame["arg"]["instId"]
                .as_str()
                .or_else(|| value["instId"].as_str())
                .context("missing instrument")?;
            // 清算频道只支持按合约类型订阅，收到其他永续的事件在此排除。
            if !SYMBOLS.contains(&symbol) {
                continue;
            }
            if channel == "liquidation-orders" {
                for detail in value["details"]
                    .as_array()
                    .context("missing liquidation details")?
                {
                    let ts = timestamp(detail, "ts")?;
                    let mut payload = value.clone();
                    payload["details"] = json!([detail]);
                    out.push(Observation {
                        symbol: symbol.into(),
                        channel: channel.into(),
                        exchange_ts: ts,
                        received_at,
                        session_id: Some(session),
                        event_key: format!("{}:{}", ts.timestamp_millis(), detail),
                        quality: if matches!(detail["side"].as_str(), Some("buy" | "sell"))
                            && field_num(detail, "bkPx").is_some_and(|p| p > 0.0)
                            && field_num(detail, "sz").is_some_and(|n| n > 0.0)
                        {
                            "ok"
                        } else {
                            "invalid_payload"
                        },
                        payload,
                    });
                }
                continue;
            }

            let ts = timestamp(value, "ts")?;
            let mut quality = "ok";
            let mut payload = value.clone();
            let mut key = format!("{session}:{}", ts.timestamp_millis());
            let mut reconnect = None;
            match channel {
                "books" => {
                    let action = frame["action"].as_str().context("missing book action")?;
                    let sequence = seq(value, "seqId")?;
                    key = format!("{key}:{sequence}:{action}");
                    payload["action"] = json!(action);
                    let book = self.books.entry(symbol.into()).or_default();
                    match book.apply(action, sequence, value) {
                        Ok(()) => {
                            let depth = book.measure(sequence, &key);
                            let both_covered =
                                depth["bid_covered"] == true && depth["ask_covered"] == true;
                            out.push(Observation {
                                symbol: symbol.into(),
                                channel: "depth-25bp".into(),
                                exchange_ts: ts,
                                received_at,
                                payload: depth,
                                session_id: Some(session),
                                event_key: key.clone(),
                                quality: if both_covered {
                                    "ok"
                                } else {
                                    "insufficient_depth"
                                },
                            });
                        }
                        Err(e) => {
                            quality = "book_gap";
                            reconnect = Some(e);
                        }
                    }
                }
                "books5" => {
                    key = format!("{key}:{}", value["seqId"]);
                    if !valid_snapshot(value) {
                        quality = "invalid_payload";
                    }
                }
                "trades" => {
                    let last = seq(value, "tradeId")?;
                    let count = seq(value, "count")?;
                    ensure!(
                        count > 0 && last >= count,
                        "invalid trade aggregation range"
                    );
                    key = last.to_string();
                    // tradeId 是聚合末笔编号；count 是这组 maker 成交的数量。
                    // 新连接的首笔不证明先前连续。内部断档永久标识，避免把缺失主动单流视作零。
                    if let Some(previous) = self.last_trade.get(symbol) {
                        if last == *previous {
                            continue;
                        }
                        if last != previous.saturating_add(count) {
                            quality = "trade_gap";
                        }
                    }
                    self.last_trade.insert(symbol.into(), last);
                    if !matches!(value["side"].as_str(), Some("buy" | "sell"))
                        || !field_num(value, "px").is_some_and(|p| p > 0.0)
                        || !field_num(value, "sz").is_some_and(|n| n > 0.0)
                    {
                        quality = "invalid_payload";
                    }
                }
                "open-interest"
                    if !field_num(value, "oi").is_some_and(|n| n > 0.0)
                        || !field_num(value, "oiCcy").is_some_and(|n| n > 0.0)
                        || !field_num(value, "oiUsd").is_some_and(|n| n > 0.0) =>
                {
                    quality = "invalid_payload"
                }
                "funding-rate"
                    if field_num(value, "fundingRate").is_none()
                        || timestamp(value, "fundingTime").is_err()
                        || timestamp(value, "nextFundingTime").is_err() =>
                {
                    quality = "invalid_payload"
                }
                "mark-price" if !field_num(value, "markPx").is_some_and(|p| p > 0.0) => {
                    quality = "invalid_payload"
                }
                _ => {}
            }
            if ts > received_at + chrono::Duration::seconds(1) {
                quality = "clock_skew";
            }
            out.push(Observation {
                symbol: symbol.into(),
                channel: channel.into(),
                exchange_ts: ts,
                received_at,
                payload,
                session_id: Some(session),
                event_key: key,
                quality,
            });
            if let Some(e) = reconnect {
                return Err(e);
            }
        }
        Ok(())
    }
}

fn level(row: &Value) -> Result<(f64, f64)> {
    let items = row.as_array().context("invalid OKX book level")?;
    let num = |i: usize| {
        items
            .get(i)?
            .as_str()?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    };
    let price = num(0).filter(|p| *p > 0.0).context("invalid book price")?;
    let size = num(1).filter(|n| *n >= 0.0).context("invalid book size")?;
    Ok((price, size))
}

fn valid_snapshot(value: &Value) -> bool {
    let mut best = [0.0; 2];
    for (i, key) in ["bids", "asks"].iter().enumerate() {
        let Some(rows) = value[*key].as_array() else {
            return false;
        };
        if rows.is_empty() {
            return false;
        }
        let mut previous = None;
        for row in rows {
            let Ok((price, size)) = level(row) else {
                return false;
            };
            if size <= 0.0
                || previous
                    .is_some_and(|last| (i == 0 && price >= last) || (i == 1 && price <= last))
            {
                return false;
            }
            if previous.is_none() {
                best[i] = price;
            }
            previous = Some(price);
        }
    }
    best[0] < best[1]
}

#[derive(Default)]
struct DepthBook {
    bids: Vec<(f64, f64)>, // 双侧均按价格升序，买一在尾、卖一在首
    asks: Vec<(f64, f64)>,
    last_seq: Option<i64>,
}

impl DepthBook {
    fn apply(&mut self, action: &str, sequence: i64, value: &Value) -> Result<()> {
        match action {
            "snapshot" => {
                self.bids.clear();
                self.asks.clear();
            }
            "update" => ensure!(
                self.last_seq == Some(seq(value, "prevSeqId")?),
                "OKX books sequence discontinuity"
            ),
            _ => bail!("unrecognized OKX books action"),
        }
        for (levels, key) in [(&mut self.bids, "bids"), (&mut self.asks, "asks")] {
            for row in value[key].as_array().context("missing depth side")? {
                let (price, size) = level(row)?;
                match levels.binary_search_by(|(px, _)| px.total_cmp(&price)) {
                    Ok(index) if size == 0.0 => {
                        levels.remove(index);
                    }
                    Ok(index) => levels[index].1 = size,
                    Err(index) if size > 0.0 => levels.insert(index, (price, size)),
                    _ => {}
                }
            }
        }
        // 只保留频道公布的前 400 档，边界以外可能未收到后续更新。
        if self.bids.len() > 400 {
            self.bids.drain(..self.bids.len() - 400);
        }
        self.asks.truncate(400);
        ensure!(
            self.bids
                .last()
                .zip(self.asks.first())
                .is_some_and(|(bid, ask)| bid.0 < ask.0),
            "empty or crossed OKX depth book"
        );
        self.last_seq = Some(sequence);
        Ok(())
    }

    fn measure(&self, seq_id: i64, source: &str) -> Value {
        let mid = (self.bids.last().unwrap().0 + self.asks[0].0) / 2.0;
        let lower = mid * (1.0 - 0.0025);
        let upper = mid * (1.0 + 0.0025);
        let bid_covered = self.bids[0].0 <= lower;
        let ask_covered = self.asks.last().unwrap().0 >= upper;
        let depth = |levels: &[(f64, f64)]| {
            levels
                .iter()
                .filter(|(p, _)| *p >= lower && *p <= upper)
                .fold((0.0, 0.0), |(qty, notional), (p, q)| {
                    (qty + q, notional + p * q)
                })
        };
        let (bid_size, bid_price_size) = depth(&self.bids);
        let (ask_size, ask_price_size) = depth(&self.asks);
        // price_size 是原生数量 × 价格；对 USDT 永续乘合约面值才是 USDT 深度。
        json!({"mid":mid,"bid_covered":bid_covered,"ask_covered":ask_covered,
            "bid_size":bid_covered.then_some(bid_size),"ask_size":ask_covered.then_some(ask_size),
            "bid_price_size":bid_covered.then_some(bid_price_size),"ask_price_size":ask_covered.then_some(ask_price_size),
            "seqId":seq_id,"books_event_key":source,"event_depth":true})
    }
}

/// 批量写入一个 SQL 语句，逐笔/盘口原文、异常记录和最新指针共同提交。
/// 重放的交易或已结算费率按业务 ID 去重；接收时间相同时按交易所时间保留最新值。
async fn persist(
    db: &PgPool,
    redis: &Option<ConnectionManager>,
    observations: &[Observation],
) -> Result<()> {
    if observations.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(r#"
        WITH archived AS (
            INSERT INTO llf_market_data(symbol,channel,exchange_ts,received_at,payload,session_id,event_key,quality)
            SELECT symbol,channel,exchange_ts,received_at,payload,session_id,event_key,quality
            FROM jsonb_to_recordset($1) AS e(symbol text,channel text,exchange_ts timestamptz,
                received_at timestamptz,payload jsonb,session_id uuid,event_key text,quality text)
            ON CONFLICT(symbol,channel,event_key) DO NOTHING RETURNING *
        ), gaps AS (
            INSERT INTO llf_market_gaps(symbol,channel,session_id,detected_at,reason)
            SELECT symbol,channel,session_id,received_at,quality FROM archived
            WHERE channel IN ('books','trades') AND quality <> 'ok'
        ), newest AS (
            SELECT DISTINCT ON(symbol,channel) * FROM archived
            ORDER BY symbol,channel,received_at DESC,exchange_ts DESC,id DESC
        ), updated AS (
            INSERT INTO llf_market_latest(symbol,channel,raw_id,exchange_ts,received_at,payload,session_id,quality)
            SELECT symbol,channel,id,exchange_ts,received_at,payload,session_id,quality FROM newest
            ON CONFLICT(symbol,channel) DO UPDATE SET raw_id=EXCLUDED.raw_id,exchange_ts=EXCLUDED.exchange_ts,
                received_at=EXCLUDED.received_at,payload=EXCLUDED.payload,session_id=EXCLUDED.session_id,quality=EXCLUDED.quality
            WHERE EXCLUDED.received_at >= llf_market_latest.received_at AND
                (EXCLUDED.session_id IS DISTINCT FROM llf_market_latest.session_id
                 OR EXCLUDED.exchange_ts >= llf_market_latest.exchange_ts OR EXCLUDED.quality <> 'ok'
                 OR llf_market_latest.quality <> 'ok')
            RETURNING *
        ) SELECT symbol,channel,to_jsonb(updated) AS entry FROM updated WHERE channel <> 'books'
    "#).bind(serde_json::to_value(observations)?).fetch_all(db).await?;
    if let Some(mut conn) = redis.clone() {
        if !rows.is_empty() {
            let mut pipe = redis::pipe();
            for row in rows {
                let symbol: String = row.try_get("symbol")?;
                let channel: String = row.try_get("channel")?;
                let entry: Value = row.try_get("entry")?;
                pipe.cmd("SET")
                    .arg(format!("llf:okx:{symbol}:{channel}"))
                    .arg(entry.to_string())
                    .arg("EX")
                    .arg(1800)
                    .ignore();
            }
            // 可选缓存故障不能阻塞实时流或令已提交证据回滚；读取端依然校验源时间。
            match timeout(
                Duration::from_millis(100),
                pipe.query_async::<()>(&mut conn),
            )
            .await
            {
                Ok(Ok(())) => {}
                _ => tracing::debug!("LLF Redis latest cache unavailable"),
            }
        }
    }
    Ok(())
}
