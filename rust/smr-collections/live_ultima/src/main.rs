//! smr-collections **live_ultima** — writer-observed latency while a
//! serializer thread encodes a pinned old version concurrently. Capture is
//! O(1): the writer just hands over its last committed version number.

use bench_common::smrcoll::{SmrConfig, emit_live};
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::{UltimaBook, encode_at};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

const EXPERIMENT: &str = "live_ultima";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;
    for _ in 0..cfg.warmup {
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
    }

    let busy = Arc::new(AtomicBool::new(false));
    let busy_ser = Arc::clone(&busy);
    let store = Arc::clone(&book.store);
    let buf_len = 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32;
    let (tx, rx) = mpsc::sync_channel::<(u64, Instant)>(1);
    let ser = std::thread::spawn(move || {
        let mut buf = vec![0u8; buf_len];
        let mut durations: Vec<u64> = Vec::new();
        let mut len = 0usize;
        for (version, t0) in rx {
            len = encode_at(&store, version, &mut buf);
            durations.push(t0.elapsed().as_nanos() as u64);
            busy_ser.store(false, Ordering::Release);
        }
        (durations, len)
    });

    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut skipped = 0u64;
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            if busy.load(Ordering::Acquire) {
                skipped += 1;
            } else {
                busy.store(true, Ordering::Relaxed);
                tx.send((book.current_version(), t0))
                    .expect("serializer alive");
            }
        }
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
        *w = t0.elapsed().as_nanos() as u64;
    }
    drop(tx);
    let (snap_ns, snap_len) = ser.join().expect("serializer join");
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, skipped, snap_len);
}
