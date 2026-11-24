use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{now_ms, okx::Frame, ASSETS, VERSION};

pub(super) const BIN: i64 = 10_000;
pub(super) const GRACE: i64 = 5_000;
pub(super) const DAY: i64 = 86_400_000;
const WINDOW: i64 = 6 * BIN;
const MIN_HISTORY: usize = (DAY / BIN * 9 / 10) as usize;
const HORIZONS: [i64; 3] = [1, 5, 15];

fn bin(stamp: i64) -> i64 {
    stamp.div_euclid(BIN) * BIN
}

#[derive(Default)]
struct History(VecDeque<(i64, f64)>);

impl History {
    fn percentile(&mut self, end: i64, p: usize) -> Option<f64> {
        while self.0.front().is_some_and(|(time, _)| *time < end - DAY) {
            self.0.pop_front();
        }
        if self.0.len() < MIN_HISTORY {
            return None;
        }
        let mut values: Vec<_> = self.0.iter().map(|(_, value)| *value).collect();
        let rank = (values.len() * p).div_ceil(100) - 1;
        Some(*values.select_nth_unstable_by(rank, f64::total_cmp).1)
    }

    fn add(&mut self, end: i64, value: Option<f64>) {
        if let Some(value) = value {
            self.0.push_back((end, value));
        }
    }

    fn remove(&mut self, start: i64, end: i64) {
        self.0.retain(|(time, _)| *time < start || *time > end);
    }
}

#[derive(Clone, Copy, Default)]
struct Bucket {
    buy: f64,
    sell: f64,
    close: Option<(i64, i64, f64)>,
    healthy: bool,
}

impl Bucket {
    fn volume(self) -> f64 {
        self.buy + self.sell
    }

    fn imbalance(self) -> Option<f64> {
        self.healthy.then(|| {
            if self.volume() == 0.0 {
                0.0
            } else {
                (self.buy - self.sell) / self.volume()
            }
        })
    }

    fn export(self) -> Value {
        json!({"buy_usdt": self.buy, "sell_usdt": self.sell, "imbalance": self.imbalance()})
    }
}

fn common(spot: f64, perp: f64) -> f64 {
    if spot * perp > 0.0 {
        spot.abs().min(perp.abs()).copysign(spot)
    } else {
        0.0
    }
}

#[derive(Default)]
struct Market {
    ack: Option<i64>,
    frame: Option<i64>,
    last_trade: Option<(i64, i64)>,
    buckets: BTreeMap<i64, Bucket>,
    bad: BTreeSet<i64>,
    volume: History,
}

#[derive(Default)]
struct Asset {
    prices: VecDeque<(i64, Option<f64>)>,
    jumps: History,
    volatility: History,
    pending: Vec<Prediction>,
    next_due: Option<i64>,
}

#[derive(Serialize)]
pub(super) struct Sample {
    pub asset: &'static str,
    pub factor_version: &'static str,
    pub factor_time: i64,
    pub available_time: i64,
    pub cdfp_value: Option<f64>,
    pub spot_imbalance: Option<f64>,
    pub perp_imbalance: Option<f64>,
    pub same_direction_bins: Option<i16>,
    pub volume_quality: &'static str,
    pub market_regime: &'static str,
    pub quality_status: &'static str,
    pub momentum: Option<f64>,
    pub abs_return_1m: Option<f64>,
    pub realized_vol_15m: Option<f64>,
    pub thresholds: Value,
    pub bins: Value,
}

#[derive(Clone, Copy, Serialize)]
struct Label {
    target_time: Option<i64>,
    price_time: Option<i64>,
    price_event_time: Option<i64>,
    log_return: Option<f64>,
    completed_time: Option<i64>,
    status: &'static str,
}

impl Label {
    fn new(status: &'static str, completed_time: Option<i64>) -> Self {
        Self {
            target_time: None,
            price_time: None,
            price_event_time: None,
            log_return: None,
            completed_time,
            status,
        }
    }
}

#[derive(Clone)]
pub(super) struct Prediction {
    pub factor_time: i64,
    available: i64,
    pub price_time: Option<i64>,
    pub price_event_time: Option<i64>,
    pub start_price: Option<f64>,
    labels: [Label; 3],
}

impl Prediction {
    pub(super) fn labels(&self) -> Value {
        json!({"1": self.labels[0], "5": self.labels[1], "15": self.labels[2]})
    }

    fn due(&self) -> Option<i64> {
        self.labels
            .iter()
            .find(|label| label.status == "PENDING")
            .map(|label| label.target_time.unwrap_or(self.available))
    }
}

pub(super) fn initial_labels(sample: &Sample) -> Value {
    let (status, completed) = if sample.cdfp_value.is_some() {
        ("PENDING", None)
    } else {
        ("FACTOR_UNAVAILABLE", Some(sample.available_time))
    };
    let label = Label::new(status, completed);
    json!({"1": label, "5": label, "15": label})
}

impl Asset {
    fn resolve(&mut self, received: i64, quote: Option<(i64, f64)>) -> Vec<Prediction> {
        let check = quote.map_or(received - BIN, |(event, _)| event);
        if self.next_due.is_none_or(|due| check < due) {
            return Vec::new();
        }
        let mut updates = Vec::new();
        self.pending.retain_mut(|item| {
            let mut changed = false;
            if let Some(start_price) = item.start_price {
                for label in item
                    .labels
                    .iter_mut()
                    .filter(|label| label.status == "PENDING")
                {
                    let target = label.target_time.expect("target set with start price");
                    if received > target + BIN {
                        label.status = "NO_END_PRICE";
                    } else if let Some((event, price)) = quote.filter(|(event, _)| *event >= target)
                    {
                        label.status = "COMPLETE";
                        label.price_time = Some(received);
                        label.price_event_time = Some(event);
                        label.log_return = Some(price.ln() - start_price.ln());
                    } else {
                        break;
                    }
                    label.completed_time = Some(now_ms().max(received));
                    changed = true;
                }
            } else if received > item.available + BIN {
                item.labels = [Label::new("NO_START_PRICE", Some(now_ms().max(received))); 3];
                changed = true;
            } else if let Some((event, price)) = quote.filter(|(event, _)| *event >= item.available)
            {
                item.price_time = Some(received);
                item.price_event_time = Some(event);
                item.start_price = Some(price);
                for (label, minutes) in item.labels.iter_mut().zip(HORIZONS) {
                    label.target_time = Some(received + minutes * 60_000);
                }
                changed = true;
            }
            if changed {
                updates.push(item.clone());
            }
            item.due().is_some()
        });
        self.next_due = self.pending.iter().filter_map(Prediction::due).min();
        updates
    }
}

pub(super) struct Gap {
    pub asset: usize,
    pub start: i64,
    pub through: i64,
    pub detected: i64,
}

#[derive(Default)]
pub(super) struct Changes {
    pub gaps: Vec<Gap>,
    pub labels: Vec<(usize, Prediction)>,
}

pub(super) struct Study {
    pub end: i64,
    started: i64,
    multipliers: [f64; 6],
    markets: [Market; 6],
    assets: [Asset; 3],
}

impl Study {
    pub(super) fn new(start: i64, multipliers: [f64; 6]) -> Self {
        let end = bin(start) + BIN;
        Self {
            end,
            started: end,
            multipliers,
            markets: std::array::from_fn(|_| Market::default()),
            assets: std::array::from_fn(|_| Asset::default()),
        }
    }

    fn invalidate(&mut self, index: usize, start: i64, end: i64, now: i64, changes: &mut Changes) {
        let market = &mut self.markets[index];
        if start == end && market.bad.contains(&start) {
            return;
        }
        for stamp in (start.max(self.end - WINDOW)..=end).step_by(BIN as usize) {
            market.bad.insert(stamp);
            if let Some(bucket) = market.buckets.get_mut(&stamp) {
                bucket.healthy = false;
            }
        }
        if start < self.end - BIN {
            market.volume.remove(start + BIN, end + BIN);
            if index % 2 == 0 {
                let asset = &mut self.assets[index / 2];
                for (stamp, price) in &mut asset.prices {
                    if start < *stamp && *stamp <= end + BIN {
                        *price = None;
                    }
                }
                asset.jumps.remove(start + BIN, end + BIN + WINDOW);
                asset.volatility.remove(start + BIN, end + BIN + 90 * BIN);
            }
            changes.gaps.push(Gap {
                asset: index / 2,
                start,
                through: end + WINDOW,
                detected: now,
            });
        }
    }

    pub(super) fn receive(&mut self, index: usize, received: i64, frame: Frame) -> Changes {
        let mut changes = Changes::default();
        let market = &mut self.markets[index];
        let trade = match frame {
            Frame::Connected | Frame::Disconnected => {
                market.ack = None;
                market.frame = None;
                market.last_trade = None;
                return changes;
            }
            Frame::Subscribed => {
                market.ack = Some(received);
                market.frame = Some(received);
                return changes;
            }
            Frame::Alive => {
                market.frame = Some(received);
                return changes;
            }
            Frame::Trade(trade) => trade.fields(index, self.multipliers[index]).ok(),
            Frame::Malformed => None,
        };
        if market.ack.is_none() {
            return changes;
        }
        market.frame = Some(received);
        let previous = market.last_trade;
        let Some((id, stamp, buy, price, amount)) =
            trade.filter(|(_, stamp, ..)| *stamp <= received + GRACE)
        else {
            let start = previous.map_or(bin(received), |(_, stamp)| bin(stamp));
            self.invalidate(
                index,
                start,
                bin(received).max(start),
                received,
                &mut changes,
            );
            return changes;
        };
        let start = bin(stamp);
        if let Some((previous_id, previous_stamp)) = previous {
            if previous_id == id {
                return changes;
            }
            if previous_id.checked_add(1) != Some(id) {
                let other = bin(previous_stamp);
                self.invalidate(
                    index,
                    start.min(other),
                    start.max(other),
                    received,
                    &mut changes,
                );
                if id < previous_id {
                    return changes;
                }
            }
        }
        self.markets[index].last_trade = Some((id, stamp));
        if start < self.end - BIN || received > start + BIN + GRACE {
            self.invalidate(index, start, start, received, &mut changes);
            return changes;
        }
        let market = &mut self.markets[index];
        let bucket = market.buckets.entry(start).or_default();
        if buy {
            bucket.buy += amount;
        } else {
            bucket.sell += amount;
        }
        if index % 2 == 0 && !market.bad.contains(&start) {
            if bucket
                .close
                .is_none_or(|(old, old_id, _)| (stamp, id) > (old, old_id))
            {
                bucket.close = Some((stamp, id, price));
            }
            if stamp <= received {
                changes.labels.extend(
                    self.assets[index / 2]
                        .resolve(received, Some((stamp, price)))
                        .into_iter()
                        .map(|item| (index / 2, item)),
                );
            }
        }
        changes
    }

    pub(super) fn expire_labels(&mut self, now: i64) -> Changes {
        let mut changes = Changes::default();
        for (index, asset) in self.assets.iter_mut().enumerate() {
            changes.labels.extend(
                asset
                    .resolve(now, None)
                    .into_iter()
                    .map(|item| (index, item)),
            );
        }
        changes
    }

    pub(super) fn seal(&mut self, available_clock: impl FnOnce() -> i64) -> [Sample; 3] {
        let end = self.end;
        for market in &mut self.markets {
            market.buckets.retain(|stamp, _| *stamp >= end - WINDOW);
            market.bad.retain(|stamp| *stamp >= end - WINDOW);
            let start = end - BIN;
            market.buckets.entry(start).or_default().healthy =
                market.ack.is_some_and(|ack| ack <= start)
                    && market.frame.is_some_and(|frame| frame >= end)
                    && !market.bad.contains(&start);
        }
        let mut samples = std::array::from_fn(|index| self.sample(index));
        let available = available_clock();
        for (asset, sample) in self.assets.iter_mut().zip(&mut samples) {
            sample.available_time = available;
            if sample.cdfp_value.is_some() {
                asset.pending.push(Prediction {
                    factor_time: end,
                    available,
                    price_time: None,
                    price_event_time: None,
                    start_price: None,
                    labels: [Label::new("PENDING", None); 3],
                });
                asset.next_due = Some(asset.next_due.map_or(available, |old| old.min(available)));
            }
        }
        self.end += BIN;
        samples
    }

    fn sample(&mut self, index: usize) -> Sample {
        let end = self.end;
        let warmed = end >= self.started + DAY;
        let [spot, perp] = &mut self.markets[2 * index..2 * index + 2] else {
            unreachable!()
        };
        let buckets: Vec<_> = (end - WINDOW..end)
            .step_by(BIN as usize)
            .map(|stamp| {
                (
                    stamp + BIN,
                    spot.buckets.get(&stamp).copied().unwrap_or_default(),
                    perp.buckets.get(&stamp).copied().unwrap_or_default(),
                )
            })
            .collect();
        let flows = buckets
            .iter()
            .map(|(_, left, right)| Some((left.imbalance()?, right.imbalance()?)))
            .collect::<Option<Vec<_>>>();
        let (score, spot_flow, perp_flow, count) =
            flows.as_ref().map_or((None, None, None, None), |flows| {
                let mut totals = [0.0; 3];
                let mut count = 0;
                for &(left, right) in flows {
                    let contribution = common(left, right);
                    totals[0] += contribution;
                    totals[1] += left;
                    totals[2] += right;
                    count += i16::from(contribution != 0.0);
                }
                (
                    Some(totals[0] / 6.0),
                    Some(totals[1] / 6.0),
                    Some(totals[2] / 6.0),
                    Some(count),
                )
            });
        let &(_, last_spot, last_perp) = buckets.last().expect("six buckets");
        let spot_min = spot.volume.percentile(end, 10);
        let perp_min = perp.volume.percentile(end, 10);
        let volume_quality = match (warmed, score, spot_min, perp_min) {
            (true, Some(_), Some(left), Some(right)) => {
                if buckets
                    .iter()
                    .any(|(_, spot, perp)| spot.volume() < left || perp.volume() < right)
                {
                    "LOW"
                } else {
                    "OK"
                }
            }
            _ => "UNKNOWN",
        };
        for (market, bucket) in [(spot, last_spot), (perp, last_perp)] {
            market.volume.add(
                end,
                (bucket.healthy && bucket.volume() > 0.0).then(|| bucket.volume()),
            );
        }

        let asset = &mut self.assets[index];
        let quote = last_spot
            .close
            .filter(|_| last_spot.healthy)
            .map(|(_, _, price)| price);
        asset.prices.push_back((end, quote));
        if asset.prices.len() > 91 {
            asset.prices.pop_front();
        }
        let momentum = asset
            .prices
            .len()
            .checked_sub(7)
            .and_then(|position| Some(quote?.ln() - asset.prices[position].1?.ln()));
        let jump = momentum.map(f64::abs);
        let prices = &asset.prices;
        let rv = if prices.len() == 91 && prices[0].1.is_some() && quote.is_some() {
            let mut count = 0;
            let sum = prices
                .iter()
                .zip(prices.iter().skip(1))
                .filter_map(|(a, b)| Some((b.1?.ln() - a.1?.ln()).powi(2)))
                .inspect(|_| count += 1)
                .sum::<f64>();
            (count >= 81).then(|| sum.sqrt())
        } else {
            None
        };
        let p99 = asset.jumps.percentile(end, 99);
        let p95 = asset.volatility.percentile(end, 95);
        let market_regime = match (warmed, jump, rv, p99, p95) {
            (true, Some(jump), Some(rv), Some(p99), Some(p95)) if jump < p99 && rv < p95 => {
                "NORMAL"
            }
            (true, Some(_), Some(_), Some(_), Some(_)) => "EXTREME",
            _ => "WARMUP",
        };
        asset.jumps.add(end, jump);
        asset.volatility.add(end, rv);
        Sample {
            asset: ASSETS[index], factor_version: VERSION, factor_time: end, available_time: 0,
            cdfp_value: score, spot_imbalance: spot_flow, perp_imbalance: perp_flow,
            same_direction_bins: count, volume_quality, market_regime, momentum,
            abs_return_1m: jump, realized_vol_15m: rv,
            quality_status: if score.is_some() { "OK" } else if end < self.started + WINDOW - BIN {
                "WARMUP"
            } else { "GAP" },
            thresholds: json!({"abs_return_p99": p99, "rv_p95": p95, "spot_volume_p10": spot_min,
                "perp_volume_p10": perp_min}),
            bins: Value::Array(buckets.iter().map(|(end, left, right)| {
                json!({"end": end, "spot": left.export(), "perp": right.export()})
            }).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(index: usize, id: i64, time: i64, size: &str, side: &str) -> Frame {
        let payload = json!({
            "instId": super::super::MARKETS[index], "tradeId": id.to_string(), "ts": time.to_string(),
            "px": "100", "sz": size, "side": side, "source": "0"
        });
        Frame::Trade(serde_json::from_value(payload).unwrap())
    }

    fn subscribe(study: &mut Study) {
        for market in 0..6 {
            study.receive(market, 1_000, Frame::Subscribed);
        }
    }

    fn seal(study: &mut Study) -> [Sample; 3] {
        let end = study.end;
        for market in 0..6 {
            study.receive(market, end + 100, Frame::Alive);
        }
        study.seal(|| end + GRACE)
    }

    #[test]
    fn common_flow_and_real_zero_are_distinct_from_a_late_trade_gap() {
        let mut study = Study::new(0, [1.0, 0.1, 1.0, 0.1, 1.0, 0.1]);
        subscribe(&mut study);
        for _ in 0..4 {
            seal(&mut study);
        }
        study.receive(0, 49_990, trade(0, 1, 49_980, "1", "buy"));
        study.receive(1, 49_990, trade(1, 1, 49_980, "60", "buy"));
        study.receive(1, 49_995, trade(1, 2, 49_981, "40", "sell"));
        seal(&mut study);
        assert!(seal(&mut study)[0].cdfp_value.is_none());
        let [sample, eth, _] = seal(&mut study);
        assert_eq!(sample.same_direction_bins, Some(1));
        assert!((sample.cdfp_value.unwrap() - 0.2 / 6.0).abs() < 1e-12);
        assert_eq!(sample.bins[3]["perp"]["buy_usdt"], 600.0);
        assert_eq!(eth.cdfp_value, Some(0.0));
        assert_eq!(sample.market_regime, "WARMUP");

        let changes = study.receive(1, 75_001, trade(1, 3, 69_998, "1", "buy"));
        assert_eq!(changes.gaps.len(), 1);
        assert_eq!(changes.gaps[0].start, 60_000);
        assert!(seal(&mut study)[0].cdfp_value.is_none());
    }

    #[test]
    fn labels_use_the_first_eligible_quote_after_actual_availability() {
        let mut study = Study::new(0, [1.0; 6]);
        subscribe(&mut study);
        for _ in 0..6 {
            seal(&mut study);
        }
        let [sample, ..] = seal(&mut study);
        let available = sample.available_time;
        let before = study.receive(0, available + 1, trade(0, 1, available - 1, "1", "buy"));
        assert!(before.labels.is_empty());
        let start = available + 2;
        let accepted = study.receive(0, start, trade(0, 2, start - 1, "1", "buy"));
        let prediction = &accepted.labels[0].1;
        assert_eq!(prediction.price_time, Some(start));
        assert_eq!(prediction.labels[1].target_time, Some(start + 300_000));
        for _ in 0..5 {
            seal(&mut study);
        }
        let end = start + 60_000;
        let completed = study.receive(0, end + 1, trade(0, 3, end, "1", "buy"));
        let prediction = completed
            .labels
            .iter()
            .find(|(_, item)| item.factor_time == sample.factor_time);
        let prediction = &prediction.unwrap().1;
        assert_eq!(prediction.labels[0].status, "COMPLETE");
        assert_eq!(prediction.labels[0].log_return, Some(0.0));
        assert_eq!(prediction.labels[0].price_event_time, Some(end));
        assert_eq!(prediction.labels[1].status, "PENDING");
    }
}
