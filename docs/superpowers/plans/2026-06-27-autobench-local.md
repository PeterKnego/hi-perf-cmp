# autobench `Local`-kind support + thread-handoff cells — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the `autobench` harness to run single-host (`Local`) optimization cells, and register `rust-thread-handoff-spin` and `rust-thread-handoff-ring`, so the loop can optimize the Rust thread-handoff `spin`/`ring` experiments.

**Architecture:** A data-driven, backward-compatible harness change. `TaskSpec` gains env/metric fields; `parse_contract_metrics` keys by `<metric>_<unit>` (preserving the network cells' `rtt_p50_ns`); `sampling` gains a single-process `run_local_once` driver; `run-iter`'s `correctness`/`microbench` dispatch on `kind`. Then two `Local` `TaskSpec` rows + task overlays. The actual optimization run is a separate step (not this plan).

**Tech Stack:** Rust 1.96 (the standalone `autobench/` crate `hi-perf-autobench`, deps serde/serde_json/clap/wait-timeout/tempfile). Runs from the **primary repo root** (not a worktree) — `run-iter` resolves cell dirs via `.git`.

## Global Constraints

- **Backward compatibility:** the nine existing `Network` cells must keep resolving and behaving identically. `parse_contract_metrics` of an `ns` line must still key `<metric>_ns` (so `rtt_p50_ns` is unchanged). Existing autobench tests stay green.
- **Metric key format:** `<metric>_<unit>` from the contract line's `metric` + `unit` fields (e.g. `rtt_p50_ns`, `handoff_rtt_p50_ns`, `handoff_throughput_ops_per_sec`).
- **Two new cells:** `rust-thread-handoff-spin` (primary `handoff_rtt_p50_ns`, **Minimize**, expected the 3 `handoff_rtt_*_ns`, no extra env) and `rust-thread-handoff-ring` (primary `handoff_throughput_ops_per_sec`, **Maximize**, expected `handoff_throughput_ops_per_sec`, extra env `TH_RING_CAP=1024`). Both `kind: Local`, build/run/gate via `cargo … -p thread-handoff-<exp>` in `rust/`, `warmup_env=TH_WARMUP`, `iters_env=TH_ITERATIONS`.
- **Frozen for the cells** (documented in overlays, enforced by review later): `rust/bench-common/**`, the result contract, every other cell, `autobench/**`, docs; no new cell dependency.
- **Crate stays clippy- and rustfmt-clean** (`cargo clippy --all-targets`, `cargo fmt --check` within `autobench/`). `run-iter` always exits 0 and emits one JSON `Verdict`.
- All `autobench` cargo commands run with `--manifest-path autobench/Cargo.toml` (the crate is **not** in the `rust/` workspace).

---

## Task 1: Unit-aware metric keying

**Files:**
- Modify: `autobench/src/sampling.rs` (the `parse_contract_metrics` fn + its doc comment + tests)

**Interfaces:**
- Produces: `parse_contract_metrics(stdout, focus_area, experiment) -> BTreeMap<String,f64>` keyed `<metric>_<unit>` (signature unchanged; key format changed).

- [ ] **Step 1: Update the tests to the new keying** — in `autobench/src/sampling.rs`, replace the `parses_three_tcp_lines` test and add two new tests. Replace this existing test:

```rust
    #[test]
    fn parses_three_tcp_lines() {
        let m = parse_contract_metrics(TCP_LINES, "network-rtt", "tcp");
        assert_eq!(m.len(), 3);
        assert_eq!(m["rtt_p50_ns"], 42000.0);
        assert_eq!(m["rtt_p99_ns"], 81000.0);
        assert_eq!(m["rtt_mean_ns"], 50000.5);
    }
```

  with (note: `ns` lines still key `rtt_p50_ns` — backward compatibility — plus new cases for thread-handoff units):

```rust
    #[test]
    fn parses_three_tcp_lines_ns_keys_unchanged() {
        // Backward compatibility: an `ns` unit still yields the `_ns` key.
        let m = parse_contract_metrics(TCP_LINES, "network-rtt", "tcp");
        assert_eq!(m.len(), 3);
        assert_eq!(m["rtt_p50_ns"], 42000.0);
        assert_eq!(m["rtt_p99_ns"], 81000.0);
        assert_eq!(m["rtt_mean_ns"], 50000.5);
    }

    #[test]
    fn keys_thread_handoff_latency_and_throughput_by_unit() {
        let spin = r#"
{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_p50","value":182,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_mean","value":186.2,"unit":"ns","samples":100000}
"#;
        let m = parse_contract_metrics(spin, "thread-handoff", "spin");
        assert_eq!(m["handoff_rtt_p50_ns"], 182.0);
        assert_eq!(m["handoff_rtt_mean_ns"], 186.2);

        let ring = r#"{"language":"rust","focus_area":"thread-handoff","experiment":"ring","metric":"handoff_throughput","value":28139265.7,"unit":"ops_per_sec","samples":100000}"#;
        let r = parse_contract_metrics(ring, "thread-handoff", "ring");
        assert_eq!(r["handoff_throughput_ops_per_sec"], 28139265.7);
    }

    #[test]
    fn line_missing_unit_is_skipped() {
        let line = r#"{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_p50","value":1,"samples":1}"#;
        let m = parse_contract_metrics(line, "thread-handoff", "spin");
        assert!(m.is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd autobench && cargo test --bin run-iter 2>/dev/null; cargo test keys_thread_handoff_latency_and_throughput_by_unit line_missing_unit_is_skipped 2>&1 | tail -15`
Expected: the new tests FAIL (`handoff_throughput_ops_per_sec` absent — current code keys it `handoff_throughput_ns`; the missing-unit line currently still parses).

- [ ] **Step 3: Implement unit-aware keying** — in `parse_contract_metrics`, after the `metric` extraction and before the `value` extraction, read the `unit` field, and key by `<metric>_<unit>`. Replace this block:

```rust
        let Some(metric) = v.get("metric").and_then(|m| m.as_str()) else {
            continue;
        };
        let Some(value) = v.get("value").and_then(serde_json::Value::as_f64) else {
            continue;
        };
        out.insert(format!("{metric}_ns"), value);
```

  with:

```rust
        let Some(metric) = v.get("metric").and_then(|m| m.as_str()) else {
            continue;
        };
        let Some(unit) = v.get("unit").and_then(|u| u.as_str()) else {
            continue;
        };
        let Some(value) = v.get("value").and_then(serde_json::Value::as_f64) else {
            continue;
        };
        out.insert(format!("{metric}_{unit}"), value);
```

  And update the fn doc comment line `/// metrics map keyed `<metric>_ns` (e.g. `rtt_p50` -> `"rtt_p50_ns"`).` to `/// metrics map keyed `<metric>_<unit>` (e.g. `rtt_p50`+`ns` -> `"rtt_p50_ns"`).`

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd autobench && cargo test 2>&1 | tail -15`
Expected: all sampling tests PASS, including the two new ones and the unchanged `rtt_*_ns` assertions.

- [ ] **Step 5: Commit**

```bash
git add autobench/src/sampling.rs
git commit -m "autobench: key contract metrics by <metric>_<unit> (backward-compatible for ns)"
```

---

## Task 2: `Local` single-run driver

**Files:**
- Modify: `autobench/src/sampling.rs` (add `LocalRun` + `run_local_once` + a test)

**Interfaces:**
- Consumes: the existing private `run_command(run, run_dir, env) -> Command`.
- Produces: `pub struct LocalRun { pub ok: bool, pub stdout: String, pub stderr: String }`; `pub fn run_local_once(run: &[&str], run_dir: &str, env: &[(&str,&str)]) -> std::io::Result<LocalRun>`.

- [ ] **Step 1: Write the failing test** — add to the `tests` module in `autobench/src/sampling.rs`:

```rust
    #[test]
    fn run_local_once_captures_output_and_exit() {
        // `.` resolves (via resolve_dir) to the repo root — a valid cwd.
        let ok = run_local_once(
            &["sh", "-c", "printf 'OUT\\n'; printf 'ERR\\n' 1>&2; exit 0"],
            ".",
            &[],
        )
        .unwrap();
        assert!(ok.ok);
        assert!(ok.stdout.contains("OUT"));
        assert!(ok.stderr.contains("ERR"));

        let bad = run_local_once(&["sh", "-c", "exit 3"], ".", &[]).unwrap();
        assert!(!bad.ok);

        // Env is passed through to the child.
        let env = run_local_once(&["sh", "-c", "printf '%s' \"$AB_TEST_VAR\""], ".", &[("AB_TEST_VAR", "xyz")]).unwrap();
        assert_eq!(env.stdout, "xyz");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd autobench && cargo test run_local_once_captures_output_and_exit 2>&1 | tail -15`
Expected: FAIL to compile — `run_local_once` / `LocalRun` not found.

- [ ] **Step 3: Implement the driver** — add to `autobench/src/sampling.rs`, immediately after the `NetworkRun` struct definition (before `run_network_once`):

```rust
/// Outcome of one single-process local run (a `Local` cell emits its contract
/// lines directly on stdout — no server child, port, or readiness probe).
pub struct LocalRun {
    /// True if the process exited 0.
    pub ok: bool,
    /// Captured stdout (the contract lines).
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
}

/// Run the artifact once with `env`, capturing its stdout/stderr and exit
/// status. The `Local` analogue of [`run_network_once`].
pub fn run_local_once(
    run: &[&str],
    run_dir: &str,
    env: &[(&str, &str)],
) -> std::io::Result<LocalRun> {
    let output = run_command(run, run_dir, env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    Ok(LocalRun {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd autobench && cargo test run_local_once_captures_output_and_exit 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add autobench/src/sampling.rs
git commit -m "autobench: add run_local_once single-process driver for Local cells"
```

---

## Task 3: `TaskSpec` env/metric fields + the two `Local` rows

**Files:**
- Modify: `autobench/src/task_spec.rs` (struct fields; all 9 network rows; 2 new rows; tests)

**Interfaces:**
- Consumes: `Direction::{Minimize,Maximize}`, `Kind::{Network,Local}`.
- Produces: `TaskSpec` with new fields `warmup_env: &'static str`, `iters_env: &'static str`, `extra_env: &'static [(&'static str,&'static str)]`, `expected_metrics: &'static [&'static str]`, `primary_key: &'static str`; resolvable tasks `"rust-thread-handoff-spin"`, `"rust-thread-handoff-ring"`.

- [ ] **Step 1: Add the five fields to the `TaskSpec` struct** — in `autobench/src/task_spec.rs`, immediately after the `pub direction: Direction,` field (the last field), add:

```rust
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
```

- [ ] **Step 2: Fill the five fields into every one of the nine network rows** — each existing `Some(TaskSpec { … })` arm ends with `direction: Direction::Minimize,`. Immediately after that line, in **all nine** network arms (`{rust,go,java}-network-rtt-{tcp,udp,quic}`), add these exact five lines (identical for every network row):

```rust
            warmup_env: "RTT_WARMUP",
            iters_env: "RTT_ITERATIONS",
            extra_env: &[],
            expected_metrics: &["rtt_p50_ns", "rtt_p99_ns", "rtt_mean_ns"],
            primary_key: "rtt_p50_ns",
```

(The crate will not compile until all nine arms have them — every `TaskSpec` literal needs all fields. Build after this step to confirm: `cd autobench && cargo build 2>&1 | tail -5` should succeed once all nine are filled.)

- [ ] **Step 3: Add the two `Local` rows** — in `task_spec.rs`, insert these two arms immediately before the final `_ => None,` arm:

```rust
        "rust-thread-handoff-spin" => Some(TaskSpec {
            task: "rust-thread-handoff-spin",
            language: "rust",
            focus_area: "thread-handoff",
            experiment: "spin",
            kind: Kind::Local,
            build: &["cargo", "build", "--release", "-p", "thread-handoff-spin"],
            build_dir: "rust",
            run: &["cargo", "run", "--release", "-q", "-p", "thread-handoff-spin"],
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
            run: &["cargo", "run", "--release", "-q", "-p", "thread-handoff-ring"],
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
```

- [ ] **Step 4: Add resolution tests** — append to the `tests` module in `task_spec.rs`:

```rust
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
            &["handoff_rtt_p50_ns", "handoff_rtt_p99_ns", "handoff_rtt_mean_ns"]
        );
        assert_eq!(s.warmup_env, "TH_WARMUP");
        assert_eq!(s.iters_env, "TH_ITERATIONS");
        assert!(s.extra_env.is_empty());
        assert_eq!(
            s.run,
            &["cargo", "run", "--release", "-q", "-p", "thread-handoff-spin"]
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
```

- [ ] **Step 5: Run the tests**

Run: `cd autobench && cargo test --lib task_spec 2>&1 | tail -20`
Expected: all `task_spec` tests PASS (existing 6 + new 3).

- [ ] **Step 6: Commit**

```bash
git add autobench/src/task_spec.rs
git commit -m "autobench: add Local thread-handoff spin/ring TaskSpec rows + env/metric fields"
```

---

## Task 4: `run-iter` `Local` correctness + microbench

**Files:**
- Modify: `autobench/src/bin/run-iter.rs` (imports; unknown-task message; replace `correctness`/`microbench` with kind dispatchers + network/local helpers + `check_expected`/`local_env`)

**Interfaces:**
- Consumes: `run_local_once`, `LocalRun` (Task 2); `TaskSpec.{kind,warmup_env,iters_env,extra_env,expected_metrics,primary_key}` (Task 3); unit-aware `parse_contract_metrics` (Task 1).
- Produces: a `Verdict` with `status:"pass"` + populated `metrics`/`primary` for `Local` cells.

- [ ] **Step 1: Update the imports** — in `autobench/src/bin/run-iter.rs`, replace the import block:

```rust
use hi_perf_autobench::sampling::{
    NetworkRun, Transport, median, parse_contract_metrics, run_network_once,
};
```

  with:

```rust
use std::collections::BTreeMap;

use hi_perf_autobench::sampling::{
    LocalRun, NetworkRun, Transport, median, parse_contract_metrics, run_local_once,
    run_network_once,
};
```

- [ ] **Step 2: Update the unknown-task message** — replace:

```rust
        v.stderr_tail = Some(format!(
            "unknown task {:?}; known: {{rust,go,java}}-network-rtt-{{tcp,udp,quic}}",
            args.task
        ));
```

  with:

```rust
        v.stderr_tail = Some(format!(
            "unknown task {:?}; known: {{rust,go,java}}-network-rtt-{{tcp,udp,quic}}, \
             rust-thread-handoff-{{spin,ring}}",
            args.task
        ));
```

- [ ] **Step 3: Replace the `correctness` and `microbench` functions** — delete the entire existing `fn correctness(v: &mut Verdict, spec: &TaskSpec) { … }` and `fn microbench(v: &mut Verdict, spec: &TaskSpec, samples: usize) { … }` (the two functions spanning from `fn correctness` through the end of `microbench`), and replace them with the following. `correctness_fail`, `microbench_fail`, and `one_network_run` above them are unchanged and still used.

```rust
/// Build the per-run env for a `Local` cell: the cell's warmup/iters env names
/// set to the given counts, plus its fixed `extra_env`.
fn local_env<'a>(spec: &'a TaskSpec, warmup: &'a str, iters: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut env: Vec<(&str, &str)> = vec![(spec.warmup_env, warmup), (spec.iters_env, iters)];
    env.extend_from_slice(spec.extra_env);
    env
}

/// Verify every `expected_metric` is present in `metrics` and strictly > 0.
/// On any miss, fail as a correctness (smoke) or microbench failure.
fn check_expected(
    v: &Verdict,
    spec: &TaskSpec,
    metrics: &BTreeMap<String, f64>,
    stdout: &str,
    is_smoke: bool,
) {
    for key in spec.expected_metrics {
        let detail = match metrics.get(*key) {
            None => format!(
                "expected metric `{key}` missing for {}/{} (got {:?})\n--- stdout ---\n{stdout}",
                spec.focus_area,
                spec.experiment,
                metrics.keys().collect::<Vec<_>>(),
            ),
            Some(&val) if val <= 0.0 => {
                format!("metric `{key}`={val} is not > 0\n--- stdout ---\n{stdout}")
            }
            Some(_) => continue,
        };
        if is_smoke {
            correctness_fail(v.clone(), detail);
        } else {
            microbench_fail(v.clone(), detail);
        }
    }
}

/// Correctness smoke — dispatch on cell kind.
fn correctness(v: &mut Verdict, spec: &TaskSpec) {
    match spec.kind {
        Kind::Network => correctness_network(v, spec),
        Kind::Local => correctness_local(v, spec),
    }
}

/// Network correctness: a tiny two-process run that must exit 0 and yield the
/// cell's expected metrics, all > 0.
fn correctness_network(v: &mut Verdict, spec: &TaskSpec) {
    let env = [
        (spec.warmup_env, SMOKE_WARMUP),
        (spec.iters_env, SMOKE_ITERATIONS),
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
    check_expected(v, spec, &metrics, &run.stdout, true);
}

/// Local correctness: a tiny single-process run that must exit 0 and yield the
/// cell's expected metrics, all > 0.
fn correctness_local(v: &mut Verdict, spec: &TaskSpec) {
    let env = local_env(spec, SMOKE_WARMUP, SMOKE_ITERATIONS);
    let run: LocalRun = match run_local_once(spec.run, spec.run_dir, &env) {
        Ok(r) => r,
        Err(e) => correctness_fail(v.clone(), format!("local run I/O error: {e}")),
    };
    if !run.ok {
        correctness_fail(
            v.clone(),
            format!(
                "correctness run exited non-zero\n--- stderr ---\n{}\n--- stdout ---\n{}",
                run.stderr, run.stdout
            ),
        );
    }
    let metrics = parse_contract_metrics(&run.stdout, spec.focus_area, spec.experiment);
    check_expected(v, spec, &metrics, &run.stdout, true);
}

/// Microbench fitness — dispatch on cell kind.
fn microbench(v: &mut Verdict, spec: &TaskSpec, samples: usize) {
    match spec.kind {
        Kind::Network => microbench_network(v, spec, samples),
        Kind::Local => microbench_local(v, spec, samples),
    }
}

/// Record the per-metric medians across `series` and set `primary` from the
/// cell's `primary_key`, or fail if it is absent.
fn finalize_metrics(v: &mut Verdict, spec: &TaskSpec, series: &BTreeMap<String, Vec<f64>>, n: usize) {
    for (k, vals) in series {
        v.metrics.insert(k.clone(), median(vals));
    }
    match v.metrics.get(spec.primary_key) {
        Some(&p) => v.primary = Some(p),
        None => microbench_fail(
            v.clone(),
            format!("missing primary metric `{}` after {n} samples", spec.primary_key),
        ),
    }
}

/// Network microbench: `samples` two-process runs at standard counts.
fn microbench_network(v: &mut Verdict, spec: &TaskSpec, samples: usize) {
    let n = samples.max(1);
    let env = [
        (spec.warmup_env, BENCH_WARMUP),
        (spec.iters_env, BENCH_ITERATIONS),
    ];
    let mut series: BTreeMap<String, Vec<f64>> = BTreeMap::new();
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
        check_expected(v, spec, &metrics, &run.stdout, false);
        for (k, val) in metrics {
            series.entry(k).or_default().push(val);
        }
    }
    finalize_metrics(v, spec, &series, n);
}

/// Local microbench: `samples` single-process runs at standard counts.
fn microbench_local(v: &mut Verdict, spec: &TaskSpec, samples: usize) {
    let n = samples.max(1);
    let env = local_env(spec, BENCH_WARMUP, BENCH_ITERATIONS);
    let mut series: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for i in 0..n {
        let run = match run_local_once(spec.run, spec.run_dir, &env) {
            Ok(r) => r,
            Err(e) => microbench_fail(v.clone(), format!("local run I/O error (sample {i}): {e}")),
        };
        if !run.ok {
            microbench_fail(
                v.clone(),
                format!(
                    "microbench run (sample {i}) exited non-zero\n--- stderr ---\n{}\n--- stdout ---\n{}",
                    run.stderr, run.stdout
                ),
            );
        }
        let metrics = parse_contract_metrics(&run.stdout, spec.focus_area, spec.experiment);
        check_expected(v, spec, &metrics, &run.stdout, false);
        for (k, val) in metrics {
            series.entry(k).or_default().push(val);
        }
    }
    finalize_metrics(v, spec, &series, n);
}
```

- [ ] **Step 4: Build, lint, and run the crate tests**

Run: `cd autobench && cargo build 2>&1 | tail -5 && cargo test 2>&1 | tail -15 && cargo clippy --all-targets 2>&1 | tail -5 && cargo fmt --check`
Expected: builds; all tests pass; no clippy warnings; no fmt diff. (The `tail_lines` test and the Task 1/2/3 tests all pass.)

- [ ] **Step 5: Real harness smoke on both `Local` cells** (this is the end-to-end verification; the thread-handoff binaries already exist on the branch)

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
cargo run --manifest-path autobench/Cargo.toml --bin run-iter --release -- \
  --task rust-thread-handoff-spin --json --samples 3 | tee /tmp/ab-spin.json
cargo run --manifest-path autobench/Cargo.toml --bin run-iter --release -- \
  --task rust-thread-handoff-ring --json --samples 3 | tee /tmp/ab-ring.json
python3 -c "import json; d=json.load(open('/tmp/ab-spin.json')); assert d['status']=='pass', d; assert d['primary']>0, d; assert 'handoff_rtt_p50_ns' in d['metrics'], d; print('spin OK primary', d['primary'])"
python3 -c "import json; d=json.load(open('/tmp/ab-ring.json')); assert d['status']=='pass', d; assert d['primary']>0, d; assert 'handoff_throughput_ops_per_sec' in d['metrics'], d; print('ring OK primary', d['primary'])"
```
Expected: both print `… OK primary <number>`. `status:"pass"`; spin's `primary` is `handoff_rtt_p50_ns` (a few hundred ns); ring's is `handoff_throughput_ops_per_sec` (tens of millions). Note: each invocation also runs `cargo test` over the rust workspace (Gate A), so it takes ~30–60s.

- [ ] **Step 6: Commit**

```bash
git add autobench/src/bin/run-iter.rs
git commit -m "autobench: run-iter Local correctness + microbench (kind dispatch, expected_metrics, primary_key)"
```

---

## Task 5: Task overlays + CLAUDE.md

**Files:**
- Create: `autobench/tasks/rust-thread-handoff-spin/program.md`, `autobench/tasks/rust-thread-handoff-spin/results.tsv`
- Create: `autobench/tasks/rust-thread-handoff-ring/program.md`, `autobench/tasks/rust-thread-handoff-ring/results.tsv`
- Modify: `autobench/CLAUDE.md`

**Interfaces:** none (docs/data the loop orchestrator reads).

- [ ] **Step 1: Create `autobench/tasks/rust-thread-handoff-spin/program.md`:**

```markdown
# Task: rust-thread-handoff-spin

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Minimize the round-trip handoff latency** of the Rust `thread-handoff`/`spin`
cell — a timer thread ping-pongs a token with a parked responder thread via a
single-slot atomic busy-wait, measured by the existing artifact
(`rust/thread-handoff/spin`), which emits `handoff_rtt_p50` / `handoff_rtt_p99`
/ `handoff_rtt_mean` (`experiment="spin"`, unit `ns`).

## Metrics

| Metric | TSV column | Direction | Role |
|--------|-----------|-----------|------|
| `handoff_rtt_p50` | `handoff_rtt_p50_ns` | minimize | **primary** |
| `handoff_rtt_p99` | `handoff_rtt_p99_ns` | minimize | secondary (must not regress) |
| `handoff_rtt_mean` | `handoff_rtt_mean_ns` | minimize | secondary (must not regress) |

Values are median-of-N across `--samples` single-process runs (note N in
`description`).

## Kind

`Local`. The fitness is a **single process run**: `run-iter` runs the artifact
once with `TH_WARMUP`/`TH_ITERATIONS` and parses its three contract lines.
thread-handoff is single-host, so this local number is fully meaningful (no
cross-host tension). A plateaued champion may later be graduated via a
bench-infra AWS run + `tools/journal` — a manual step, not per-iteration.

## Mutable paths (the only thing you may edit)

- `rust/thread-handoff/spin/src/**`

## Frozen paths (never edit)

- `rust/bench-common/**` — the shared emitter, env-config, and `measure` loop
  (it owns the timing; you can only change the handoff mechanism).
- `docs/result-contract.md`.
- Every other cell (`rust/thread-handoff/{condvar,channel,ring}`, all
  `network-rtt`/`filesystem-write` cells, all of `go/`, `java/`).
- `autobench/**`, all docs/specs.
- The cell's `Cargo.toml` dependency list — never add a dependency (std-only
  beyond `bench-common`).

**Goodhart trap:** the round trip must remain a real cross-thread handoff — the
timer must actually wait for the responder's echo each iteration, and the
responder must service `warmup+iterations` round trips. Do not lower latency by
removing the wait, decoupling the threads, or short-circuiting the ping-pong;
that produces a meaningless number. (The orchestrator reviews each KEEP diff.)

## Noise

Single-thread-pair latency on a shared dev box is scheduler-noisy. **Always use
median-of-N** (`--samples`, default 5); treat within-noise deltas as washes and
re-run before committing a KEEP.

## Gates

1. **build** — `cargo build --release -p thread-handoff-spin` (in `rust/`).
2. **correctness** — a single-process smoke (`TH_WARMUP=20`,
   `TH_ITERATIONS=200`) that must exit 0 and yield `handoff_rtt_p50_ns` /
   `_p99_ns` / `_mean_ns`, all > 0. (A broken handoff deadlocks → timeout.)
3. **microbench (fitness)** — median-of-N single runs at standard counts
   (`TH_WARMUP=2000`, `TH_ITERATIONS=20000`) → metrics + primary.
4. **Gate A (tests)** — `cargo test` over the rust workspace (in `rust/`).

## TSV schema

`autobench/tasks/rust-thread-handoff-spin/results.tsv` (tab-separated):

```
commit	handoff_rtt_p50_ns	handoff_rtt_p99_ns	handoff_rtt_mean_ns	status	description
```

`handoff_rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash.
Values are median-of-N (note N in `description`).
```

- [ ] **Step 2: Create `autobench/tasks/rust-thread-handoff-spin/results.tsv`** (header row only; a literal tab between columns):

```
commit	handoff_rtt_p50_ns	handoff_rtt_p99_ns	handoff_rtt_mean_ns	status	description
```

- [ ] **Step 3: Create `autobench/tasks/rust-thread-handoff-ring/program.md`:**

```markdown
# Task: rust-thread-handoff-ring

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Maximize pipelined handoff throughput** of the Rust `thread-handoff`/`ring`
cell — a bounded single-producer/single-consumer ring buffer (busy-wait, depth
`TH_RING_CAP`) over which a producer thread streams tokens to a consumer,
measured by the existing artifact (`rust/thread-handoff/ring`), which emits
`handoff_throughput` (`experiment="ring"`, unit `ops_per_sec`).

## Metrics

| Metric | TSV column | Direction | Role |
|--------|-----------|-----------|------|
| `handoff_throughput` | `handoff_throughput_ops_per_sec` | maximize | **primary** |

Values are median-of-N across `--samples` single-process runs (note N in
`description`).

## Kind

`Local`. The fitness is a **single process run** with `TH_WARMUP` /
`TH_ITERATIONS` / `TH_RING_CAP=1024`. thread-handoff is single-host, so the
local number is fully meaningful. Graduation (AWS + journal) is a later manual
step, not per-iteration.

## Mutable paths (the only thing you may edit)

- `rust/thread-handoff/ring/src/**`

## Frozen paths (never edit)

- `rust/bench-common/**` (owns the throughput timing/emission).
- `docs/result-contract.md`.
- Every other cell (`rust/thread-handoff/{spin,condvar,channel}`, all
  `network-rtt`/`filesystem-write` cells, all of `go/`, `java/`).
- `autobench/**`, all docs/specs.
- The cell's `Cargo.toml` dependency list — never add a dependency.

**Goodhart trap:** the ring must still deliver every token single-producer/
single-consumer in order. The Gate A `cargo test` includes the SPSC
`spsc_preserves_order_and_count` test — breaking ordering or dropping tokens
fails the gate. Do not "win" by shrinking the real work per handoff.

## Noise

Throughput on a shared dev box varies with scheduling. **Always use
median-of-N**; re-run within-noise deltas before a KEEP.

## Gates

1. **build** — `cargo build --release -p thread-handoff-ring` (in `rust/`).
2. **correctness** — a single-process smoke (`TH_WARMUP=20`,
   `TH_ITERATIONS=200`, `TH_RING_CAP=1024`) that must exit 0 and yield
   `handoff_throughput_ops_per_sec` > 0.
3. **microbench (fitness)** — median-of-N single runs at standard counts
   (`TH_WARMUP=2000`, `TH_ITERATIONS=20000`, `TH_RING_CAP=1024`) → primary.
4. **Gate A (tests)** — `cargo test` over the rust workspace (includes the SPSC
   order+count test — the anti-Goodhart floor for this cell).

## TSV schema

`autobench/tasks/rust-thread-handoff-ring/results.tsv` (tab-separated):

```
commit	handoff_throughput_ops_per_sec	status	description
```

`handoff_throughput_ops_per_sec` is primary (maximize). `status` ∈ keep |
discard | crash. Values are median-of-N (note N in `description`).
```

- [ ] **Step 4: Create `autobench/tasks/rust-thread-handoff-ring/results.tsv`** (header row only; a literal tab between columns):

```
commit	handoff_throughput_ops_per_sec	status	description
```

- [ ] **Step 5: Update `autobench/CLAUDE.md`** — in the opening paragraph, after the sentence naming the pilot, add a line noting `Local` support. Replace:

```
A **task is a matrix cell** `(focus_area, experiment, language)`. The pilot is
`rust-network-rtt-tcp`. Adding a cell is a data-only change (a `TaskSpec` row +
a task overlay) — see "Adding a task".
```

  with:

```
A **task is a matrix cell** `(focus_area, experiment, language)`. The pilot is
`rust-network-rtt-tcp`. Both `Network` (two-process `127.0.0.1`) and `Local`
(single-host, single-process — e.g. `rust-thread-handoff-spin`,
`rust-thread-handoff-ring`) cells are supported. Adding a cell is a data-only
change (a `TaskSpec` row + a task overlay) — see "Adding a task".
```

  And in the "Reading results" section, after the `rust-network-rtt-tcp` columns line, add:

```
**Local cells** (`rust-thread-handoff-spin`: minimize `handoff_rtt_p50_ns`;
`rust-thread-handoff-ring`: maximize `handoff_throughput_ops_per_sec`) run the
artifact as a single process — no server/client — and key metrics `<metric>_<unit>`.
```

- [ ] **Step 6: Verify overlays are well-formed and the tabs are real tabs**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
for f in autobench/tasks/rust-thread-handoff-spin/results.tsv autobench/tasks/rust-thread-handoff-ring/results.tsv; do
  head -1 "$f" | grep -qP '\t' && echo "$f: tabs OK" || echo "$f: NO TABS (fix)";
done
ls autobench/tasks/rust-thread-handoff-spin/program.md autobench/tasks/rust-thread-handoff-ring/program.md
```
Expected: both `tabs OK`; both `program.md` listed.

- [ ] **Step 7: Commit**

```bash
git add autobench/tasks/rust-thread-handoff-spin/ autobench/tasks/rust-thread-handoff-ring/ autobench/CLAUDE.md
git commit -m "autobench: thread-handoff spin/ring task overlays + CLAUDE.md Local note"
```

---

## Final verification

- [ ] **Step 1: Whole-crate green**

Run: `cd autobench && cargo build && cargo test && cargo clippy --all-targets && cargo fmt --check`
Expected: all pass, no warnings, no diff.

- [ ] **Step 2: Both Local cells resolve and pass the harness end-to-end** (re-confirm Task 4 Step 5 still holds after the overlays):

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
cargo run --manifest-path autobench/Cargo.toml --bin run-iter --release -- --task rust-thread-handoff-spin --json --samples 3 | python3 -c "import sys,json; d=json.load(sys.stdin); print('spin', d['status'], d['primary'])"
cargo run --manifest-path autobench/Cargo.toml --bin run-iter --release -- --task rust-thread-handoff-ring --json --samples 3 | python3 -c "import sys,json; d=json.load(sys.stdin); print('ring', d['status'], d['primary'])"
```
Expected: `spin pass <ns>` and `ring pass <ops/sec>`.

- [ ] **Step 3:** The optimization run itself (the capped plateau loop on spin then ring) is a **separate step driven by the human/orchestrator per `autobench/program.md`**, not part of this plan. This plan delivers only the harness support + the two registered cells.
