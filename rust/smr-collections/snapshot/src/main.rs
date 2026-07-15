//! smr-collections **snapshot** — time serialize + restore of a steady book.

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::book::Book;
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_common::snapshot::{encode, restore};
use std::time::Instant;

const EXPERIMENT: &str = "snapshot";

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
    // Buffer sized generously: header + 2*levels + hwm orders, ~64 B/order.
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];

    let mut snap_ns = vec![0u64; cfg.iters];
    let mut rest_ns = vec![0u64; cfg.iters];
    let mut snap_len = 0usize;
    for _ in 0..cfg.warmup {
        snap_len = encode(&book, &mut buf);
        let _ = restore(&buf[..snap_len], &cfg).expect("restore");
    }
    for k in 0..cfg.iters {
        let t0 = Instant::now();
        snap_len = encode(&book, &mut buf);
        snap_ns[k] = t0.elapsed().as_nanos() as u64;
        let t1 = Instant::now();
        let r = restore(&buf[..snap_len], &cfg).expect("restore");
        rest_ns[k] = t1.elapsed().as_nanos() as u64;
        std::hint::black_box(&r);
    }
    emit_latency(EXPERIMENT, "snapshot", &snap_ns);
    emit_latency(EXPERIMENT, "restore", &rest_ns);
    emit_int(EXPERIMENT, "snapshot_bytes", snap_len as u64, "bytes", 1);
    let mean_ns = bench_common::stats::mean(&snap_ns);
    let mbps = (snap_len as f64) / (mean_ns / 1e9);
    emit_float(
        EXPERIMENT,
        "snapshot_throughput",
        mbps,
        "bytes_per_sec",
        cfg.iters,
    );
}
