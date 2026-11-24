use anyhow::{ensure, Context, Result};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::mpsc, time};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{now, ASSETS, MARKETS};

const LEVELS: usize = 400;
const BAND: f64 = 0.0002;

#[derive(Clone, Copy, Debug)]
pub struct Quote {
    pub ts: i64,
    pub received: i64,
    pub seq: i64,
    pub mid: f64,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub far_bid: f64,
    pub far_ask: f64,
}

impl Quote {
    pub fn depth(self) -> Option<[f64; 3]> {
        let (bid, ask) = (self.bid?, self.ask?);
        Some([bid, ask, bid + ask])
    }
}

pub enum Frame {
    Connected,
    Down(String),
    Ack(bool),
    Heartbeat,
    Book(Quote),
    Trade {
        stamp: i64,
        last_id: i64,
        count: i64,
    },
}

pub type Event = (usize, i64, Frame);

async fn spec(client: &reqwest::Client, index: usize) -> Result<(f64, f64, Value)> {
    let spec = client
        .get("https://www.okx.com/api/v5/public/instruments")
        .query(&[("instType", "SWAP"), ("instId", MARKETS[index])])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let row = &spec["data"][0];
    ensure!(
        spec["code"] == "0"
            && row["instId"] == MARKETS[index]
            && row["ctType"] == "linear"
            && row["ctValCcy"] == ASSETS[index]
            && row["settleCcy"] == "USDT"
            && row["state"] == "live",
        "Unusable linear contract {}",
        MARKETS[index]
    );
    let unit = positive(row["ctVal"].as_str().context("ctVal")?)?
        * positive(row["ctMult"].as_str().context("ctMult")?)?;
    ensure!(unit.is_finite() && unit > 0., "Invalid contract multiplier");
    let tick = positive(row["tickSz"].as_str().context("tickSz")?)?;
    Ok((unit, tick, row.clone()))
}

pub async fn specs() -> Result<([(f64, f64); 3], Value)> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(18))
        .build()?;
    let result = tokio::try_join!(spec(&client, 0), spec(&client, 1), spec(&client, 2))?;
    let [(u0, t0, s0), (u1, t1, s1), (u2, t2, s2)] = [result.0, result.1, result.2];
    Ok(([(u0, t0), (u1, t1), (u2, t2)], json!([s0, s1, s2])))
}

fn positive(raw: &str) -> Result<f64> {
    let value: f64 = raw.parse()?;
    ensure!(value.is_finite() && value > 0., "Invalid numeric field");
    Ok(value)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Book {
    ts: String,
    prev_seq_id: i64,
    seq_id: i64,
    bids: Vec<[String; 4]>,
    asks: Vec<[String; 4]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    inst_id: String,
    ts: String,
    trade_id: String,
    count: String,
}

#[derive(Default)]
struct Depth {
    bids: BTreeMap<i64, f64>,
    asks: BTreeMap<i64, f64>,
    seq: Option<i64>,
    ts: Option<i64>,
}

fn levels(
    side: &mut BTreeMap<i64, f64>,
    rows: Vec<[String; 4]>,
    tick: f64,
    bid: bool,
) -> Result<()> {
    for [price, qty, _, _] in rows {
        let index = positive(&price)? / tick;
        ensure!(
            (index - index.round()).abs() <= 1e-6 && index < i64::MAX as f64,
            "Off-tick book level"
        );
        let qty: f64 = qty.parse()?;
        ensure!(qty.is_finite() && qty >= 0., "Invalid book quantity");
        if qty == 0. {
            side.remove(&(index.round() as i64));
        } else {
            side.insert(index.round() as i64, qty);
        }
    }
    while side.len() > LEVELS {
        if bid {
            side.pop_first();
        } else {
            side.pop_last();
        }
    }
    Ok(())
}

impl Depth {
    fn apply(
        &mut self,
        row: Book,
        action: &str,
        received: i64,
        unit: f64,
        tick: f64,
    ) -> Result<Quote> {
        let ts: i64 = row.ts.parse()?;
        ensure!(
            ts > 0 && received >= ts - 1_000,
            "Book exchange clock ahead"
        );
        ensure!(self.ts.is_none_or(|old| ts >= old), "Book time reversed");
        match action {
            "snapshot" if row.prev_seq_id == -1 && self.seq.is_none() => {}
            "update" if self.seq == Some(row.prev_seq_id) => {
                ensure!(
                    row.seq_id != row.prev_seq_id || (row.bids.is_empty() && row.asks.is_empty()),
                    "Book changed with unchanged sequence"
                );
            }
            _ => anyhow::bail!("Book sequence link or snapshot invalid"),
        }
        levels(&mut self.bids, row.bids, tick, true)?;
        levels(&mut self.asks, row.asks, tick, false)?;
        let best_bid = *self.bids.last_key_value().context("Empty bid side")?.0;
        let best_ask = *self.asks.first_key_value().context("Empty ask side")?.0;
        ensure!(best_ask > best_bid, "Crossed book");
        let mid_ticks = (best_bid as f64 + best_ask as f64) / 2.;
        let lower = mid_ticks * (1. - BAND);
        let upper = mid_ticks * (1. + BAND);
        let worst_bid = *self.bids.first_key_value().context("Empty bids")?.0;
        let worst_ask = *self.asks.last_key_value().context("Empty asks")?.0;
        let sum = |side: &BTreeMap<i64, f64>, left, right| {
            if left > right {
                return 0.;
            }
            side.range(left..=right)
                .map(|(p, q)| *p as f64 * tick * *q * unit)
                .sum::<f64>()
        };
        let bid = (worst_bid as f64 <= lower + 1e-9)
            .then(|| sum(&self.bids, (lower - 1e-9).ceil() as i64, best_bid));
        let ask = (worst_ask as f64 >= upper - 1e-9)
            .then(|| sum(&self.asks, best_ask, (upper + 1e-9).floor() as i64));
        self.seq = Some(row.seq_id);
        self.ts = Some(ts);
        Ok(Quote {
            ts,
            received,
            seq: row.seq_id,
            mid: mid_ticks * tick,
            bid,
            ask,
            far_bid: worst_bid as f64 * tick,
            far_ask: worst_ask as f64 * tick,
        })
    }
}

async fn connect(index: usize, out: &mpsc::Sender<Event>, unit: f64, tick: f64) -> Result<()> {
    let (mut socket, _) = time::timeout(
        Duration::from_secs(10),
        connect_async("wss://ws.okx.com/ws/v5/public"),
    )
    .await??;
    out.send((index, now(), Frame::Connected)).await?;
    socket.send(Message::text(json!({"op":"subscribe","args":[
        {"channel":"books","instId":MARKETS[index]}, {"channel":"trades","instId":MARKETS[index]}
    ]}).to_string())).await?;
    let mut book = Depth::default();
    let mut ack = [false; 2];
    let mut ping = time::interval(Duration::from_secs(3));
    let mut last = time::Instant::now();
    let opened = last;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                ensure!(last.elapsed() < Duration::from_secs(8), "Transport silence");
                ensure!(ack == [true, true] || opened.elapsed() < Duration::from_secs(10), "Subscription timeout");
                socket.send(Message::text("ping")).await?;
            }
            message = socket.next() => {
                let received = now();
                let text = match message.context("Socket closed")?? {
                    Message::Text(text) => text,
                    Message::Close(_) => return Ok(()),
                    _ => continue,
                };
                last = time::Instant::now();
                let frame = if text == "pong" { Frame::Heartbeat } else {
                    let packet: Value = serde_json::from_str(&text)?;
                    let channel = packet["arg"]["channel"].as_str();
                    ensure!(packet["event"] != "error", "OKX rejected subscription");
                    ensure!(packet["arg"]["instId"] == MARKETS[index], "Unexpected instrument");
                    let quotes = match channel {
                        Some("books") => true,
                        Some("trades") => false,
                        _ => anyhow::bail!("Unexpected channel"),
                    };
                    let which = usize::from(!quotes);
                    if packet["event"] == "subscribe" {
                        ack[which] = true;
                        Frame::Ack(quotes)
                    } else {
                        ensure!(ack[which], "Data before subscription");
                        let mut rows = packet["data"].as_array().context("Missing data")?.iter();
                        let row = rows.next().context("Empty data")?;
                        ensure!(rows.next().is_none(), "Multiple records in an OKX packet");
                        if quotes {
                            Frame::Book(book.apply(serde_json::from_value(row.clone())?,
                                packet["action"].as_str().context("Book action")?, received, unit, tick)?)
                        } else {
                            let trade: Trade = serde_json::from_value(row.clone())?;
                            ensure!(trade.inst_id == MARKETS[index], "Unexpected trade instrument");
                            Frame::Trade { stamp: trade.ts.parse()?, last_id: trade.trade_id.parse()?,
                                count: trade.count.parse()? }
                        }
                    }
                };
                out.send((index, received, frame)).await?;
            }
        }
    }
}

pub async fn feed(index: usize, out: mpsc::Sender<Event>, (unit, tick): (f64, f64)) {
    time::sleep(Duration::from_millis(index as u64 * 550)).await;
    while !out.is_closed() {
        let reason = match connect(index, &out, unit, tick).await {
            Ok(()) => "socket_closed".into(),
            Err(error) => error.to_string(),
        };
        tracing::warn!(asset = ASSETS[index], %reason, "VET quote/trade connection lost");
        if out.send((index, now(), Frame::Down(reason))).await.is_err() {
            break;
        }
        time::sleep(Duration::from_secs(3 + index as u64)).await;
    }
}
