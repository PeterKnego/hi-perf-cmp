//! smr-collections **live_stw_churn** — writer-observed latency under the
//! churn workload while stop-the-world snapshots run inline at a fixed
//! cadence. The op that triggers a snapshot pays the whole serialize
//! (writer_max is the stall); the per-op split shows which op absorbed it.

use bench_common::smrcoll::{SmrConfig, emit_int, emit_latency, emit_live, rss_bytes};
use smr_collections_common::book::Book;
use smr_collections_common::churn::{Churn, ChurnOp, ChurnSamples};
use smr_collections_common::snapshot::encode;
use std::time::Instant;

const EXPERIMENT: &str = "live_stw_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = Book::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    for _ in 0..cfg.warmup {
        let op = churn.next_op();
        Churn::apply(&mut book, op);
    }
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];
    // warm the encode path + buffer pages so the k=0 trigger measures
    // steady-state stall, not first-touch cost
    encode(&book, &mut buf);

    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut snap_ns: Vec<u64> = Vec::with_capacity(cfg.live_iters / cfg.snap_every + 1);
    let mut snap_len = 0usize;
    let mut s = ChurnSamples::default();
    let mut rss_peak = rss_bytes();
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let op = churn.next_op();
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            snap_len = encode(&book, &mut buf);
            snap_ns.push(t0.elapsed().as_nanos() as u64);
            rss_peak = rss_peak.max(rss_bytes());
        }
        Churn::apply(&mut book, op);
        let ns = t0.elapsed().as_nanos() as u64;
        *w = ns;
        match op {
            ChurnOp::Insert { .. } => s.insert_ns.push(ns),
            ChurnOp::Cancel(_) => s.cancel_ns.push(ns),
            ChurnOp::Fill(_) => s.fill_ns.push(ns),
        }
    }
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, 0, snap_len);
    if !s.insert_ns.is_empty() {
        emit_latency(EXPERIMENT, "insert", &s.insert_ns);
    }
    if !s.cancel_ns.is_empty() {
        emit_latency(EXPERIMENT, "cancel", &s.cancel_ns);
    }
    if !s.fill_ns.is_empty() {
        emit_latency(EXPERIMENT, "fill", &s.fill_ns);
    }
    emit_int(EXPERIMENT, "rss_peak_bytes", rss_peak, "bytes", 1);
}
