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
   `_p99_ns` / `_mean_ns`, all > 0. (A broken handoff that fails to round-trip will hang this stage — the loop orchestrator runs `run-iter` under an external `timeout` and treats a kill as a crash/revert.)
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
