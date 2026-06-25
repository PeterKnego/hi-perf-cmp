// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Peter Knego

//! `hi-perf-autobench` — an autoresearch optimization loop for hi-perf-cmp
//! benchmark cells. The `run-iter` binary measures a candidate; `program.md`
//! describes the loop that drives it. Logic lives in these modules so the
//! binary stays thin.

pub mod sampling;
pub mod task_spec;
pub mod verdict;

use std::path::PathBuf;

/// Resolve a `TaskSpec` dir (e.g. `"rust"`), declared relative to the repo
/// root, into an absolute path — so `run-iter` works regardless of the CWD it
/// is launched from (not only the repo root). Walks up from the current dir to
/// the first ancestor containing `.git`; falls back to the dir as-is if none.
pub fn resolve_dir(dir: &str) -> PathBuf {
    if let Ok(mut p) = std::env::current_dir() {
        loop {
            if p.join(".git").exists() {
                return p.join(dir);
            }
            if !p.pop() {
                break;
            }
        }
    }
    PathBuf::from(dir)
}
