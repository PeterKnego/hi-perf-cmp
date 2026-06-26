//! filesystem-write **batch** experiment (Rust): append one entry, fdatasync
//! with preallocation and batching. Emits four result-contract lines. See the design spec.

use bench_common::fswrite::{self, SyncKind};
use std::process::ExitCode;

const EXPERIMENT: &str = "batch";

fn main() -> ExitCode {
    match fswrite::run_and_emit(EXPERIMENT, SyncKind::Data, true, true) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("filesystem-write-{EXPERIMENT}: {msg}");
            ExitCode::FAILURE
        }
    }
}
