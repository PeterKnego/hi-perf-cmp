//! smr-collections **live_ultima_churn** — writer-observed latency under the
//! churn workload while a serializer thread encodes a pinned ultima_db
//! version concurrently. Capture is O(1): the writer pins its last committed
//! version (`VersionPin` is `Send`) and hands the pin over; the pin keeps the
//! snapshot alive until the serializer's read-txn opens, so retention stays
//! at the store default (see `live_ultima`). The per-op split shows which op
//! type absorbed the pin cost; `rss_peak_bytes` tracks growth as cancels
//! retire rows ultima never recycles.

use bench_common::smrcoll::{SmrConfig, emit_int, emit_latency, emit_live, rss_bytes};
use smr_collections_common::churn::{Churn, ChurnOp, ChurnSamples};
use smr_collections_ultima::{UltimaBook, VersionPin, encode_at};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

const EXPERIMENT: &str = "live_ultima_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = UltimaBook::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    for _ in 0..cfg.warmup {
        let op = churn.next_op();
        Churn::apply(&mut book, op);
    }

    let busy = Arc::new(AtomicBool::new(false));
    let busy_ser = Arc::clone(&busy);
    let store = Arc::clone(&book.store);
    let buf_len = 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32;
    let (tx, rx) = mpsc::sync_channel::<(VersionPin, Instant)>(1);
    let ser = std::thread::spawn(move || {
        let mut buf = vec![0u8; buf_len];
        let mut durations: Vec<u64> = Vec::new();
        let mut len = 0usize;
        for (pin, t0) in rx {
            len = encode_at(&store, pin.version(), &mut buf);
            durations.push(t0.elapsed().as_nanos() as u64);
            busy_ser.store(false, Ordering::Release);
        }
        (durations, len)
    });

    // Warm handshake: push one untimed pin through the serializer before the
    // timed loop starts, so first-touch page faults of its encode buffer land
    // here instead of in the first timed sample. Same message shape as the
    // timed sends; the main thread drops this sample after join.
    busy.store(true, Ordering::Relaxed);
    tx.send((book.pin_current(), Instant::now()))
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
        let fired = k % cfg.snap_every == 0;
        let t0 = Instant::now();
        let mut captured = false;
        if fired {
            if busy.load(Ordering::Acquire) {
                skipped += 1;
            } else {
                // Relaxed: no other thread observes this store directly — the
                // serializer only learns of it via the channel send below,
                // which already establishes the needed happens-before.
                busy.store(true, Ordering::Relaxed);
                tx.send((book.pin_current(), t0)).expect("serializer alive");
                captured = true;
            }
        }
        Churn::apply(&mut book, op);
        let ns = t0.elapsed().as_nanos() as u64;
        // Sample RSS only AFTER the clock closes, and only on an iteration
        // that actually pinned. rss_bytes() reads /proc/self/statm —
        // microseconds against 50-300 ns ops — so calling it inside the timed
        // region would inflate writer_max, the one metric this cell exists to
        // report precisely. This sample only sees the writer-side pin cost,
        // not the encode: encode_at() runs concurrently on the serializer
        // thread, so growth from it lags behind this point by design; the
        // sample taken after `ser.join()` below catches the final in-flight
        // window's growth.
        if captured {
            rss_peak = rss_peak.max(rss_bytes());
        }
        *w = ns;
        match op {
            ChurnOp::Insert { .. } => s.insert_ns.push(ns),
            ChurnOp::Cancel(_) => s.cancel_ns.push(ns),
            ChurnOp::Fill(_) => s.fill_ns.push(ns),
        }
    }
    drop(tx);
    let (mut snap_ns, snap_len) = ser.join().expect("serializer join");
    // Catch growth from the final in-flight window: the last pin's encode_at()
    // may still have been landing concurrently with the loop above, so only a
    // post-join sample sees its full effect.
    rss_peak = rss_peak.max(rss_bytes());
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
