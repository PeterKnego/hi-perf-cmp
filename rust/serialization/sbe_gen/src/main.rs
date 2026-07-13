//! serialization **sbe_gen** experiment binary.

use bench_common::serial::{CountingAllocator, SerialConfig, run_journal};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

const EXPERIMENT: &str = "sbe_gen";

fn main() {
    let cfg = match SerialConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("serialization-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let (entries, cmd) = (cfg.entries, cfg.cmd_bytes);
    run_journal(
        EXPERIMENT,
        &cfg,
        |i| serialization_common::build_record(i, entries, cmd),
        serialization_sbe_gen::encode,
        serialization_sbe_gen::decode_checksum,
    );
}
