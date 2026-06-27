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
    /// Env var the cell reads for the warmup count (e.g. `RTT_WARMUP`,
    /// `TH_WARMUP`). `run-iter` sets it to the smoke/standard count per stage.
    pub warmup_env: &'static str,
    /// Env var the cell reads for the iteration count (e.g. `RTT_ITERATIONS`,
    /// `TH_ITERATIONS`).
    pub iters_env: &'static str,
    /// Fixed extra env set on every correctness/microbench run of this cell
    /// (e.g. `[("TH_RING_CAP","1024")]`). Empty for most cells.
    pub extra_env: &'static [(&'static str, &'static str)],
    /// The suffixed metric keys (`<metric>_<unit>`) the cell must emit; the
    /// correctness/microbench floor requires all present and > 0.
    pub expected_metrics: &'static [&'static str],
    /// The suffixed key (`<metric>_<unit>`) of the primary metric in the parsed
    /// map — the champion value `run-iter` reports as `primary`.
    pub primary_key: &'static str,
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
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "go-network-rtt-tcp" => Some(TaskSpec {
            task: "go-network-rtt-tcp",
            language: "go",
            focus_area: "network-rtt",
            experiment: "tcp",
            kind: Kind::Network,
            // Build the cell to a binary and launch it directly (not `go run`):
            // run-iter SIGKILLs the server child on drop, and a killed `go run`
            // parent can orphan the actual server process holding the port.
            build: &[
                "go",
                "build",
                "-o",
                "bin/network-rtt-tcp",
                "./cmd/network-rtt-tcp",
            ],
            build_dir: "go",
            run: &["./bin/network-rtt-tcp"],
            run_dir: "go",
            gate_a: &["go", "test", "./..."],
            gate_a_dir: "go",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "java-network-rtt-tcp" => Some(TaskSpec {
            task: "java-network-rtt-tcp",
            language: "java",
            focus_area: "network-rtt",
            experiment: "tcp",
            kind: Kind::Network,
            // `installDist` emits a launcher script that `exec`s the JVM, so the
            // launched process IS the JVM — run-iter's SIGKILL reaps it cleanly
            // (no Gradle daemon / worker JVM left orphaned on the port).
            build: &["./gradlew", "--quiet", ":network-rtt-tcp:installDist"],
            build_dir: "java",
            run: &["./network-rtt-tcp/build/install/network-rtt-tcp/bin/network-rtt-tcp"],
            run_dir: "java",
            gate_a: &[
                "./gradlew",
                "--quiet",
                ":network-rtt-tcp:test",
                ":common:test",
            ],
            gate_a_dir: "java",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "rust-network-rtt-udp" => Some(TaskSpec {
            task: "rust-network-rtt-udp",
            language: "rust",
            focus_area: "network-rtt",
            experiment: "udp",
            kind: Kind::Network,
            build: &["cargo", "build", "--release", "-p", "network-rtt-udp"],
            build_dir: "rust",
            run: &["cargo", "run", "--release", "-q", "-p", "network-rtt-udp"],
            run_dir: "rust",
            gate_a: &["cargo", "test"],
            gate_a_dir: "rust",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "go-network-rtt-udp" => Some(TaskSpec {
            task: "go-network-rtt-udp",
            language: "go",
            focus_area: "network-rtt",
            experiment: "udp",
            kind: Kind::Network,
            build: &[
                "go",
                "build",
                "-o",
                "bin/network-rtt-udp",
                "./cmd/network-rtt-udp",
            ],
            build_dir: "go",
            run: &["./bin/network-rtt-udp"],
            run_dir: "go",
            gate_a: &["go", "test", "./..."],
            gate_a_dir: "go",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "java-network-rtt-udp" => Some(TaskSpec {
            task: "java-network-rtt-udp",
            language: "java",
            focus_area: "network-rtt",
            experiment: "udp",
            kind: Kind::Network,
            build: &["./gradlew", "--quiet", ":network-rtt-udp:installDist"],
            build_dir: "java",
            run: &["./network-rtt-udp/build/install/network-rtt-udp/bin/network-rtt-udp"],
            run_dir: "java",
            gate_a: &[
                "./gradlew",
                "--quiet",
                ":network-rtt-udp:test",
                ":common:test",
            ],
            gate_a_dir: "java",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "rust-network-rtt-quic" => Some(TaskSpec {
            task: "rust-network-rtt-quic",
            language: "rust",
            focus_area: "network-rtt",
            experiment: "quic",
            kind: Kind::Network,
            build: &["cargo", "build", "--release", "-p", "network-rtt-quic"],
            build_dir: "rust",
            run: &["cargo", "run", "--release", "-q", "-p", "network-rtt-quic"],
            run_dir: "rust",
            gate_a: &["cargo", "test"],
            gate_a_dir: "rust",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "go-network-rtt-quic" => Some(TaskSpec {
            task: "go-network-rtt-quic",
            language: "go",
            focus_area: "network-rtt",
            experiment: "quic",
            kind: Kind::Network,
            build: &[
                "go",
                "build",
                "-o",
                "bin/network-rtt-quic",
                "./cmd/network-rtt-quic",
            ],
            build_dir: "go",
            run: &["./bin/network-rtt-quic"],
            run_dir: "go",
            gate_a: &["go", "test", "./..."],
            gate_a_dir: "go",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "java-network-rtt-quic" => Some(TaskSpec {
            task: "java-network-rtt-quic",
            language: "java",
            focus_area: "network-rtt",
            experiment: "quic",
            kind: Kind::Network,
            build: &["./gradlew", "--quiet", ":network-rtt-quic:installDist"],
            build_dir: "java",
            run: &["./network-rtt-quic/build/install/network-rtt-quic/bin/network-rtt-quic"],
            run_dir: "java",
            gate_a: &[
                "./gradlew",
                "--quiet",
                ":network-rtt-quic:test",
                ":common:test",
            ],
            gate_a_dir: "java",
            primary_metric: "rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
        }),
        "rust-thread-handoff-spin" => Some(TaskSpec {
            task: "rust-thread-handoff-spin",
            language: "rust",
            focus_area: "thread-handoff",
            experiment: "spin",
            kind: Kind::Local,
            build: &["cargo", "build", "--release", "-p", "thread-handoff-spin"],
            build_dir: "rust",
            run: &[
                "cargo",
                "run",
                "--release",
                "-q",
                "-p",
                "thread-handoff-spin",
            ],
            run_dir: "rust",
            gate_a: &["cargo", "test"],
            gate_a_dir: "rust",
            primary_metric: "handoff_rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[],
            expected_metrics: &[
                "handoff_rtt_p50_ns",
                "handoff_rtt_p99_ns",
                "handoff_rtt_mean_ns",
            ],
            primary_key: "handoff_rtt_p50_ns",
        }),
        "rust-thread-handoff-ring" => Some(TaskSpec {
            task: "rust-thread-handoff-ring",
            language: "rust",
            focus_area: "thread-handoff",
            experiment: "ring",
            kind: Kind::Local,
            build: &["cargo", "build", "--release", "-p", "thread-handoff-ring"],
            build_dir: "rust",
            run: &[
                "cargo",
                "run",
                "--release",
                "-q",
                "-p",
                "thread-handoff-ring",
            ],
            run_dir: "rust",
            gate_a: &["cargo", "test"],
            gate_a_dir: "rust",
            primary_metric: "handoff_throughput",
            direction: Direction::Maximize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[("TH_RING_CAP", "1024")],
            expected_metrics: &["handoff_throughput_ops_per_sec"],
            primary_key: "handoff_throughput_ops_per_sec",
        }),
        "rust-thread-handoff-disruptor" => Some(TaskSpec {
            task: "rust-thread-handoff-disruptor",
            language: "rust",
            focus_area: "thread-handoff",
            experiment: "disruptor",
            kind: Kind::Local,
            build: &["cargo", "build", "--release", "-p", "thread-handoff-disruptor"],
            build_dir: "rust",
            run: &[
                "cargo",
                "run",
                "--release",
                "-q",
                "-p",
                "thread-handoff-disruptor",
            ],
            run_dir: "rust",
            gate_a: &["cargo", "test"],
            gate_a_dir: "rust",
            primary_metric: "handoff_throughput",
            direction: Direction::Maximize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[("TH_RING_CAP", "1024")],
            expected_metrics: &["handoff_throughput_ops_per_sec"],
            primary_key: "handoff_throughput_ops_per_sec",
        }),
        "go-thread-handoff-spin" => Some(TaskSpec {
            task: "go-thread-handoff-spin",
            language: "go",
            focus_area: "thread-handoff",
            experiment: "spin",
            kind: Kind::Local,
            build: &[
                "go",
                "build",
                "-o",
                "bin/thread-handoff-spin",
                "./cmd/thread-handoff-spin",
            ],
            build_dir: "go",
            run: &["./bin/thread-handoff-spin"],
            run_dir: "go",
            gate_a: &["go", "test", "./..."],
            gate_a_dir: "go",
            primary_metric: "handoff_rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[],
            expected_metrics: &[
                "handoff_rtt_p50_ns",
                "handoff_rtt_p99_ns",
                "handoff_rtt_mean_ns",
            ],
            primary_key: "handoff_rtt_p50_ns",
        }),
        "go-thread-handoff-ring" => Some(TaskSpec {
            task: "go-thread-handoff-ring",
            language: "go",
            focus_area: "thread-handoff",
            experiment: "ring",
            kind: Kind::Local,
            build: &[
                "go",
                "build",
                "-o",
                "bin/thread-handoff-ring",
                "./cmd/thread-handoff-ring",
            ],
            build_dir: "go",
            run: &["./bin/thread-handoff-ring"],
            run_dir: "go",
            gate_a: &["go", "test", "./..."],
            gate_a_dir: "go",
            primary_metric: "handoff_throughput",
            direction: Direction::Maximize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[("TH_RING_CAP", "1024")],
            expected_metrics: &["handoff_throughput_ops_per_sec"],
            primary_key: "handoff_throughput_ops_per_sec",
        }),
        "java-thread-handoff-spin" => Some(TaskSpec {
            task: "java-thread-handoff-spin",
            language: "java",
            focus_area: "thread-handoff",
            experiment: "spin",
            kind: Kind::Local,
            build: &["./gradlew", "--quiet", ":thread-handoff-spin:installDist"],
            build_dir: "java",
            run: &[
                "./thread-handoff-spin/build/install/thread-handoff-spin/bin/thread-handoff-spin",
            ],
            run_dir: "java",
            gate_a: &[
                "./gradlew",
                "--quiet",
                ":thread-handoff-spin:test",
                ":common:test",
            ],
            gate_a_dir: "java",
            primary_metric: "handoff_rtt_p50",
            direction: Direction::Minimize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[],
            expected_metrics: &[
                "handoff_rtt_p50_ns",
                "handoff_rtt_p99_ns",
                "handoff_rtt_mean_ns",
            ],
            primary_key: "handoff_rtt_p50_ns",
        }),
        "java-thread-handoff-ring" => Some(TaskSpec {
            task: "java-thread-handoff-ring",
            language: "java",
            focus_area: "thread-handoff",
            experiment: "ring",
            kind: Kind::Local,
            build: &["./gradlew", "--quiet", ":thread-handoff-ring:installDist"],
            build_dir: "java",
            run: &[
                "./thread-handoff-ring/build/install/thread-handoff-ring/bin/thread-handoff-ring",
            ],
            run_dir: "java",
            gate_a: &[
                "./gradlew",
                "--quiet",
                ":thread-handoff-ring:test",
                ":common:test",
            ],
            gate_a_dir: "java",
            primary_metric: "handoff_throughput",
            direction: Direction::Maximize,
            warmup_env: "TH_WARMUP",
            iters_env: "TH_ITERATIONS",
            extra_env: &[("TH_RING_CAP", "1024")],
            expected_metrics: &["handoff_throughput_ops_per_sec"],
            primary_key: "handoff_throughput_ops_per_sec",
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
    fn go_cell_resolves_to_direct_binary() {
        let s = task_spec("go-network-rtt-tcp").unwrap();
        assert_eq!(s.language, "go");
        assert_eq!(s.experiment, "tcp");
        assert_eq!(s.kind, Kind::Network);
        // Launch the built binary directly so SIGKILL can't orphan the server.
        assert_eq!(s.run, &["./bin/network-rtt-tcp"]);
        assert_eq!(s.run_dir, "go");
        assert_eq!(s.gate_a, &["go", "test", "./..."]);
        assert_eq!(s.primary_metric, "rtt_p50");
    }

    #[test]
    fn java_cell_resolves_to_installdist_launcher() {
        let s = task_spec("java-network-rtt-tcp").unwrap();
        assert_eq!(s.language, "java");
        assert_eq!(s.experiment, "tcp");
        assert_eq!(s.kind, Kind::Network);
        assert_eq!(
            s.build,
            &["./gradlew", "--quiet", ":network-rtt-tcp:installDist"]
        );
        // The launcher script exec's the JVM, so the spawned process is the JVM.
        assert_eq!(
            s.run,
            &["./network-rtt-tcp/build/install/network-rtt-tcp/bin/network-rtt-tcp"]
        );
        assert_eq!(s.run_dir, "java");
    }

    #[test]
    fn udp_cells_resolve_with_udp_experiment() {
        for (task, lang) in [
            ("rust-network-rtt-udp", "rust"),
            ("go-network-rtt-udp", "go"),
            ("java-network-rtt-udp", "java"),
        ] {
            let s = task_spec(task).unwrap();
            assert_eq!(s.language, lang);
            assert_eq!(s.experiment, "udp");
            assert_eq!(s.focus_area, "network-rtt");
            assert_eq!(s.kind, Kind::Network);
            assert_eq!(s.primary_metric, "rtt_p50");
        }
    }

    #[test]
    fn quic_cells_resolve_with_quic_experiment() {
        for (task, lang) in [
            ("rust-network-rtt-quic", "rust"),
            ("go-network-rtt-quic", "go"),
            ("java-network-rtt-quic", "java"),
        ] {
            let s = task_spec(task).unwrap();
            assert_eq!(s.language, lang);
            assert_eq!(s.experiment, "quic");
            assert_eq!(s.focus_area, "network-rtt");
            assert_eq!(s.kind, Kind::Network);
            assert_eq!(s.primary_metric, "rtt_p50");
        }
    }

    #[test]
    fn unknown_task_is_none() {
        assert!(task_spec("nope").is_none());
    }

    #[test]
    fn network_rows_keep_ns_primary_and_expected() {
        for t in [
            "rust-network-rtt-tcp",
            "go-network-rtt-udp",
            "java-network-rtt-quic",
        ] {
            let s = task_spec(t).unwrap();
            assert_eq!(s.kind, Kind::Network);
            assert_eq!(s.primary_key, "rtt_p50_ns");
            assert_eq!(
                s.expected_metrics,
                &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"]
            );
            assert_eq!(s.warmup_env, "RTT_WARMUP");
            assert_eq!(s.iters_env, "RTT_ITERATIONS");
            assert!(s.extra_env.is_empty());
        }
    }

    #[test]
    fn thread_handoff_spin_resolves_local_minimize() {
        let s = task_spec("rust-thread-handoff-spin").unwrap();
        assert_eq!(s.language, "rust");
        assert_eq!(s.focus_area, "thread-handoff");
        assert_eq!(s.experiment, "spin");
        assert_eq!(s.kind, Kind::Local);
        assert_eq!(s.direction, Direction::Minimize);
        assert_eq!(s.primary_key, "handoff_rtt_p50_ns");
        assert_eq!(
            s.expected_metrics,
            &[
                "handoff_rtt_p50_ns",
                "handoff_rtt_p99_ns",
                "handoff_rtt_mean_ns"
            ]
        );
        assert_eq!(s.warmup_env, "TH_WARMUP");
        assert_eq!(s.iters_env, "TH_ITERATIONS");
        assert!(s.extra_env.is_empty());
        assert_eq!(
            s.run,
            &[
                "cargo",
                "run",
                "--release",
                "-q",
                "-p",
                "thread-handoff-spin"
            ]
        );
    }

    #[test]
    fn thread_handoff_ring_resolves_local_maximize_with_ring_cap() {
        let s = task_spec("rust-thread-handoff-ring").unwrap();
        assert_eq!(s.kind, Kind::Local);
        assert_eq!(s.direction, Direction::Maximize);
        assert_eq!(s.primary_key, "handoff_throughput_ops_per_sec");
        assert_eq!(s.expected_metrics, &["handoff_throughput_ops_per_sec"]);
        assert_eq!(s.extra_env, &[("TH_RING_CAP", "1024")]);
    }

    #[test]
    fn go_thread_handoff_cells_resolve_local() {
        let spin = task_spec("go-thread-handoff-spin").unwrap();
        assert_eq!(spin.language, "go");
        assert_eq!(spin.focus_area, "thread-handoff");
        assert_eq!(spin.kind, Kind::Local);
        assert_eq!(spin.direction, Direction::Minimize);
        assert_eq!(spin.primary_key, "handoff_rtt_p50_ns");
        assert_eq!(spin.run, &["./bin/thread-handoff-spin"]);
        assert_eq!(spin.gate_a, &["go", "test", "./..."]);
        assert_eq!(spin.warmup_env, "TH_WARMUP");
        assert_eq!(spin.iters_env, "TH_ITERATIONS");
        assert!(spin.extra_env.is_empty());

        let ring = task_spec("go-thread-handoff-ring").unwrap();
        assert_eq!(ring.kind, Kind::Local);
        assert_eq!(ring.direction, Direction::Maximize);
        assert_eq!(ring.primary_key, "handoff_throughput_ops_per_sec");
        assert_eq!(ring.run, &["./bin/thread-handoff-ring"]);
        assert_eq!(ring.extra_env, &[("TH_RING_CAP", "1024")]);
    }

    #[test]
    fn java_thread_handoff_cells_resolve_local() {
        let spin = task_spec("java-thread-handoff-spin").unwrap();
        assert_eq!(spin.language, "java");
        assert_eq!(spin.kind, Kind::Local);
        assert_eq!(spin.direction, Direction::Minimize);
        assert_eq!(spin.primary_key, "handoff_rtt_p50_ns");
        assert_eq!(
            spin.build,
            &["./gradlew", "--quiet", ":thread-handoff-spin:installDist"]
        );
        assert_eq!(
            spin.run,
            &["./thread-handoff-spin/build/install/thread-handoff-spin/bin/thread-handoff-spin"]
        );
        assert_eq!(spin.warmup_env, "TH_WARMUP");

        let ring = task_spec("java-thread-handoff-ring").unwrap();
        assert_eq!(ring.kind, Kind::Local);
        assert_eq!(ring.direction, Direction::Maximize);
        assert_eq!(ring.primary_key, "handoff_throughput_ops_per_sec");
        assert_eq!(ring.extra_env, &[("TH_RING_CAP", "1024")]);
    }
}
