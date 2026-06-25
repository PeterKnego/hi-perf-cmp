# Task: rust-network-rtt-udp

Task overlay for the Rust UDP RTT cell. Read this overlay, then run the loop
per `autobench/program.md`.

## Objective

**Minimize UDP round-trip latency** for the Rust `network-rtt`/`udp` cell — the
synchronous request/response RTT measured over `127.0.0.1` by the existing
artifact (`rust/network-rtt/udp`), which emits the result-contract lines
`rtt_p50` / `rtt_p99` / `rtt_mean` (`experiment="udp"`, unit `ns`).

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
`run-iter` spawns the artifact as `RTT_MODE=server`, probes readiness by
sending a probe datagram and waiting for the echo server to reflect it (UDP has
no connection to probe, unlike TCP), then runs it as
`RTT_MODE=client RTT_HOST=127.0.0.1` and parses the client's three contract
lines. The port is passed via `RTT_UDP_PORT`. This exercises the real kernel UDP
stack, so *relative* optimizations are meaningful.

**The AWS cross-host run is the graduation gate, NOT a per-iteration step.**
When the champion plateaus, trigger a bench-infra cross-host run (server and
client on separate hosts) and record the result in `tools/journal` as the real,
reportable number (see `autobench/program.md` → Graduation).

## Mutable paths (the only thing you may edit)

- `rust/network-rtt/udp/src/**`

## Frozen paths (never edit)

- `rust/bench-common/**` — the shared emitter, env-config, and measurement loop.
- `docs/result-contract.md` — the JSON line contract.
- Every other benchmark cell (`rust/network-rtt/tcp`, `.../quic`,
  `rust/filesystem-write`, `rust/thread-handoff`, and all of `go/`, `java/`).
- `autobench/**` — the harness itself, including `run-iter`.
- All docs and specs.
- The cell's `Cargo.toml` dependency list — **never add a dependency**; the cell
  is intentionally std-only (beyond `bench-common`).

Never weaken the artifact's built-in echo-byte equality check to win a number —
that is the Goodhart trap and it invalidates the comparison grid.

**CRITICAL — recv-timeout / datagram-loss-detection semantic must be
preserved.** The client currently uses a bounded recv timeout (~1 s) and treats
loss as a hard error. Any optimization (e.g. busy-polling the recv) must NOT
silently spin forever on a lost datagram — keep a bounded wait that still
surfaces loss as an error. Never weaken the echo-byte equality check.

## Noise

UDP RTT is noisy over loopback. **Always use the harness's median-of-N**
(`--samples`, default 5); never decide on a single sample. When a delta is
within run-to-run noise, re-run `run-iter` for fresh samples before committing
a KEEP.

**UDP-specific caution:** datagram loss is possible even over loopback under
load. The cell treats a recv timeout as a hard error (no retransmit); a
`correctness_failed` or `microbench_failed` status from loss is a signal to
re-run, not a code defect — but repeated loss indicates a bug.

## Gates

1. **build** — `cargo build --release -p network-rtt-udp` (in `rust/`).
2. **correctness** — a tiny two-process smoke (`RTT_WARMUP=20`,
   `RTT_ITERATIONS=200`) that must exit 0 and yield exactly 3 contract lines for
   `network-rtt`/`udp` with all values > 0.
3. **microbench (fitness)** — median-of-N two-process runs at standard counts
   (`RTT_WARMUP=2000`, `RTT_ITERATIONS=20000`) → metrics + primary.
4. **Gate A (tests)** — `cargo test` over the rust workspace (in `rust/`), so an
   optimization can't pass by breaking shared code.

## TSV schema

`autobench/tasks/rust-network-rtt-udp/results.tsv` (tab-separated):

```
commit	rtt_p50_ns	rtt_p99_ns	rtt_mean_ns	status	description
```

`rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash. Numeric
values are median-of-N (note N in `description`).
