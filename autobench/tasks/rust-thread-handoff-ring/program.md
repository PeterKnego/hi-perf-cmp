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
