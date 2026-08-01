//! smr-collections **ultima_batch_churn** — insert/cancel/fill churn cost
//! through ultima_db with ONE explicit-version write-txn per `apply_batch`
//! commands (the SMR consensus-batch pattern), mirroring `ultima_batch_insert`
//! for a stream that mixes op types instead of running one type at a time.
//! `batch_*` times the whole mixed-type txn; a mixed txn cannot isolate one
//! op type's share of that cost, so the per-op-type split below apportions it
//! evenly (`batch_ns / batch_len`) across the ops the batch carried — an
//! amortized estimate, not a per-type measurement. Compare `batch_mean`
//! against `ultima_churn`'s per-op means: the difference is pure txn
//! amortization, same as `ultima_batch_insert` vs `ultima_insert`.

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::churn::{Churn, ChurnOp, ChurnSamples};
use smr_collections_ultima::UltimaBook;
use std::time::Instant;

const EXPERIMENT: &str = "ultima_batch_churn";

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
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);

    let warm_batches = cfg.warmup / b;
    for _ in 0..warm_batches {
        let ops: Vec<ChurnOp> = (0..b).map(|_| churn.next_op()).collect();
        if cfg.multi_table {
            book.churn_batch_txn_multi(&ops);
        } else {
            book.churn_batch_txn(&ops);
        }
    }

    let batches = cfg.iters / b;
    let mut batch_ns = vec![0u64; batches];
    let mut s = ChurnSamples::default();
    for w in batch_ns.iter_mut() {
        // Op generation is untimed, same convention as run_churn: the driver
        // produces the batch, the clock times only the store applying it.
        let ops: Vec<ChurnOp> = (0..b).map(|_| churn.next_op()).collect();
        let t0 = Instant::now();
        if cfg.multi_table {
            book.churn_batch_txn_multi(&ops);
        } else {
            book.churn_batch_txn(&ops);
        }
        let ns = t0.elapsed().as_nanos() as u64;
        *w = ns;
        let per_op = ns / b as u64;
        for op in &ops {
            match op {
                ChurnOp::Insert { .. } => s.insert_ns.push(per_op),
                ChurnOp::Cancel(_) => s.cancel_ns.push(per_op),
                ChurnOp::Fill(_) => s.fill_ns.push(per_op),
            }
        }
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
    if !s.insert_ns.is_empty() {
        emit_latency(EXPERIMENT, "insert", &s.insert_ns);
    }
    if !s.cancel_ns.is_empty() {
        emit_latency(EXPERIMENT, "cancel", &s.cancel_ns);
    }
    if !s.fill_ns.is_empty() {
        emit_latency(EXPERIMENT, "fill", &s.fill_ns);
    }
}
