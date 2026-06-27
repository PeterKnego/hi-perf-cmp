# Task: java-thread-handoff-ring

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Maximize pipelined handoff throughput** of the Java `thread-handoff`/`ring`
cell — a bounded single-producer/single-consumer ring buffer (busy-wait, depth
`TH_RING_CAP`, `AtomicLong` head/tail) over which a producer thread streams
tokens to a consumer, measured by the existing artifact
(`java/thread-handoff-ring`), which emits `handoff_throughput`
(`experiment="ring"`, unit `ops_per_sec`).

## Metrics

| Metric | TSV column | Direction | Role |
|--------|-----------|-----------|------|
| `handoff_throughput` | `handoff_throughput_ops_per_sec` | maximize | **primary** |

Values are median-of-N across `--samples` single-process runs (note N in
`description`).

## Kind

`Local`. The fitness is a **single process run** (built via `installDist`) with
`TH_WARMUP` / `TH_ITERATIONS` / `TH_RING_CAP=1024`. thread-handoff is
single-host, so the local number is fully meaningful. Graduation (AWS + journal)
is a later manual step, not per-iteration.

## Mutable paths (the only thing you may edit)

- `java/thread-handoff-ring/src/**`

## Frozen paths (never edit)

- `java/common/src/**` (owns the throughput timing/emission).
- `docs/result-contract.md`.
- Every other cell (`java/thread-handoff-{spin,condvar,channel}`, all
  `network-rtt`/`filesystem-write` subprojects, all of `rust/`, `go/`).
- `autobench/**`, all docs/specs.
- The subproject's dependency list — never add a dependency.

**Goodhart trap:** the ring must still deliver every token single-producer/
single-consumer in order. The Gate A `:thread-handoff-ring:test` includes the
`SpscTest` order+count test — breaking ordering or dropping tokens fails the
gate. Do not "win" by shrinking the real work per handoff.

## Noise

Throughput on a shared dev box varies with thread/core scheduling and JIT
(run-to-run swings of several-fold are common). **Always use median-of-N**;
re-run within-noise deltas before a KEEP. If the local noise floor swamps the
signal, graduate to a dedicated/AWS box.

## Gates

1. **build** — `./gradlew --quiet :thread-handoff-ring:installDist` (in `java/`).
2. **correctness** — a single-process smoke (`TH_WARMUP=20`,
   `TH_ITERATIONS=200`, `TH_RING_CAP=1024`) that must exit 0 and yield
   `handoff_throughput_ops_per_sec` > 0.
3. **microbench (fitness)** — median-of-N single runs at standard counts
   (`TH_WARMUP=2000`, `TH_ITERATIONS=20000`, `TH_RING_CAP=1024`) → primary.
4. **Gate A (tests)** — `./gradlew --quiet :thread-handoff-ring:test :common:test`
   (includes the SPSC order+count test — the anti-Goodhart floor for this cell).

## TSV schema

`autobench/tasks/java-thread-handoff-ring/results.tsv` (tab-separated):

```
commit	handoff_throughput_ops_per_sec	status	description
```

`handoff_throughput_ops_per_sec` is primary (maximize). `status` ∈ keep |
discard | crash. Values are median-of-N (note N in `description`).
