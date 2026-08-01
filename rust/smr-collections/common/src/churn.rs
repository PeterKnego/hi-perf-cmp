//! The churn workload: a deterministic insert/cancel/fill stream at a
//! configurable order-to-trade ratio (default 1 %, the real-exchange figure).
//! Op *generation* is deliberately outside the timed region — the driver
//! produces an op, the caller times only the store's application of it, so
//! per-op numbers stay comparable with the insert/update cells.

use crate::book::workload::next_insert;
use crate::rng::{SEED, SplitMix};
use bench_common::smrcoll::{SmrConfig, emit_int, emit_latency};
use std::time::Instant;

/// The three ops a churn stream drives into a store.
pub trait ChurnStore {
    fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8);
    fn cancel(&mut self, order_id: i64);
    fn fill(&mut self, order_id: i64);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChurnOp {
    Insert {
        order_id: i64,
        price: i64,
        qty: i64,
        side: u8,
    },
    Cancel(i64),
    Fill(i64),
}

pub struct Churn {
    rng: SplitMix,
    /// Order IDs currently resting, dense so a victim is one uniform draw.
    live: Vec<i64>,
    /// Global op index: drives both the insert/depart alternation and the
    /// order ID, so IDs are sparse (1, 3, 5, …) but never reused.
    i: usize,
    otr_bps: u64,
    levels: u32,
    tick: i64,
    price_min: i64,
}

impl Churn {
    pub fn new(cfg: &SmrConfig) -> Churn {
        Churn {
            rng: SplitMix::new(SEED),
            live: Vec::with_capacity(cfg.cap),
            i: 0,
            otr_bps: cfg.otr_bps,
            levels: cfg.levels,
            tick: cfg.tick,
            price_min: cfg.price_min,
        }
    }

    fn insert_op(&mut self) -> ChurnOp {
        let ins = next_insert(
            &mut self.rng,
            self.i,
            self.levels,
            self.tick,
            self.price_min,
        );
        self.i += 1;
        self.live.push(ins.order_id);
        ChurnOp::Insert {
            order_id: ins.order_id,
            price: ins.price,
            qty: ins.qty,
            side: ins.side,
        }
    }

    /// The next op. Even index inserts, odd index departs; the departure is a
    /// fill with probability `otr_bps / 10_000`, otherwise a cancel.
    pub fn next_op(&mut self) -> ChurnOp {
        if self.i.is_multiple_of(2) || self.live.is_empty() {
            return self.insert_op();
        }
        self.i += 1;
        let v = (self.rng.next() % self.live.len() as u64) as usize;
        let id = self.live[v];
        let is_fill = self.rng.next() % 10_000 < self.otr_bps;
        self.live.swap_remove(v);
        if is_fill {
            ChurnOp::Fill(id)
        } else {
            ChurnOp::Cancel(id)
        }
    }

    /// Bring the store to its steady-state live set with inserts only.
    pub fn prebuild<S: ChurnStore>(&mut self, store: &mut S, steady: usize) {
        for _ in 0..steady {
            let op = self.insert_op();
            Churn::apply(store, op);
        }
    }

    pub fn apply<S: ChurnStore>(store: &mut S, op: ChurnOp) {
        match op {
            ChurnOp::Insert {
                order_id,
                price,
                qty,
                side,
            } => store.insert(order_id, price, qty, side),
            ChurnOp::Cancel(id) => store.cancel(id),
            ChurnOp::Fill(id) => store.fill(id),
        }
    }
}

impl ChurnStore for crate::book::Book {
    fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        crate::book::Book::insert(self, order_id, price, qty, side)
    }
    fn cancel(&mut self, order_id: i64) {
        crate::book::Book::cancel(self, order_id)
    }
    fn fill(&mut self, order_id: i64) {
        crate::book::Book::fill(self, order_id)
    }
}

#[derive(Default)]
pub struct ChurnSamples {
    pub insert_ns: Vec<u64>,
    pub cancel_ns: Vec<u64>,
    pub fill_ns: Vec<u64>,
}

/// Warm up, then time `cfg.iters` ops into per-op-type sample vectors.
/// Only the store call is inside the clock.
pub fn run_churn<S: ChurnStore>(cfg: &SmrConfig, store: &mut S, churn: &mut Churn) -> ChurnSamples {
    for _ in 0..cfg.warmup {
        let op = churn.next_op();
        Churn::apply(store, op);
    }
    let half = cfg.iters / 2 + 1;
    let mut s = ChurnSamples {
        insert_ns: Vec::with_capacity(half),
        cancel_ns: Vec::with_capacity(half),
        fill_ns: Vec::with_capacity(half),
    };
    for _ in 0..cfg.iters {
        let op = churn.next_op();
        let t0 = Instant::now();
        Churn::apply(store, op);
        let ns = t0.elapsed().as_nanos() as u64;
        match op {
            ChurnOp::Insert { .. } => s.insert_ns.push(ns),
            ChurnOp::Cancel(_) => s.cancel_ns.push(ns),
            ChurnOp::Fill(_) => s.fill_ns.push(ns),
        }
    }
    s
}

/// Emit the per-op-type distributions plus RSS growth. A distribution with no
/// samples is skipped rather than emitted as zeros — at `SMRC_OTR_BPS=0` there
/// are no fills, and a fabricated zero would read as a real measurement.
pub fn emit_churn(experiment: &str, s: &ChurnSamples, rss_growth: u64) {
    if !s.insert_ns.is_empty() {
        emit_latency(experiment, "insert", &s.insert_ns);
    }
    if !s.cancel_ns.is_empty() {
        emit_latency(experiment, "cancel", &s.cancel_ns);
    }
    if !s.fill_ns.is_empty() {
        emit_latency(experiment, "fill", &s.fill_ns);
    }
    emit_int(experiment, "rss_growth_bytes", rss_growth, "bytes", 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::snapshot::{encode, restore};

    fn cfg() -> SmrConfig {
        SmrConfig {
            cap: 4096,
            levels: 64,
            tick: 1,
            price_min: 0,
            steady: 2000,
            warmup: 0,
            iters: 0,
            chunk: 4096,
            apply_batch: 64,
            multi_table: false,
            live_iters: 200_000,
            snap_every: 20_000,
            otr_bps: 100,
        }
    }

    #[test]
    fn op_stream_is_deterministic() {
        let c = cfg();
        let (mut a, mut b) = (Churn::new(&c), Churn::new(&c));
        for k in 0..10_000 {
            assert_eq!(a.next_op(), b.next_op(), "op {k} diverged");
        }
    }

    #[test]
    fn stream_alternates_and_honours_otr() {
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut store = Book::new(&c);
        ch.prebuild(&mut store, c.steady);
        let (mut ins, mut can, mut fil) = (0usize, 0usize, 0usize);
        for _ in 0..100_000 {
            match ch.next_op() {
                ChurnOp::Insert { .. } => ins += 1,
                ChurnOp::Cancel(_) => can += 1,
                ChurnOp::Fill(_) => fil += 1,
            }
        }
        assert_eq!(ins, 50_000, "half the ops are inserts");
        assert_eq!(can + fil, 50_000, "the other half depart");
        // 100 bps of 50k departures = 500 fills; allow generous sampling slack.
        assert!((300..800).contains(&fil), "fills out of band: {fil}");
    }

    #[test]
    fn live_set_stays_constant() {
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut store = Book::new(&c);
        ch.prebuild(&mut store, c.steady);
        for _ in 0..20_000 {
            let op = ch.next_op();
            Churn::apply(&mut store, op);
        }
        let live = (0..store.hwm())
            .filter(|&s| store.pool[s as usize].order_id != 0)
            .count();
        assert_eq!(live, c.steady, "alternating stream holds the live set flat");
    }

    #[test]
    fn snapshot_restore_replay_is_bit_identical() {
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut hot = Book::new(&c);
        ch.prebuild(&mut hot, c.steady);
        for _ in 0..5_000 {
            let op = ch.next_op();
            Churn::apply(&mut hot, op);
        }
        // Snapshot at N and restore a cold replica from it.
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&hot, &mut buf);
        let mut cold = restore(&buf[..n], &c).expect("restore");
        // Replay the SAME ops N+1..M into both.
        let ops: Vec<ChurnOp> = (0..5_000).map(|_| ch.next_op()).collect();
        for &op in &ops {
            Churn::apply(&mut hot, op);
            Churn::apply(&mut cold, op);
        }
        let mut hot_buf = vec![0u8; 4 * 1024 * 1024];
        let mut cold_buf = vec![0u8; 4 * 1024 * 1024];
        let hn = encode(&hot, &mut hot_buf);
        let cn = encode(&cold, &mut cold_buf);
        assert_eq!(
            &hot_buf[..hn],
            &cold_buf[..cn],
            "restored replica diverged from the never-restarted one"
        );
    }

    #[test]
    fn export_churn_golden_when_requested() {
        if std::env::var("SMRC_WRITE_GOLDEN").is_err() {
            return;
        }
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut b = Book::new(&c);
        ch.prebuild(&mut b, c.steady);
        for _ in 0..10_000 {
            let op = ch.next_op();
            Churn::apply(&mut b, op);
        }
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&b, &mut buf);
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../testdata/golden_churn_snapshot.bin"
            ),
            &buf[..n],
        )
        .expect("write churn golden");
    }
}
