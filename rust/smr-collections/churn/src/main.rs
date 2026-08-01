//! smr-collections **churn** — insert/cancel/fill at a real-exchange
//! order-to-trade ratio against the flat stop-the-world book. Cancels recycle
//! slots through the free list, so this is the steady state a matching engine
//! actually lives in.

use bench_common::smrcoll::{SmrConfig, rss_bytes};
use smr_collections_common::book::Book;
use smr_collections_common::churn::{Churn, emit_churn, run_churn};

const EXPERIMENT: &str = "churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = Book::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    let rss0 = rss_bytes();
    let samples = run_churn(&cfg, &mut book, &mut churn);
    let rss1 = rss_bytes();
    emit_churn(EXPERIMENT, &samples, rss1.saturating_sub(rss0));
}
