# autobench — Autoresearch Optimization Loop — Design

**Date:** 2026-06-25
**Status:** Proposed — awaiting review

## Purpose

A Claude-Code-orchestrated autoresearch loop (Karpathy shape, modeled on
`../ultima_cluster/uc_autobench` and `../ultima_db/autobench`) that **optimizes a
benchmark experiment's implementation** and autonomously keeps wins / reverts
losses, logging every iteration to a committed `results.tsv`, until the human
presses Ctrl-C.

Scope of this spec: build the **full framework** but wire up **one pilot task**
(`rust network-rtt-tcp`) to prove it end-to-end. Adding more cells is then a
data-only change (a `TaskSpec` row + a task overlay).

## How it maps onto hi-perf-cmp

- **A task is a matrix cell `(focus_area, experiment, language)`** — pilot:
  `rust-network-rtt-tcp`. Mutable = that cell's source (`rust/network-rtt/tcp/src/**`);
  frozen = `rust/bench-common/**`, the result contract, every other cell,
  `autobench/**`, and all docs/specs.
- **The fitness binary is the experiment artifact we already built** — it emits
  `rtt_p50/p99/mean` per the result contract. autobench does not add a separate
  microbench; it runs the real artifact.
- **`run-iter` is polyglot** — the per-stage commands are *data* in the
  `TaskSpec` (a Rust/Go/Java cell uses cargo / go / gradlew), so the harness
  dispatches without forking per language.

### Relationship to what already exists (no duplication)

- **tools/journal** stays the curated, cross-host, cross-language record + the
  regression-vs-baseline comparison. autobench does **not** reimplement baseline
  gating; the loop decides KEEP/DISCARD from the **champion row in its own
  `results.tsv`**, and a graduated champion is recorded into the journal via a
  real AWS run.
- **bench-infra** stays the real cross-host AWS runner. It is the autobench
  **confirmation gate** for network cells (see below), not a per-iteration step.

### The network cross-host tension (resolved)

Loopback-in-process is meaningless for network perf, but an autoresearch loop
needs *fast local* iterations. Resolution:
- **Fast loop fitness = two-process run over `127.0.0.1`** (separate server +
  client processes — exercises the real kernel TCP stack, so *relative*
  optimizations are meaningful), median-of-N.
- **AWS cross-host is the periodic confirmation gate**, run manually via
  bench-infra when a champion plateaus or on demand, and recorded in the journal
  as the real result. It is **not** run every iteration (too slow/costly).
- Single-host focus areas (filesystem-write, thread-handoff, shared-memory-ipc)
  have no such tension — their local fitness is fully meaningful (a future
  `kind: Local` task runs the artifact once instead of server+client).

## Layout

```
autobench/                      (repo root, standalone Rust crate — NOT in rust/ workspace)
├── Cargo.toml                  # bin run-iter; deps serde/serde_json/clap/wait-timeout/tempfile
├── program.md                  # the autonomous loop orchestration (Claude Code reads this)
├── CLAUDE.md                   # how to start a run, read results, run-iter reference
├── src/
│   ├── lib.rs
│   ├── task_spec.rs            # registry: cell + per-stage commands + primary metric/direction/kind
│   ├── verdict.rs              # Direction enum + champion compare + JSON verdict types
│   ├── sampling.rs             # spawn artifact, parse contract JSON lines, median-of-N, two-process driver
│   └── bin/run-iter.rs         # the gated harness (thin; logic lives in lib modules)
├── tasks/
│   ├── TEMPLATE.md
│   └── rust-network-rtt-tcp/
│       ├── program.md          # pilot task overlay
│       └── results.tsv         # header only at first
```

## `run-iter` — the measurement harness

`run-iter --task <id> --json [--baseline-primary <v>] [--samples N]`

Looks up the `TaskSpec`, then runs stages, hard-timeout-bounded (`wait-timeout`),
emitting one JSON verdict on stdout (exit 0 even on failure, like the reference):

1. **build** — run the cell's build command (pilot: `cargo build --release -p
   network-rtt-tcp` in `rust/`).
2. **correctness** — the anti-Goodhart floor. For a `Network` cell: a two-process
   smoke at tiny iteration counts that must exit 0 and emit 3 well-formed contract
   lines with `experiment=tcp` and positive values. The artifact already asserts
   echo-byte equality internally and crashes on mismatch, so you cannot fake a
   low RTT without breaking this. Plus the cell's test command if any.
3. **microbench (fitness)** — `Network` cell: start the artifact in `RTT_MODE=server`
   as a child, wait for bind, run `RTT_MODE=client RTT_HOST=127.0.0.1` at the
   standard config, capture its 3 lines; repeat `--samples` (default 5) times and
   take the **median** of the primary metric. Records all of p50/p99/mean.
4. **Gate A (tests)** — the cell's broader test suite (pilot: `cargo test` over the
   rust workspace) so an optimization can't pass by breaking shared code.

There is **no per-iteration Gate B** — the AWS confirmation gate is a separate
manual graduation step (documented in `program.md`).

### JSON verdict

`{ status, stage, duration_s{build,correctness,microbench,tests}, metrics{rtt_p50_ns,
rtt_p99_ns,rtt_mean_ns}, primary, stderr_tail }`. Statuses mirror the reference:
`pass | build_failed | correctness_failed | microbench_failed | tests_failed |
timeout | unknown_task`.

### TaskSpec (data-driven, polyglot)

```rust
pub struct TaskSpec {
    pub task: &'static str,          // "rust-network-rtt-tcp"
    pub language: &'static str,      // "rust" | "go" | "java"
    pub focus_area: &'static str,    // "network-rtt"
    pub experiment: &'static str,    // "tcp"
    pub kind: Kind,                  // Network (two-process) | Local (single run)
    pub build: &'static [&'static str],   // cmd argv
    pub build_dir: &'static str,          // cwd relative to repo root
    pub run: &'static [&'static str],      // how to launch the artifact (RTT_MODE via env)
    pub run_dir: &'static str,
    pub gate_a: &'static [&'static str],   // test command
    pub gate_a_dir: &'static str,
    pub primary_metric: &'static str,      // "rtt_p50"
    pub direction: Direction,              // Minimize
}
```
Adding a cell = a new `TaskSpec` row + a task overlay. (Go/Java rows differ only
in the `build`/`run`/`gate_a` argv — e.g. `go build`/`go/bin/<art>`/`go test`, or
`gradlew :…:installDist`/the launcher/`gradlew test`.)

## program.md — the loop (autonomy)

Mirrors the reference contract: **the human is not at the keyboard** — once
started, loop continuously and silently, never ask questions, never pause for
approval, KEEP/DISCARD per the rule and commit every iteration; the only stop is
Ctrl-C. Per-iteration sequence:

1. `tail` the task's `results.tsv`; identify the champion (best primary among
   `status=keep`).
2. **Hypothesis subagent (opus)** → one-line hypothesis + file sketch (champion
   description + last ~10 rows + any hotspot summary).
3. **Implementation subagent (sonnet; escalate to opus** for unsafe/lock-free/
   syscall-ordering work or after two failed builds) edits ONLY the cell's
   mutable paths.
4. Run `run-iter --task <id> --json --baseline-primary <champion>` → parse.
5. Decide: `pass` & primary improved beyond noise (median-of-N) & correctness/Gate
   A green → **KEEP** (append TSV row, commit); no improvement → **DISCARD**
   (`git checkout -- <mutable>`, append discard row, commit the TSV); `*_failed`/
   `timeout` → **haiku triage**, ≤2 fix attempts, else **crash** row + revert.
6. GOTO 1.

Subagent dispatch is **mandatory** (the orchestrator never reads source/raw bench
output itself — only subagent summaries) so a run survives hours without context
exhaustion. **Graduation:** when a champion plateaus, the human (or an explicit
instruction) triggers a bench-infra AWS cross-host run for the cell and records it
in the journal — the real, reportable number.

results.tsv schema (pilot):
```
commit	rtt_p50_ns	rtt_p99_ns	rtt_mean_ns	status	description
```
`rtt_p50_ns` is primary (minimize); values are median-of-N (N noted in
`description`); `status` ∈ keep|discard|crash.

## CLAUDE.md (autobench/)

How to start a run (`Run the autobench loop for task rust-network-rtt-tcp per
autobench/program.md.`), how to read `results.tsv` / find the champion, the
`run-iter` flag + JSON reference, the subagent/model table, and the "Adding a
task" pointer to `TEMPLATE.md`.

## Testing

- **TDD the pure logic:** contract-line parsing (incl. Java `0.0` floats + notes),
  median-of-N, `Direction` champion comparison (improve/regress/within-noise),
  `task_spec` resolution, verdict JSON shaping.
- **Harness smoke (real):** `run-iter --task rust-network-rtt-tcp --json` runs
  build → correctness → two-process microbench → cargo test and emits a verdict
  with `status:"pass"` and populated `metrics`/`primary`. Verified by hand.
- autobench crate stays clippy/fmt-clean.

## Out of scope (YAGNI)

- Wiring all 9 network cells / the single-host areas (data-only follow-ups once
  the pilot proves the framework).
- A standalone `make perf/check` baseline gate — that role is the journal's.
- Automated per-iteration AWS gating (manual graduation step instead).
- Profiling/flamegraph automation (the reference's periodic profiler) — can be
  added later; the loop works without it.
