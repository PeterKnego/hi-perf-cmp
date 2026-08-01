//! smr-collections **live_mvcc_churn** — writer-observed latency under the
//! churn workload while a serializer thread encodes captured CoW roots
//! concurrently. The writer pays only the O(#chunks) capture plus CoW chunk
//! copies as it re-dirties state; the per-op split shows which op absorbed
//! the capture cost.

use bench_common::smrcoll::{SmrConfig, emit_int, emit_latency, emit_live, rss_bytes};
use smr_collections_common::churn::{Churn, ChurnOp, ChurnSamples};
use smr_collections_common::cowbook::{CowBook, Root};
use smr_collections_common::cowsnap::encode_root;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

const EXPERIMENT: &str = "live_mvcc_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    for _ in 0..cfg.warmup {
        let op = churn.next_op();
        Churn::apply(&mut book, op);
    }

    let busy = Arc::new(AtomicBool::new(false));
    let busy_ser = Arc::clone(&busy);
    let buf_len = 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32;
    let (tx, rx) = mpsc::sync_channel::<(Root, Instant)>(1);
    let ser = std::thread::spawn(move || {
        let mut buf = vec![0u8; buf_len];
        let mut durations: Vec<u64> = Vec::new();
        let mut len = 0usize;
        for (root, t0) in rx {
            len = encode_root(&root, &mut buf);
            durations.push(t0.elapsed().as_nanos() as u64);
            busy_ser.store(false, Ordering::Release);
        }
        (durations, len)
    });

    // Warm handshake: push one untimed capture through the serializer before
    // the timed loop starts, so first-touch page faults of its encode buffer
    // land here instead of in the first timed sample. Same message shape as
    // the timed sends; the main thread drops this sample after join.
    busy.store(true, Ordering::Relaxed);
    tx.send((book.capture(), Instant::now()))
        .expect("serializer alive");
    while busy.load(Ordering::Acquire) {
        std::hint::spin_loop();
    }

    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut skipped = 0u64;
    let mut s = ChurnSamples::default();
    let mut rss_peak = rss_bytes();
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let op = churn.next_op();
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            if busy.load(Ordering::Acquire) {
                skipped += 1;
            } else {
                // Relaxed: no other thread observes this store directly — the
                // serializer only learns of it via the channel send below,
                // which already establishes the needed happens-before.
                busy.store(true, Ordering::Relaxed);
                tx.send((book.capture(), t0)).expect("serializer alive");
                rss_peak = rss_peak.max(rss_bytes());
            }
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
    drop(tx);
    let (mut snap_ns, snap_len) = ser.join().expect("serializer join");
    // The first recorded duration is the warm handshake above (the serializer
    // is a single-consumer FIFO over the channel, so send order == process
    // order) — exclude it from the emitted stats and counts.
    if !snap_ns.is_empty() {
        snap_ns.remove(0);
    }
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, skipped, snap_len);
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
