//! filesystem-write **prealloc** experiment (Rust): append one entry, fdatasync
//! with preallocation. Emits four result-contract lines. See the design spec.

use bench_common::fswrite::{self, SyncKind};
use std::process::ExitCode;

const EXPERIMENT: &str = "prealloc";

fn main() -> ExitCode {
    match fswrite::run_and_emit(EXPERIMENT, SyncKind::Data, true, false) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("filesystem-write-{EXPERIMENT}: {msg}");
            ExitCode::FAILURE
        }
    }
}
