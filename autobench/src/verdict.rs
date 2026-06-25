// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Peter Knego

//! Champion-comparison direction and the JSON `Verdict` emitted by `run-iter`.
//!
//! A `Verdict` is the single JSON object `run-iter` prints on stdout. The loop
//! orchestrator (see `program.md`) reads `status`/`metrics`/`primary` to decide
//! KEEP / DISCARD / CRASH — never the process exit code (which is always 0).

use std::collections::BTreeMap;

use serde::Serialize;

/// Whether a lower or higher value of the primary metric is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Lower is better (e.g. latency).
    Minimize,
    /// Higher is better (e.g. throughput).
    Maximize,
}

impl Direction {
    /// True when `candidate` is a strict improvement over `champion` in this
    /// direction. Equal values are NOT an improvement (a wash does not win).
    pub fn improves(self, candidate: f64, champion: f64) -> bool {
        match self {
            Direction::Minimize => candidate < champion,
            Direction::Maximize => candidate > champion,
        }
    }
}

/// Overall outcome of a `run-iter` invocation. Serializes to the contract
/// strings the loop matches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// All stages ran to completion.
    Pass,
    /// `cargo build` failed.
    BuildFailed,
    /// The correctness smoke failed (bad/missing contract lines or non-zero exit).
    CorrectnessFailed,
    /// The microbench could not produce metrics.
    MicrobenchFailed,
    /// Gate A (the cell's test suite) failed.
    TestsFailed,
    /// A stage exceeded its hard wall-clock budget.
    Timeout,
    /// The `--task` value is not in the registry.
    UnknownTask,
}

/// Per-stage wall-clock durations (seconds).
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Durations {
    pub build: f64,
    pub correctness: f64,
    pub microbench: f64,
    pub tests: f64,
}

/// The JSON verdict emitted on stdout.
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    /// Overall outcome (see [`Status`]).
    pub status: Status,
    /// The last stage that ran (`setup`/`build`/`correctness`/`microbench`/`tests`).
    pub stage: String,
    /// Per-stage wall times.
    pub duration_s: Durations,
    /// Median-of-N metrics keyed `rtt_p50_ns`/`rtt_p99_ns`/`rtt_mean_ns`.
    pub metrics: BTreeMap<String, f64>,
    /// The primary metric value (median-of-N), or `None` if not measured.
    pub primary: Option<f64>,
    /// Last ~50 lines of stderr+stdout on failure.
    pub stderr_tail: Option<String>,
}

impl Verdict {
    /// A blank verdict with the given status/stage and everything else empty.
    pub fn new(status: Status, stage: &str) -> Verdict {
        Verdict {
            status,
            stage: stage.to_string(),
            duration_s: Durations::default(),
            metrics: BTreeMap::new(),
            primary: None,
            stderr_tail: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimize_lower_is_better() {
        assert!(Direction::Minimize.improves(90.0, 100.0));
        assert!(!Direction::Minimize.improves(110.0, 100.0));
    }

    #[test]
    fn maximize_higher_is_better() {
        assert!(Direction::Maximize.improves(110.0, 100.0));
        assert!(!Direction::Maximize.improves(90.0, 100.0));
    }

    #[test]
    fn equal_is_not_an_improvement() {
        assert!(!Direction::Minimize.improves(100.0, 100.0));
        assert!(!Direction::Maximize.improves(100.0, 100.0));
    }

    #[test]
    fn status_serializes_to_contract_strings() {
        let cases = [
            (Status::Pass, "pass"),
            (Status::BuildFailed, "build_failed"),
            (Status::CorrectnessFailed, "correctness_failed"),
            (Status::MicrobenchFailed, "microbench_failed"),
            (Status::TestsFailed, "tests_failed"),
            (Status::Timeout, "timeout"),
            (Status::UnknownTask, "unknown_task"),
        ];
        for (status, want) in cases {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{want}\""));
        }
    }

    #[test]
    fn verdict_json_has_expected_keys() {
        let mut v = Verdict::new(Status::Pass, "tests");
        v.metrics.insert("rtt_p50_ns".to_string(), 42000.0);
        v.metrics.insert("rtt_p99_ns".to_string(), 81000.0);
        v.metrics.insert("rtt_mean_ns".to_string(), 50000.5);
        v.primary = Some(42000.0);
        v.duration_s.build = 1.0;

        let val: serde_json::Value = serde_json::to_value(&v).unwrap();
        let obj = val.as_object().unwrap();
        for key in [
            "status",
            "stage",
            "duration_s",
            "metrics",
            "primary",
            "stderr_tail",
        ] {
            assert!(obj.contains_key(key), "missing key {key}");
        }
        assert_eq!(obj["status"], "pass");
        assert_eq!(obj["primary"], 42000.0);
        let dur = obj["duration_s"].as_object().unwrap();
        for key in ["build", "correctness", "microbench", "tests"] {
            assert!(dur.contains_key(key), "missing duration key {key}");
        }
        let metrics = obj["metrics"].as_object().unwrap();
        assert_eq!(metrics["rtt_p50_ns"], 42000.0);
        assert_eq!(metrics["rtt_p99_ns"], 81000.0);
        assert_eq!(metrics["rtt_mean_ns"], 50000.5);
    }
}
