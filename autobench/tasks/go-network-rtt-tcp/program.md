# Task: go-network-rtt-tcp

Task overlay for the Go TCP RTT cell. Read this overlay, then run the loop
per `autobench/program.md`.

## Objective

**Minimize TCP round-trip latency** for the Go `network-rtt`/`tcp` cell — the
synchronous request/response RTT measured over `127.0.0.1` by the existing
artifact (`go/cmd/network-rtt-tcp`), which emits the result-contract lines
`rtt_p50` / `rtt_p99` / `rtt_mean` (`experiment="tcp"`, `language="go"`, unit
`ns`).

## Metrics

| Metric | TSV column | Direction | Role |
|--------|-----------|-----------|------|
| `rtt_p50` | `rtt_p50_ns` | minimize | **primary** (drives KEEP/DISCARD) |
| `rtt_p99` | `rtt_p99_ns` | minimize | secondary (must not regress) |
| `rtt_mean` | `rtt_mean_ns` | minimize | secondary (must not regress) |

Values are median-of-N across `--samples` two-process runs (note N in the
`description` column).

## Kind

`Network`. The fast-loop fitness is a **two-process run over `127.0.0.1`**:
`run-iter` spawns the artifact as `RTT_MODE=server`, waits for it to bind, then
runs it as `RTT_MODE=client RTT_HOST=127.0.0.1` and parses the client's three
contract lines. This exercises the real kernel TCP stack, so *relative*
optimizations are meaningful.

**The AWS cross-host run is the graduation gate, NOT a per-iteration step.**
When the champion plateaus, trigger a bench-infra cross-host run (server and
client on separate hosts) and record the result in `tools/journal` as the real,
reportable number (see `autobench/program.md` → Graduation).

## Mutable paths (the only thing you may edit)

- `go/cmd/network-rtt-tcp/**`

## Frozen paths (never edit)

- `go/internal/bench/**` — the shared emitter, env-config, and measurement loop.
- `docs/result-contract.md` — the JSON line contract.
- Every other benchmark cell (`go/cmd/network-rtt-udp`, `.../network-rtt-quic`,
  `go/cmd/filesystem-write`, `go/cmd/thread-handoff`, and all of `rust/`,
  `java/`).
- `autobench/**` — the harness itself, including `run-iter`.
- All docs and specs.
- The Go module dependency list — **never add a Go module dependency**; the cell
  is intentionally std-library only.

Never weaken the artifact's built-in echo-byte equality check to win a number —
that is the Goodhart trap and it invalidates the comparison grid.

## Noise

Network latency is noisy even over loopback. **Always use the harness's
median-of-N** (`--samples`, default 5); never decide on a single sample. When a
delta is within run-to-run noise, re-run `run-iter` for fresh samples before
committing a KEEP.

## Gates

1. **build** — `go build -o bin/network-rtt-tcp ./cmd/network-rtt-tcp` (in
   `go/`).
2. **correctness** — a tiny two-process smoke (`RTT_WARMUP=20`,
   `RTT_ITERATIONS=200`) that must exit 0 and yield exactly 3 contract lines for
   `network-rtt`/`tcp` with all values > 0.
3. **microbench (fitness)** — median-of-N two-process runs at standard counts
   (`RTT_WARMUP=2000`, `RTT_ITERATIONS=20000`) → metrics + primary.
4. **Gate A (tests)** — `go test ./...` (in `go/`), so an optimization can't
   pass by breaking shared code.

## TSV schema

`autobench/tasks/go-network-rtt-tcp/results.tsv` (tab-separated):

```
commit	rtt_p50_ns	rtt_p99_ns	rtt_mean_ns	status	description
```

`rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash. Numeric
values are median-of-N (note N in `description`).
