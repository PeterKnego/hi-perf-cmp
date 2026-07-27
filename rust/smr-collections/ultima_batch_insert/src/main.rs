//! smr-collections **ultima_batch_insert** — insert cost through ultima_db
//! with ONE explicit-version write-txn per `apply_batch` commands (the SMR
//! consensus-batch pattern). Compare `per_op_mean` against `ultima_insert`'s
//! `insert_mean`: the difference is pure txn amortization.

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::UltimaBook;
use std::time::Instant;

const EXPERIMENT: &str = "ultima_batch_insert";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let b = cfg.apply_batch;
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    let mut i = 0usize;
    let mut next_cmd = |rng: &mut SplitMix| {
        let ins = next_insert(rng, i, cfg.levels, cfg.tick, cfg.price_min);
        i += 1;
        (ins.order_id, ins.price, ins.qty, ins.side)
    };

    let warm_batches = cfg.warmup / b;
    for _ in 0..warm_batches {
        let cmds: Vec<_> = (0..b).map(|_| next_cmd(&mut rng)).collect();
        book.insert_batch_txn(&cmds);
    }

    let batches = cfg.iters / b;
    let mut batch_ns = vec![0u64; batches];
    for w in batch_ns.iter_mut() {
        let t0 = Instant::now();
        let cmds: Vec<_> = (0..b).map(|_| next_cmd(&mut rng)).collect();
        book.insert_batch_txn(&cmds);
        *w = t0.elapsed().as_nanos() as u64;
    }

    let ops = (batches * b) as u64;
    let total: u64 = batch_ns.iter().sum();
    emit_latency(EXPERIMENT, "batch", &batch_ns);
    emit_float(
        EXPERIMENT,
        "per_op_mean",
        total as f64 / ops as f64,
        "ns",
        ops as usize,
    );
    emit_int(EXPERIMENT, "batch_size", b as u64, "count", 1);
}
