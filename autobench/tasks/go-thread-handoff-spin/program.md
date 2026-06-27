# Task: go-thread-handoff-spin

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Minimize the round-trip handoff latency** of the Go `thread-handoff`/`spin`
cell — a timer goroutine ping-pongs a token with a responder goroutine via a
single-slot atomic busy-wait, measured by the existing artifact
(`go/cmd/thread-handoff-spin`), which emits `handoff_rtt_p50` / `handoff_rtt_p99`
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

`Local`. The fitness is a **single process run**: `run-iter` runs the built
binary once with `TH_WARMUP`/`TH_ITERATIONS` and parses its three contract
lines. thread-handoff is single-host, so this local number is fully meaningful
(no cross-host tension). A plateaued champion may later be graduated via a
bench-infra AWS run + `tools/journal` — a manual step, not per-iteration.

## Mutable paths (the only thing you may edit)

- `go/cmd/thread-handoff-spin/**`

## Frozen paths (never edit)

- `go/internal/bench/**` — the shared emitter, env-config, and `MeasureHandoff`
  loop (it owns the timing; you can only change the handoff mechanism).
- `docs/result-contract.md`.
- Every other cell (`go/cmd/thread-handoff-{condvar,channel,ring}`, all
  `network-rtt`/`filesystem-write` cells, all of `rust/`, `java/`).
- `autobench/**`, all docs/specs.
- `go.mod` — never add a dependency (std-only beyond `internal/bench`).

**Goodhart trap:** the round trip must remain a real cross-goroutine handoff —
the timer must actually wait for the responder's echo each iteration, and the
responder must service `warmup+iterations` round trips. Do not lower latency by
removing the wait, decoupling the goroutines, or short-circuiting the ping-pong;
that produces a meaningless number. (The orchestrator reviews each KEEP diff.)

## Noise

Single-goroutine-pair latency on a shared dev box is scheduler-noisy (run-to-run
core-placement variance can swing the result several-fold). **Always use
median-of-N** (`--samples`); treat within-noise deltas as washes and re-run
before committing a KEEP. If the noise floor swamps the signal locally, graduate
to a dedicated/AWS box for a stable measurement.

## Gates

1. **build** — `go build -o bin/thread-handoff-spin ./cmd/thread-handoff-spin`
   (in `go/`). `go` must be on `PATH`.
2. **correctness** — a single-process smoke (`TH_WARMUP=20`,
   `TH_ITERATIONS=200`) that must exit 0 and yield `handoff_rtt_p50_ns` /
   `_p99_ns` / `_mean_ns`, all > 0.
3. **microbench (fitness)** — median-of-N single runs at standard counts
   (`TH_WARMUP=2000`, `TH_ITERATIONS=20000`) → metrics + primary.
4. **Gate A (tests)** — `go test ./...` (in `go/`).

## TSV schema

`autobench/tasks/go-thread-handoff-spin/results.tsv` (tab-separated):

```
commit	handoff_rtt_p50_ns	handoff_rtt_p99_ns	handoff_rtt_mean_ns	status	description
```

`handoff_rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash.
Values are median-of-N (note N in `description`).
