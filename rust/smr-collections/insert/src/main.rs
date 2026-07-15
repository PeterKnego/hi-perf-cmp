//! smr-collections **insert** — time inserting resting orders into the book.

use bench_common::smrcoll::{SmrConfig, emit_latency, measure};
use smr_collections_common::book::Book;
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::rng::{SEED, SplitMix};

const EXPERIMENT: &str = "insert";

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
    let mut i = 0usize;
    let samples = measure(cfg.warmup, cfg.iters, || {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
        i += 1;
    });
    emit_latency(EXPERIMENT, "insert", &samples);
}
