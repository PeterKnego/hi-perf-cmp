# Task: go-thread-handoff-ring

A `Local` (single-host) cell. Read this overlay, then run the loop per
`autobench/program.md`.

## Objective

**Maximize pipelined handoff throughput** of the Go `thread-handoff`/`ring`
cell — a bounded single-producer/single-consumer ring buffer (busy-wait, depth
`TH_RING_CAP`) over which a producer goroutine streams tokens to a consumer,
measured by the existing artifact (`go/cmd/thread-handoff-ring`), which emits
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

- `go/cmd/thread-handoff-ring/**`

## Frozen paths (never edit)

- `go/internal/bench/**` (owns the throughput timing/emission).
- `docs/result-contract.md`.
- Every other cell (`go/cmd/thread-handoff-{spin,condvar,channel}`, all
  `network-rtt`/`filesystem-write` cells, all of `rust/`, `java/`).
- `autobench/**`, all docs/specs.
- `go.mod` — never add a dependency.

**Goodhart trap:** the ring must still deliver every token single-producer/
single-consumer in order. The Gate A `go test ./...` includes the SPSC
`TestSPSCPreservesOrderAndCount` test — breaking ordering or dropping tokens
fails the gate. Do not "win" by shrinking the real work per handoff.

## Noise

Throughput on a shared dev box varies with goroutine/core scheduling (run-to-run
swings of several-fold are common). **Always use median-of-N**; re-run
within-noise deltas before a KEEP. If the local noise floor swamps the signal,
graduate to a dedicated/AWS box.

## Gates

1. **build** — `go build -o bin/thread-handoff-ring ./cmd/thread-handoff-ring`
   (in `go/`). `go` must be on `PATH`.
2. **correctness** — a single-process smoke (`TH_WARMUP=20`,
   `TH_ITERATIONS=200`, `TH_RING_CAP=1024`) that must exit 0 and yield
   `handoff_throughput_ops_per_sec` > 0.
3. **microbench (fitness)** — median-of-N single runs at standard counts
   (`TH_WARMUP=2000`, `TH_ITERATIONS=20000`, `TH_RING_CAP=1024`) → primary.
4. **Gate A (tests)** — `go test ./...` (includes the SPSC order+count test —
   the anti-Goodhart floor for this cell).

## TSV schema

`autobench/tasks/go-thread-handoff-ring/results.tsv` (tab-separated):

```
commit	handoff_throughput_ops_per_sec	status	description
```

`handoff_throughput_ops_per_sec` is primary (maximize). `status` ∈ keep |
discard | crash. Values are median-of-N (note N in `description`).
