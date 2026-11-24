use anyhow::{ensure, Context, Result};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{sync::mpsc, time};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{now_ms, ASSETS, MARKETS};

const STREAM_URL: &str = "wss://ws.okx.com/ws/v5/business";
const INSTRUMENTS_URL: &str = "https://www.okx.com/api/v5/public/instruments";

pub(super) async fn contracts() -> Result<([f64; 6], Value)> {
    let payload = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()?
        .get(INSTRUMENTS_URL)
        .query(&[("instType", "SWAP")])
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    ensure!(payload["code"] == "0", "OKX instruments request failed");
    let rows = payload["data"]
        .as_array()
        .context("Missing instrument data")?;
    let mut sizes = [1.0; 6];
    let mut specs = Vec::new();
    for (asset_index, asset) in ASSETS.iter().enumerate() {
        let index = 2 * asset_index + 1;
        let spec = rows
            .iter()
            .find(|row| row["instId"] == MARKETS[index])
            .with_context(|| format!("Missing {}", MARKETS[index]))?;
        ensure!(
            spec["state"] == "live"
                && spec["ctType"] == "linear"
                && spec["ctValCcy"] == *asset
                && spec["settleCcy"] == "USDT",
            "Unsupported OKX contract {}",
            MARKETS[index]
        );
        sizes[index] = number(&spec["ctVal"])? * number(&spec["ctMult"])?;
        ensure!(sizes[index].is_finite(), "Invalid contract multiplier");
        specs.push(spec);
    }
    Ok((sizes, json!(specs)))
}

fn number(value: &Value) -> Result<f64> {
    let parsed = value.as_str().context("Missing number")?.parse::<f64>()?;
    ensure!(parsed.is_finite() && parsed > 0.0, "Invalid number");
    Ok(parsed)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Trade {
    inst_id: String,
    trade_id: String,
    ts: String,
    px: String,
    sz: String,
    side: String,
    source: String,
}

impl Trade {
    pub(super) fn fields(
        &self,
        market: usize,
        multiplier: f64,
    ) -> Result<(i64, i64, bool, f64, f64)> {
        let id: i64 = self.trade_id.parse()?;
        let stamp: i64 = self.ts.parse()?;
        let price: f64 = self.px.parse()?;
        let size: f64 = self.sz.parse()?;
        let amount = price * size * multiplier;
        ensure!(
            self.inst_id == MARKETS[market]
                && id > 0
                && stamp > 0
                && matches!(self.side.as_str(), "buy" | "sell")
                && matches!(self.source.as_str(), "0" | "1")
                && price.is_finite()
                && price > 0.0
                && size > 0.0
                && amount.is_finite()
                && amount > 0.0,
            "Invalid trade fields"
        );
        Ok((id, stamp, self.side == "buy", price, amount))
    }
}

pub(super) enum Frame {
    Connected,
    Disconnected,
    Subscribed,
    Alive,
    Trade(Trade),
    Malformed,
}

pub(super) type Event = (usize, i64, Frame);

#[derive(Deserialize)]
struct Packet {
    event: Option<String>,
    arg: Option<Subscription>,
    data: Option<Vec<Trade>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Subscription {
    channel: String,
    inst_id: String,
}

async fn connection(index: usize, out: &mpsc::Sender<Event>) -> Result<()> {
    let (mut socket, _) =
        time::timeout(Duration::from_secs(10), connect_async(STREAM_URL)).await??;
    out.send((index, now_ms(), Frame::Connected)).await?;
    socket
        .send(Message::text(
            json!({
                "op": "subscribe", "args": [{"channel": "trades-all", "instId": MARKETS[index]}]
            })
            .to_string(),
        ))
        .await?;
    let mut ping = time::interval(Duration::from_secs(3));
    let mut last_message = time::Instant::now();
    let mut subscribed = false;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                ensure!(last_message.elapsed() < Duration::from_secs(8), "OKX connection silent");
                socket.send(Message::text("ping")).await?;
            }
            message = socket.next() => {
                let received = now_ms();
                let Some(message) = message else { return Ok(()) };
                let frame = match message? {
                    Message::Text(text) if text == "pong" => Frame::Alive,
                    Message::Text(text) => {
                        let packet = serde_json::from_str::<Packet>(&text);
                        match packet {
                            Ok(packet) => {
                                let expected = packet.arg.is_some_and(|arg| {
                                    arg.channel == "trades-all" && arg.inst_id == MARKETS[index]
                                });
                                match (packet.event.as_deref(), expected, packet.data) {
                                    (Some("subscribe"), true, _) => {
                                        subscribed = true;
                                        Frame::Subscribed
                                    }
                                    (Some("error" | "unsubscribe"), _, _) => {
                                        anyhow::bail!("OKX subscription rejected or removed");
                                    }
                                    (None, true, Some(mut data)) if subscribed && data.len() == 1 => {
                                        Frame::Trade(data.pop().expect("one trade"))
                                    }
                                    _ => Frame::Malformed,
                                }
                            }
                            Err(_) => Frame::Malformed,
                        }
                    }
                    Message::Close(_) => return Ok(()),
                    _ => continue,
                };
                if subscribed {
                    last_message = time::Instant::now();
                }
                out.send((index, received, frame)).await?;
            }
        }
    }
}

pub(super) async fn feed(index: usize, out: mpsc::Sender<Event>) {
    time::sleep(Duration::from_millis(index as u64 * 550)).await;
    while !out.is_closed() {
        if let Err(error) = connection(index, &out).await {
            tracing::warn!(market = MARKETS[index], %error, "CDFP OKX feed disconnected");
        }
        if out
            .send((index, now_ms(), Frame::Disconnected))
            .await
            .is_err()
        {
            break;
        }
        time::sleep(Duration::from_secs(5 + index as u64)).await;
    }
}
