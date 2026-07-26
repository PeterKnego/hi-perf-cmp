//! smr-collections **live_stw** — writer-observed latency while stop-the-world
//! snapshots run inline at a fixed op cadence. The op that triggers a snapshot
//! pays the whole serialize in its own latency (writer_max is the stall).

use bench_common::smrcoll::{SmrConfig, emit_live};
use smr_collections_common::book::Book;
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_common::snapshot::encode;
use std::time::Instant;

const EXPERIMENT: &str = "live_stw";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = Book::new(&cfg);
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
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];
    // warm the encode path + buffer pages so the k=0 trigger measures
    // steady-state stall, not first-touch cost
    encode(&book, &mut buf);
    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut snap_ns: Vec<u64> = Vec::with_capacity(cfg.live_iters / cfg.snap_every + 1);
    let mut snap_len = 0usize;
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            snap_len = encode(&book, &mut buf);
            snap_ns.push(t0.elapsed().as_nanos() as u64);
        }
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
        *w = t0.elapsed().as_nanos() as u64;
    }
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, 0, snap_len);
}
