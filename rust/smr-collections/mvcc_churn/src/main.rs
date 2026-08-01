//! smr-collections **mvcc_churn** — the churn workload against the chunked
//! copy-on-write book. Cancels scatter writes across chunks rather than
//! appending to the newest one, so this is where CoW's first-touch copy cost
//! is exercised hardest.

use bench_common::smrcoll::{SmrConfig, rss_bytes};
use smr_collections_common::churn::{Churn, emit_churn, run_churn};
use smr_collections_common::cowbook::CowBook;

const EXPERIMENT: &str = "mvcc_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    let (samples, rss0) = run_churn(&cfg, &mut book, &mut churn);
    let rss1 = rss_bytes();
    emit_churn(EXPERIMENT, &samples, rss1.saturating_sub(rss0));
}
