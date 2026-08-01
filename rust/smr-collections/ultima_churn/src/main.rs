//! smr-collections **ultima_churn** — insert/cancel/fill at a real-exchange
//! order-to-trade ratio against ultima_db (one explicit-version write-txn
//! per op). Ultima never recycles a slot — row ids are the table's
//! auto-increment counter and march on monotonically — so this is the cell
//! that puts a number on what version reclamation costs when 99 % of orders
//! are cancelled rather than filled.

use bench_common::smrcoll::{SmrConfig, rss_bytes};
use smr_collections_common::churn::{Churn, emit_churn, run_churn};
use smr_collections_ultima::UltimaBook;

const EXPERIMENT: &str = "ultima_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = UltimaBook::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    let (samples, rss0) = run_churn(&cfg, &mut book, &mut churn);
    let rss1 = rss_bytes();
    emit_churn(EXPERIMENT, &samples, rss1.saturating_sub(rss0));
}
