//! smr-collections **update** — time amend/partial-fill on existing orders.

use bench_common::smrcoll::{SmrConfig, emit_latency, measure};
use smr_collections_common::book::Book;
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::rng::{SEED, SplitMix};

const EXPERIMENT: &str = "update";

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
    // Pre-build steady state (not timed). Reuses the insert stream.
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;
    let samples = measure(cfg.warmup, cfg.iters, || {
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
    });
    emit_latency(EXPERIMENT, "update", &samples);
}
