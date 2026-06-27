# Task: java-thread-handoff-spin

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Minimize the round-trip handoff latency** of the Java `thread-handoff`/`spin`
cell — a timer thread ping-pongs a token with a responder thread via a
single-slot `AtomicLong` busy-wait, measured by the existing artifact
(`java/thread-handoff-spin`), which emits `handoff_rtt_p50` / `handoff_rtt_p99`
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

`Local`. The fitness is a **single process run**: `run-iter` builds the cell via
`installDist` (a launcher that `exec`s the JVM, so stdout carries only the
contract lines) and runs it once with `TH_WARMUP`/`TH_ITERATIONS`. thread-handoff
is single-host, so this local number is fully meaningful. A plateaued champion
may later be graduated via a bench-infra AWS run + `tools/journal`.

## Mutable paths (the only thing you may edit)

- `java/thread-handoff-spin/src/**`

## Frozen paths (never edit)

- `java/common/src/**` — the shared `Result`/`Stats`/`Handoff` (it owns the
  timing; you can only change the handoff mechanism).
- `docs/result-contract.md`.
- Every other cell (`java/thread-handoff-{condvar,channel,ring}`, all
  `network-rtt`/`filesystem-write` subprojects, all of `rust/`, `go/`).
- `autobench/**`, all docs/specs.
- The subproject's dependency list — never add a dependency (std-only beyond
  `:common`).

**Goodhart trap:** the round trip must remain a real cross-thread handoff — the
timer must actually wait for the responder's echo each iteration, and the
responder must service `warmup+iterations` round trips. Do not lower latency by
removing the wait, decoupling the threads, or short-circuiting the ping-pong.
(The orchestrator reviews each KEEP diff.)

## Noise

Single-thread-pair latency on a shared dev box is scheduler- and JIT-noisy
(run-to-run swings of several-fold are common). **Always use median-of-N**
(`--samples`); treat within-noise deltas as washes and re-run before a KEEP. If
the local noise floor swamps the signal, graduate to a dedicated/AWS box.

## Gates

1. **build** — `./gradlew --quiet :thread-handoff-spin:installDist` (in `java/`).
2. **correctness** — a single-process smoke (`TH_WARMUP=20`,
   `TH_ITERATIONS=200`) that must exit 0 and yield `handoff_rtt_p50_ns` /
   `_p99_ns` / `_mean_ns`, all > 0.
3. **microbench (fitness)** — median-of-N single runs at standard counts
   (`TH_WARMUP=2000`, `TH_ITERATIONS=20000`) → metrics + primary.
4. **Gate A (tests)** — `./gradlew --quiet :thread-handoff-spin:test :common:test`.

## TSV schema

`autobench/tasks/java-thread-handoff-spin/results.tsv` (tab-separated):

```
commit	handoff_rtt_p50_ns	handoff_rtt_p99_ns	handoff_rtt_mean_ns	status	description
```

`handoff_rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash.
Values are median-of-N (note N in `description`).
