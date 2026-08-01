//! smr-collections **mvcc_insert** — insert cost on the chunked-CoW book
//! (steady-state MVCC overhead; no snapshots fire here).

use bench_common::smrcoll::{SmrConfig, emit_latency, measure};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::cowbook::CowBook;
use smr_collections_common::rng::{SEED, SplitMix};

const EXPERIMENT: &str = "mvcc_insert";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    if let Err(m) = cfg.require_bump_capacity() {
        eprintln!("smr-collections-{EXPERIMENT}: {m}");
        std::process::exit(1);
    }
    let mut book = CowBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    let mut i = 0usize;
    let samples = measure(cfg.warmup, cfg.iters, || {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
        i += 1;
    });
    emit_latency(EXPERIMENT, "insert", &samples);
}
