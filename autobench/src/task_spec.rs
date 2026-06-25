// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Peter Knego

//! Per-task descriptors for `run-iter`. A task is a benchmark matrix cell
//! `(focus_area, experiment, language)`. The per-stage commands are *data*
//! (cargo / go / gradlew argv), so the harness dispatches across languages
//! without forking per language. Adding a cell = adding a `TaskSpec` row plus a
//! task overlay under `tasks/<id>/`, not editing `run-iter`.
//!
//! Pattern from `ultima_db/autobench/src/task_spec.rs`, adapted to drive an
//! external benchmark artifact (the cell's own binary) rather than a bin inside
//! this crate.

use crate::verdict::Direction;

/// Whether the fitness run is a two-process network run or a single local run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A network cell: spawn the artifact as a `server` and a `client` over
    /// `127.0.0.1` (the fast local fitness; AWS cross-host is the graduation
    /// gate). The client emits the contract lines.
    Network,
    /// A single-host cell: run the artifact once; it emits the contract lines.
    Local,
}

/// Everything `run-iter` needs to build, smoke-test, measure, and gate a cell.
/// All commands are argv slices run from a cwd relative to the repo root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    /// Task identifier (matches `autobench/tasks/<task>/`).
    pub task: &'static str,
    /// Language dimension of the cell.
    pub language: &'static str,
    /// Focus area dimension of the cell.
    pub focus_area: &'static str,
    /// Experiment dimension of the cell.
    pub experiment: &'static str,
    /// Network (two-process) or Local (single run).
    pub kind: Kind,
    /// Build command argv.
    pub build: &'static [&'static str],
    /// CWD for the build command, relative to the repo root.
    pub build_dir: &'static str,
    /// How to launch the artifact. `RTT_MODE`/host/ports/params are passed via
    /// env, NOT argv. Launched via `cargo run` so it resolves the binary even
    /// though the global cargo config redirects the target dir.
    pub run: &'static [&'static str],
    /// CWD for the run command, relative to the repo root.
    pub run_dir: &'static str,
    /// Gate A test command argv.
    pub gate_a: &'static [&'static str],
    /// CWD for the Gate A command, relative to the repo root.
    pub gate_a_dir: &'static str,
    /// Primary metric name (contract metric, e.g. `rtt_p50`). The verdict keys
    /// the metric map as `<metric>_ns`.
    pub primary_metric: &'static str,
    /// Optimization direction for the primary metric.
    pub direction: Direction,
}

/// Resolve a `--task` value to its [`TaskSpec`], or `None` if unregistered.
pub fn task_spec(task: &str) -> Option<TaskSpec> {
    match task {
        "rust-network-rtt-tcp" => Some(TaskSpec {
            task: "rust-network-rtt-tcp",
            language: "rust",
            focus_area: "network-rtt",
            experiment: "tcp",
            kind: Kind::Network,
            build: &["cargo", "build", "--release", "-p", "network-rtt-tcp"],
            build_dir: "rust",
            run: &["cargo", "run", "--release", "-q", "-p", "network-rtt-tcp"],
            run_dir: "rust",
            gate_a: &["cargo", "test"],
            gate_a_dir: "rust",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilot_resolves() {
        let s = task_spec("rust-network-rtt-tcp").unwrap();
        assert_eq!(s.language, "rust");
        assert_eq!(s.focus_area, "network-rtt");
        assert_eq!(s.experiment, "tcp");
        assert_eq!(s.kind, Kind::Network);
        assert_eq!(s.direction, Direction::Minimize);
        assert_eq!(s.primary_metric, "rtt_p50");
        assert_eq!(
            s.build,
            &["cargo", "build", "--release", "-p", "network-rtt-tcp"]
        );
        assert_eq!(s.build_dir, "rust");
        assert_eq!(
            s.run,
            &["cargo", "run", "--release", "-q", "-p", "network-rtt-tcp"]
        );
        assert_eq!(s.gate_a, &["cargo", "test"]);
    }

    #[test]
    fn unknown_task_is_none() {
        assert!(task_spec("nope").is_none());
    }
}
