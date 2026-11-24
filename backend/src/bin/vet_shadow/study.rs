use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::wire::{Frame, Quote};

pub const STEP: i64 = 10_000;
pub const GRACE: i64 = 5_000;
pub const SECOND: i64 = 1_000;
const HORIZONS: [i16; 3] = [1, 5, 15];

pub fn bucket(stamp: i64) -> i64 {
    stamp.div_euclid(STEP) * STEP
}

struct Bin {
    end: i64,
    mids: [Option<f64>; 10],
    depths: Vec<([f64; 3], i64)>,
    quote: Option<Quote>,
    book_ok: bool,
    band_ok: bool,
    trade_ok: bool,
    messages: i64,
    fills: i64,
}

pub struct Sample {
    pub stamp: i64,
    pub available: i64,
    pub a: Option<f64>,
    pub l: Option<f64>,
    pub vet: Option<f64>,
    pub depth: Option<[f64; 3]>,
    pub mid: Option<f64>,
    pub past: [Option<f64>; 3],
    pub status: &'static str,
    pub diagnostics: Value,
}

pub struct Fault {
    pub start: i64,
    pub end: i64,
    pub book: bool,
    pub reason: String,
}

struct Pending {
    stamp: i64,
    origin: i64,
    known_origin: bool,
    next: usize,
}

pub struct Anchor {
    pub stamp: i64,
    pub origin: i64,
    pub price: Option<f64>,
}

pub struct Label {
    pub stamp: i64,
    pub minutes: i16,
    pub target: i64,
    pub variance: Option<f64>,
    pub status: &'static str,
}

#[derive(Default)]
pub struct Outcomes {
    pub anchors: Vec<Anchor>,
    pub labels: Vec<Label>,
}

pub struct Market {
    start: i64,
    next: i64,
    book_ack: Option<i64>,
    trade_ack: Option<i64>,
    frame: Option<i64>,
    quotes: BTreeMap<i64, Vec<Quote>>,
    counts: BTreeMap<i64, (i64, i64)>,
    bad_books: BTreeSet<i64>,
    bad_trades: BTreeSet<i64>,
    carry: Option<Quote>,
    last_book: Option<i64>,
    last_trade: Option<(i64, i64)>,
    bins: VecDeque<Bin>,
    pending: Vec<Pending>,
    disruptions: VecDeque<(i64, String)>,
}

impl Market {
    pub fn new(started: i64) -> Self {
        let start = bucket(started);
        Self {
            start,
            next: start + STEP,
            book_ack: None,
            trade_ack: None,
            frame: None,
            quotes: BTreeMap::new(),
            counts: BTreeMap::new(),
            bad_books: BTreeSet::new(),
            bad_trades: BTreeSet::new(),
            carry: None,
            last_book: None,
            last_trade: None,
            bins: VecDeque::new(),
            pending: Vec::new(),
            disruptions: VecDeque::new(),
        }
    }

    fn mark(&mut self, from: i64, to: i64, book: bool, reason: String) -> Option<Fault> {
        let start = bucket(from.min(to)).max(self.start);
        let end = bucket(from.max(to));
        let bad = if book {
            &mut self.bad_books
        } else {
            &mut self.bad_trades
        };
        let remember = (self.next - 1_850_000).max(start);
        let mut fresh = false;
        for slot in (remember..=end).step_by(STEP as usize) {
            fresh |= bad.insert(slot);
        }
        if !fresh && start >= remember {
            return None;
        }
        self.disruptions.push_back((end, reason.clone()));
        for bin in &mut self.bins {
            if start < bin.end && bin.end <= end + STEP {
                if book {
                    bin.book_ok = false;
                    bin.band_ok = false;
                    bin.mids = [None; 10];
                } else {
                    bin.trade_ok = false;
                }
            }
        }
        Some(Fault {
            start,
            end: end + STEP,
            book,
            reason,
        })
    }

    pub fn receive(&mut self, received: i64, frame: Frame) -> Vec<Fault> {
        match frame {
            Frame::Connected => {
                self.book_ack = None;
                self.trade_ack = None;
                self.frame = None;
                self.carry = None;
                self.last_trade = None;
                self.last_book = None;
                self.quotes.clear();
                self.counts.clear();
            }
            Frame::Down(reason) => {
                let last_frame = self.frame.unwrap_or(received).min(received);
                let faults = [
                    self.mark(
                        self.last_book.unwrap_or(last_frame).min(last_frame),
                        received,
                        true,
                        reason.clone(),
                    ),
                    self.mark(
                        self.last_trade
                            .map_or(last_frame, |(_, ts)| ts)
                            .min(last_frame),
                        received,
                        false,
                        reason,
                    ),
                ]
                .into_iter()
                .flatten()
                .collect();
                self.book_ack = None;
                self.trade_ack = None;
                self.carry = None;
                self.frame = None;
                self.last_trade = None;
                self.last_book = None;
                return faults;
            }
            Frame::Ack(book) => {
                if book {
                    self.book_ack = Some(received);
                } else {
                    self.trade_ack = Some(received);
                }
                self.frame = Some(received);
            }
            Frame::Heartbeat => self.frame = Some(received),
            Frame::Book(quote) => {
                self.frame = Some(received);
                let slot = bucket(quote.ts);
                if slot < self.next - STEP || received > slot + STEP + GRACE {
                    return self
                        .mark(
                            quote.ts,
                            self.last_book.unwrap_or(quote.ts),
                            true,
                            "late_book".into(),
                        )
                        .into_iter()
                        .collect();
                }
                self.last_book = Some(quote.ts);
                self.quotes.entry(slot).or_default().push(quote);
            }
            Frame::Trade {
                stamp,
                last_id,
                count,
            } => {
                self.frame = Some(received);
                let slot = bucket(stamp);
                if stamp <= 0 || stamp > received + 1_000 || count <= 0 || last_id < count {
                    return self
                        .mark(received, received, false, "invalid_trade".into())
                        .into_iter()
                        .collect();
                }
                let mut faults = Vec::new();
                if let Some((id, previous)) = self.last_trade {
                    if last_id == id {
                        return faults;
                    }
                    if last_id - count != id || stamp < previous {
                        faults.extend(self.mark(
                            previous,
                            stamp,
                            false,
                            "trade_sequence_or_time_gap".into(),
                        ));
                        if last_id - count < id {
                            return faults;
                        }
                    }
                }
                self.last_trade = Some((last_id, stamp));
                if slot < self.next - STEP || received > slot + STEP + GRACE {
                    faults.extend(self.mark(stamp, stamp, false, "late_trade".into()));
                } else {
                    let entry = self.counts.entry(slot).or_default();
                    entry.0 += 1;
                    entry.1 += count;
                }
                return faults;
            }
        }
        Vec::new()
    }

    pub fn close(&mut self, end: i64) {
        assert_eq!(end, self.next);
        let start = end - STEP;
        let frame = self.frame.is_some_and(|receipt| receipt >= end);
        let book_ok = frame
            && self.book_ack.is_some_and(|receipt| receipt <= start)
            && !self.bad_books.contains(&start)
            && self.carry.is_some();
        let trade_ok = frame
            && self.trade_ack.is_some_and(|receipt| receipt <= start)
            && !self.bad_trades.contains(&start);
        let changes = self.quotes.remove(&start).unwrap_or_default();
        let (messages, fills) = self.counts.remove(&start).unwrap_or_default();
        let mut bin = Bin {
            end,
            mids: [None; 10],
            depths: Vec::new(),
            quote: None,
            book_ok,
            band_ok: book_ok,
            trade_ok,
            messages,
            fills,
        };

        if book_ok {
            let mut state = self.carry.expect("continuous prior book");
            let mut cursor = start;
            for quote in changes.iter().copied() {
                Self::segment(&mut bin, cursor, quote.ts, state);
                state = quote;
                cursor = quote.ts;
            }
            Self::segment(&mut bin, cursor, end, state);
            bin.quote = Some(state);
        }
        if let Some(quote) = changes.last() {
            self.carry = Some(*quote);
        }
        if !frame || self.book_ack.is_none() {
            self.carry = None;
        }
        if !book_ok && changes.is_empty() {
            self.carry = None;
        }

        self.next += STEP;
        self.bins.push_back(bin);
        while self
            .bins
            .front()
            .is_some_and(|bin| bin.end <= end - 1_840_000)
        {
            self.bins.pop_front();
        }
        self.bad_books.retain(|stamp| *stamp >= end - 1_850_000);
        self.bad_trades.retain(|stamp| *stamp >= end - 1_850_000);
        self.disruptions
            .retain(|(stamp, _)| *stamp >= end - 1_850_000);
    }

    fn segment(bin: &mut Bin, start: i64, end: i64, quote: Quote) {
        if end <= start {
            return;
        }
        if let Some(depth) = quote.depth() {
            bin.depths.push((depth, end - start));
        } else {
            bin.band_ok = false;
        }
        let first = (start.div_euclid(SECOND) + 1) * SECOND;
        for time in (first..=end).step_by(SECOND as usize) {
            let index = ((time - (bin.end - STEP)) / SECOND - 1) as usize;
            bin.mids[index] = Some(quote.mid);
        }
    }

    fn mid(&self, stamp: i64) -> Option<f64> {
        let end = bucket(stamp - 1) + STEP;
        let index: usize = ((end - self.bins.front()?.end) / STEP).try_into().ok()?;
        let bin = self.bins.get(index)?;
        let slot = ((stamp - (end - STEP)) / SECOND - 1) as usize;
        bin.mids.get(slot).copied().flatten()
    }

    fn variance(&self, start: i64, end: i64) -> Option<f64> {
        let mut last = self.mid(start)?.ln();
        let mut sum = 0.;
        for stamp in (start + SECOND..=end).step_by(SECOND as usize) {
            let mid = self.mid(stamp)?.ln();
            sum += (mid - last).powi(2);
            last = mid;
        }
        Some(sum)
    }

    fn depth(&self, skip: usize, count: usize) -> Option<[f64; 3]> {
        let bins: Vec<_> = self.bins.iter().rev().skip(skip).take(count).collect();
        if bins.len() != count || bins.iter().any(|bin| !bin.band_ok) {
            return None;
        }
        let mut weights: Vec<_> = bins
            .iter()
            .flat_map(|bin| bin.depths.iter().copied())
            .collect();
        Some(std::array::from_fn(|side| {
            weighted_median(&mut weights, side)
        }))
    }

    fn a(&self) -> Option<f64> {
        let bins: Vec<_> = self.bins.iter().rev().take(66).collect();
        if bins.len() != 66 || bins.iter().any(|bin| !bin.trade_ok) {
            return None;
        }
        let current: i64 = bins[..6].iter().map(|bin| bin.messages).sum();
        let reference: i64 = bins[6..].iter().map(|bin| bin.messages).sum();
        if reference == 0 {
            return None;
        }
        Some(if current > 0 {
            (current as f64 / (reference as f64 / 10.)).ln().max(0.)
        } else {
            0.
        })
    }

    pub fn sample(&self) -> Sample {
        let last = self.bins.back().expect("factor requires a closed bucket");
        let stamp = last.end;
        let current = self.depth(0, 3);
        let base = self.depth(3, 60);
        let a = self.a();
        let l = current.zip(base).and_then(|(near, far)| {
            (near[2] > 0. && far[2] > 0.).then(|| (far[2] / near[2]).ln().max(0.))
        });
        let past = [1, 5, 30].map(|minutes| {
            self.variance(stamp - minutes * 60_000, stamp)
                .map(f64::sqrt)
        });
        let volatility_ready = past.iter().all(Option::is_some);
        let status = match (a, l, volatility_ready) {
            (Some(_), Some(_), true) => "OK",
            (Some(_), Some(_), false) if stamp <= self.start + 1_810_000 => "RV_WARMUP",
            (Some(_), Some(_), false) => "RV_GAP",
            _ if stamp <= self.start + 670_000 => "WARMUP",
            (None, _, _) => "TRADES_UNAVAILABLE",
            _ => "DEPTH_UNAVAILABLE",
        };
        let pattern =
            current
                .zip(base)
                .map(|(near, far)| match (near[0] < far[0], near[1] < far[1]) {
                    (true, true) => "BOTH",
                    (true, false) => "BID_ONLY",
                    (false, true) => "ASK_ONLY",
                    _ => "NEITHER",
                });
        let mid = self.mid(stamp);
        let earlier = self.mid(stamp - 30_000);
        let move_bp = mid
            .zip(earlier)
            .map(|(now, then)| (now.ln() - then.ln()) * 10_000.);
        let quote = last.quote;
        Sample {
            stamp,
            available: 0,
            a,
            l,
            vet: a.zip(l).map(|(a, l)| a * l),
            depth: current,
            mid,
            past,
            status,
            diagnostics: json!({
                "quote_event_ts": quote.map(|q| q.ts), "quote_received_at": quote.map(|q| q.received),
                "quote_sequence": quote.map(|q| q.seq), "furthest_bid": quote.map(|q| q.far_bid),
                "furthest_ask": quote.map(|q| q.far_ask), "book_10s_ok": last.book_ok,
                "last_quote_depth": quote.and_then(Quote::depth),
                "depth_10s_ok": last.band_ok, "trades_10s_ok": last.trade_ok,
                "aggregated_trade_messages_10s": last.messages, "fills_count_10s": last.fills,
                "recent_depth": current, "reference_depth": base, "depth_pattern": pattern,
                "mid_move_bp_30s": move_bp, "band_center_abs_move_bp_30s": move_bp.map(f64::abs),
                "band_lower": mid.map(|price|price*0.9998), "band_upper": mid.map(|price|price*1.0002),
                "band_lower_30s_ago": earlier.map(|price|price*0.9998), "band_upper_30s_ago": earlier.map(|price|price*1.0002),
                "depth_log_ratio": current.zip(base).and_then(|(a,b)| (a[2]>0. && b[2]>0.).then(||(a[2]/b[2]).ln())),
                "recent_faults": self.disruptions.iter().filter(|(time,_)| *time >= stamp-60_000)
                    .map(|(_,reason)| reason).collect::<Vec<_>>(),
            }),
        }
    }

    pub fn arm(&mut self, sample: &Sample, published: i64) {
        if sample.vet.is_some() {
            self.pending.push(Pending {
                stamp: sample.stamp,
                origin: (sample.available.max(published).div_euclid(SECOND) + 1) * SECOND,
                known_origin: false,
                next: 0,
            });
        }
    }

    pub fn outcomes(&mut self, end: i64) -> Outcomes {
        let mut updates = Outcomes::default();
        let mut remaining = Vec::new();
        for mut item in std::mem::take(&mut self.pending) {
            let price = self.mid(item.origin);
            if !item.known_origin && end >= item.origin {
                item.known_origin = true;
                updates.anchors.push(Anchor {
                    stamp: item.stamp,
                    origin: item.origin,
                    price,
                });
            }
            while item.next < HORIZONS.len() {
                let minutes = HORIZONS[item.next];
                let target = item.origin + i64::from(minutes) * 60_000;
                if end < target {
                    break;
                }
                let variance = if price.is_some() {
                    self.variance(item.origin, target)
                } else {
                    None
                };
                updates.labels.push(Label {
                    stamp: item.stamp,
                    minutes,
                    target,
                    variance,
                    status: if price.is_none() {
                        "ORIGIN_MISSING"
                    } else if variance.is_none() {
                        "QUOTE_GAP"
                    } else {
                        "COMPLETE"
                    },
                });
                item.next += 1;
            }
            if item.next < HORIZONS.len() {
                remaining.push(item);
            }
        }
        self.pending = remaining;
        updates
    }
}

fn weighted_median(weights: &mut [([f64; 3], i64)], side: usize) -> f64 {
    weights.sort_unstable_by(|(left, _), (right, _)| left[side].total_cmp(&right[side]));
    let target = (weights.iter().map(|(_, ms)| ms).sum::<i64>() + 1) / 2;
    let mut total = 0;
    for (depth, weight) in weights {
        total += *weight;
        if total >= target {
            return depth[side];
        }
    }
    unreachable!("a book interval with full coverage has positive duration");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(market: &mut Market, slot: i64, ids: &mut i64, depth: f64, n: i64) {
        let ts = slot + 10;
        market.receive(
            ts + 1,
            Frame::Book(Quote {
                ts,
                received: ts + 1,
                seq: slot / STEP,
                mid: 100. + (slot / STEP % 7) as f64 * 0.001,
                bid: Some(depth),
                ask: Some(depth),
                far_bid: 99.,
                far_ask: 101.,
            }),
        );
        for _ in 0..n {
            *ids += 3;
            market.receive(
                ts + 2,
                Frame::Trade {
                    stamp: ts + 1,
                    last_id: *ids,
                    count: 3,
                },
            );
        }
        market.receive(slot + STEP + 1, Frame::Heartbeat);
        market.close(slot + STEP);
    }

    #[test]
    fn two_fixed_windows_and_strictly_delayed_realized_variance_labels() {
        let mut market = Market::new(0);
        let mut ids = 0;
        market.receive(1, Frame::Ack(true));
        market.receive(1, Frame::Ack(false));
        for index in 0..190 {
            feed(
                &mut market,
                index * STEP,
                &mut ids,
                if index >= 187 { 50. } else { 100. },
                if index >= 184 { 2 } else { 1 },
            );
        }
        let mut row = market.sample();
        assert_eq!(row.status, "OK");
        assert_eq!(row.depth, Some([50., 50., 100.]));
        assert!((row.a.unwrap() - 2_f64.ln()).abs() < 1e-12);
        assert!((row.l.unwrap() - 2_f64.ln()).abs() < 1e-12);
        assert!((row.vet.unwrap() - 2_f64.ln().powi(2)).abs() < 1e-12);
        row.available = row.stamp + GRACE;
        market.arm(&row, row.available);
        let origin = row.available + SECOND;
        assert!(market.outcomes(row.stamp).anchors.is_empty());
        let mut got = Vec::new();
        for step in 0..92 {
            let slot = row.stamp + step * STEP;
            feed(&mut market, slot, &mut ids, 100., 1);
            let end = slot + STEP;
            let updates = market.outcomes(end);
            if step == 0 {
                let anchor = &updates.anchors[0];
                assert_eq!(anchor.origin, origin);
                assert!(anchor.price.is_some());
            }
            for label in updates.labels {
                assert_eq!(label.status, "COMPLETE");
                assert_eq!(label.target, origin + i64::from(label.minutes) * 60_000);
                assert!(label.target <= end && label.target > end - STEP);
                assert!(label.variance.unwrap() > 0.);
                got.push(label.minutes);
            }
        }
        assert_eq!(got, vec![1, 5, 15]);
    }

    #[test]
    fn verified_mid_is_missing_after_disconnect_until_a_new_book_arrives() {
        let mut market = Market::new(0);
        let mut ids = 0;
        market.receive(1, Frame::Ack(true));
        market.receive(1, Frame::Ack(false));
        feed(&mut market, 0, &mut ids, 100., 1);
        feed(&mut market, STEP, &mut ids, 100., 1);
        assert!(market.mid(2 * STEP).is_some());
        market.receive(2 * STEP + 1, Frame::Down("test断流".into()));
        market.receive(2 * STEP + 20, Frame::Connected);
        market.receive(2 * STEP + 21, Frame::Ack(true));
        market.receive(2 * STEP + 21, Frame::Ack(false));
        market.receive(3 * STEP + 1, Frame::Heartbeat);
        market.close(3 * STEP);
        market.receive(4 * STEP + 1, Frame::Heartbeat);
        market.close(4 * STEP);
        assert!(market.mid(3 * STEP).is_none() && market.mid(4 * STEP).is_none());
        assert!(market.variance(2 * STEP, 4 * STEP).is_none());
    }
}
