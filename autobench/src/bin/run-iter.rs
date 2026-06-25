// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Peter Knego

//! run-iter — the gated per-iteration measurement harness for hi-perf-cmp
//! autobench. Stages: build -> correctness -> microbench (fitness) -> tests
//! (Gate A). It emits exactly one JSON `Verdict` on stdout and ALWAYS exits 0:
//! the loop orchestrator reads `status`, never the exit code.
//!
//! ## CWD
//!
//! Stage commands set their own `current_dir` from the `TaskSpec` (`build_dir`
//! / `run_dir` / `gate_a_dir`, relative to the repo root), so run-iter must be
//! launched from the repo root (the autobench loop runs from the primary
//! checkout). The artifact is launched via `cargo run` so it resolves the
//! binary even though the global `~/.cargo/config.toml` redirects the target
//! dir — never hardcode a `target/release/<bin>` path.
//!
//! ## Network fitness
//!
//! For a `Network` cell, the artifact is run as two processes over `127.0.0.1`
//! (a `server` child + a `client`) — this exercises the real kernel TCP stack
//! so *relative* optimizations are meaningful. The AWS cross-host run is the
//! separate graduation gate (see `program.md`), NOT a per-iteration step.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clap::Parser;
use hi_perf_autobench::sampling::{
    NetworkRun, Transport, median, parse_contract_metrics, run_network_once,
};
use hi_perf_autobench::task_spec::{Kind, TaskSpec, task_spec};
use hi_perf_autobench::verdict::{Status, Verdict};
use wait_timeout::ChildExt;

/// Loopback port for the two-process fitness runs. Distinct from the cell's
/// default 9100 to avoid colliding with anything a developer left running.
const FITNESS_PORT: u16 = 19100;

/// Tiny counts for the correctness smoke: enough to produce well-formed lines,
/// fast enough to be a smoke test.
const SMOKE_WARMUP: &str = "20";
const SMOKE_ITERATIONS: &str = "200";

/// Standard counts for the microbench fitness. Kept modest so the local fast
/// loop stays fast; relative comparisons over 127.0.0.1 are what matter.
const BENCH_WARMUP: &str = "2000";
const BENCH_ITERATIONS: &str = "20000";

/// Hard per-stage wall-clock budgets.
const BUILD_TIMEOUT: Duration = Duration::from_secs(600);
const TESTS_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Parser, Debug)]
#[command(name = "run-iter")]
struct Args {
    /// Task id, e.g. `rust-network-rtt-tcp`.
    #[arg(long)]
    task: String,
    /// Required; emit one JSON verdict on stdout (exit 0 even on failure).
    #[arg(long)]
    json: bool,
    /// Current champion's primary value (recorded for context; not gated here).
    #[arg(long)]
    baseline_primary: Option<f64>,
    /// Median-of-N sample count for the microbench fitness.
    #[arg(long, default_value_t = 5)]
    samples: usize,
}

/// Result of running a subprocess under a hard timeout.
struct StageRun {
    exit_ok: bool,
    stderr: String,
    stdout: String,
    duration_s: f64,
    timed_out: bool,
}

/// Spawn `cmd`, drain stdout/stderr concurrently (so a full pipe can't deadlock
/// the child), and enforce a hard wall-clock `timeout`. On timeout the child is
/// killed and `timed_out` is set.
fn run_stage(mut cmd: Command, timeout: Duration) -> StageRun {
    use std::io::Read;
    let started = Instant::now();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return StageRun {
                exit_ok: false,
                stderr: format!("failed to spawn: {e}"),
                stdout: String::new(),
                duration_s: 0.0,
                timed_out: false,
            };
        }
    };

    let mut out_pipe = child.stdout.take().expect("stdout piped");
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let out_handle = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out_pipe.read_to_string(&mut s);
        s
    });
    let err_handle = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err_pipe.read_to_string(&mut s);
        s
    });

    let status = match child.wait_timeout(timeout).expect("wait_timeout") {
        Some(s) => Some(s),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    StageRun {
        exit_ok: matches!(status, Some(s) if s.success()),
        stderr,
        stdout,
        duration_s: started.elapsed().as_secs_f64(),
        timed_out: status.is_none(),
    }
}

/// Build a `cargo`-style stage command from a `TaskSpec` argv + cwd.
fn stage_command(argv: &[&str], dir: &str) -> Command {
    let mut c = Command::new(argv[0]);
    c.args(&argv[1..])
        .current_dir(hi_perf_autobench::resolve_dir(dir));
    c
}

/// Last `n` lines of `s`, preserving a trailing newline if present.
fn tail_lines(s: &str, n: usize) -> String {
    let mut lines: Vec<&str> = s.split_inclusive('\n').collect();
    if lines.len() > n {
        lines = lines.split_off(lines.len() - n);
    }
    lines.concat()
}

/// Emit the verdict on stdout and exit 0.
fn emit_and_exit(v: &Verdict) -> ! {
    println!("{}", serde_json::to_string(v).expect("serialize Verdict"));
    std::process::exit(0);
}

/// Finalize a verdict for a failed stage and exit. Maps timeout -> `Timeout`.
fn fail(mut v: Verdict, fail_status: Status, stage: &str, run: &StageRun) -> ! {
    v.status = if run.timed_out {
        Status::Timeout
    } else {
        fail_status
    };
    v.stage = stage.to_string();
    v.stderr_tail = Some(tail_lines(
        &format!("{}\n--- stdout ---\n{}", run.stderr, run.stdout),
        50,
    ));
    emit_and_exit(&v);
}

fn main() {
    let args = Args::parse();
    let Some(spec) = task_spec(&args.task) else {
        let mut v = Verdict::new(Status::UnknownTask, "setup");
        v.stderr_tail = Some(format!(
            "unknown task {:?}; known: {{rust,go,java}}-network-rtt-{{tcp,udp,quic}}",
            args.task
        ));
        emit_and_exit(&v);
    };
    if !args.json {
        eprintln!("run-iter: only --json output mode is supported");
        std::process::exit(2);
    }

    let mut v = Verdict::new(Status::Pass, "setup");

    // 1: build
    let r = run_stage(stage_command(spec.build, spec.build_dir), BUILD_TIMEOUT);
    v.duration_s.build = r.duration_s;
    if r.timed_out || !r.exit_ok {
        fail(v, Status::BuildFailed, "build", &r);
    }

    // 2: correctness — anti-Goodhart floor (Network cell).
    let started = Instant::now();
    correctness(&mut v, &spec);
    v.duration_s.correctness = started.elapsed().as_secs_f64();

    // 3: microbench (fitness) — median-of-N (Network cell).
    let started = Instant::now();
    microbench(&mut v, &spec, args.samples);
    v.duration_s.microbench = started.elapsed().as_secs_f64();

    // 4: Gate A — the cell's test suite.
    let r = run_stage(stage_command(spec.gate_a, spec.gate_a_dir), TESTS_TIMEOUT);
    v.duration_s.tests = r.duration_s;
    if r.timed_out || !r.exit_ok {
        fail(v, Status::TestsFailed, "tests", &r);
    }

    v.status = Status::Pass;
    v.stage = "tests".to_string();
    emit_and_exit(&v);
}

/// A correctness failure: stamp the verdict and exit.
fn correctness_fail(mut v: Verdict, detail: String) -> ! {
    v.status = Status::CorrectnessFailed;
    v.stage = "correctness".to_string();
    v.stderr_tail = Some(tail_lines(&detail, 50));
    emit_and_exit(&v);
}

/// A microbench failure: stamp the verdict and exit.
fn microbench_fail(mut v: Verdict, detail: String) -> ! {
    v.status = Status::MicrobenchFailed;
    v.stage = "microbench".to_string();
    v.stderr_tail = Some(tail_lines(&detail, 50));
    emit_and_exit(&v);
}

/// Run a single two-process network sample, or a microbench-style error exit if
/// the spawn itself failed (I/O error launching cargo).
fn one_network_run(
    v: &Verdict,
    spec: &TaskSpec,
    env: &[(&str, &str)],
    stage_is_smoke: bool,
) -> NetworkRun {
    let transport = Transport::from_experiment(spec.experiment);
    match run_network_once(spec.run, spec.run_dir, FITNESS_PORT, env, transport) {
        Ok(run) => run,
        Err(e) => {
            let detail = format!("two-process driver I/O error: {e}");
            if stage_is_smoke {
                correctness_fail(v.clone(), detail);
            } else {
                microbench_fail(v.clone(), detail);
            }
        }
    }
}

/// Correctness smoke: a tiny two-process run that must exit 0 and yield exactly
/// 3 contract lines with `experiment=<exp>` and all values > 0.
fn correctness(v: &mut Verdict, spec: &TaskSpec) {
    if spec.kind != Kind::Network {
        // Local kind not yet wired; the pilot is Network.
        correctness_fail(
            v.clone(),
            format!("kind {:?} correctness not implemented", spec.kind),
        );
    }
    let env = [
        ("RTT_WARMUP", SMOKE_WARMUP),
        ("RTT_ITERATIONS", SMOKE_ITERATIONS),
    ];
    let run = one_network_run(v, spec, &env, true);
    if !run.client_ok {
        correctness_fail(
            v.clone(),
            format!(
                "correctness client exited non-zero\n--- stderr ---\n{}\n--- stdout ---\n{}",
                run.stderr, run.stdout
            ),
        );
    }
    let metrics = parse_contract_metrics(&run.stdout, spec.focus_area, spec.experiment);
    if metrics.len() != 3 {
        correctness_fail(
            v.clone(),
            format!(
                "expected exactly 3 contract lines for {}/{}, got {} ({:?})\n--- stdout ---\n{}",
                spec.focus_area,
                spec.experiment,
                metrics.len(),
                metrics.keys().collect::<Vec<_>>(),
                run.stdout
            ),
        );
    }
    if let Some((k, val)) = metrics.iter().find(|&(_, &val)| val <= 0.0) {
        correctness_fail(
            v.clone(),
            format!(
                "contract metric {k}={val} is not > 0\n--- stdout ---\n{}",
                run.stdout
            ),
        );
    }
}

/// Microbench fitness: `samples` two-process runs at standard counts; record
/// the per-metric medians and the primary metric.
fn microbench(v: &mut Verdict, spec: &TaskSpec, samples: usize) {
    let n = samples.max(1);
    let env = [
        ("RTT_WARMUP", BENCH_WARMUP),
        ("RTT_ITERATIONS", BENCH_ITERATIONS),
    ];

    // Accumulate per-metric value vectors across the N samples.
    let mut series: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for i in 0..n {
        let run = one_network_run(v, spec, &env, false);
        if !run.client_ok {
            microbench_fail(
                v.clone(),
                format!(
                    "microbench client (sample {i}) exited non-zero\n--- stderr ---\n{}\n--- stdout ---\n{}",
                    run.stderr, run.stdout
                ),
            );
        }
        let metrics = parse_contract_metrics(&run.stdout, spec.focus_area, spec.experiment);
        if metrics.len() != 3 {
            microbench_fail(
                v.clone(),
                format!(
                    "microbench sample {i}: expected 3 contract lines, got {}\n--- stdout ---\n{}",
                    metrics.len(),
                    run.stdout
                ),
            );
        }
        for (k, val) in metrics {
            series.entry(k).or_default().push(val);
        }
    }

    for (k, vals) in &series {
        v.metrics.insert(k.clone(), median(vals));
    }

    let primary_key = format!("{}_ns", spec.primary_metric);
    match v.metrics.get(&primary_key) {
        Some(&p) => v.primary = Some(p),
        None => microbench_fail(
            v.clone(),
            format!("missing primary metric `{primary_key}` after {n} samples"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_lines_returns_last_n() {
        assert_eq!(tail_lines("a\nb\nc\nd\n", 2), "c\nd\n");
        assert_eq!(tail_lines("a\nb", 5), "a\nb");
    }
}
