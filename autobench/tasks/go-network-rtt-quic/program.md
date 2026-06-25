# Task: go-network-rtt-quic

Task overlay for the Go QUIC RTT cell. Read this overlay, then run the loop
per `autobench/program.md`.

## Objective

**Minimize QUIC round-trip latency** for the Go `network-rtt`/`quic` cell —
the synchronous request/response RTT measured over `127.0.0.1` by the existing
artifact (`go/cmd/network-rtt-quic`), which emits the result-contract lines
`rtt_p50` / `rtt_p99` / `rtt_mean` (`experiment="quic"`, `language="go"`, unit
`ns`). The RTT is a write+read over **one long-lived bidirectional QUIC
stream**; the connection and TLS handshake happen once, outside the measured
loop.

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
`run-iter` spawns the artifact as `RTT_MODE=server` (which binds a UDP socket
for QUIC on `RTT_QUIC_PORT`), then probes readiness with a **UDP BIND-PROBE**
— the port becoming `AddrInUse` confirms the server has bound it (a QUIC server
won't echo a raw datagram, so the bind-probe is the reliable readiness signal).
It then runs the artifact as `RTT_MODE=client RTT_HOST=127.0.0.1`, which
establishes a QUIC connection (the handshake retransmits, so a brief server-
startup gap is tolerated) and parses the client's three contract lines.

**The AWS cross-host run is the graduation gate, NOT a per-iteration step.**
When the champion plateaus, trigger a bench-infra cross-host run (server and
client on separate hosts) and record the result in `tools/journal` as the real,
reportable number (see `autobench/program.md` → Graduation).

## Mutable paths (the only thing you may edit)

- `go/cmd/network-rtt-quic/**`

## Frozen paths (never edit)

- `go/internal/bench/**` — the shared emitter, env-config, and measurement loop.
- `docs/result-contract.md` — the JSON line contract.
- Every other benchmark cell (`go/cmd/network-rtt-tcp`, `.../network-rtt-udp`,
  `go/cmd/filesystem-write`, `go/cmd/thread-handoff`, and all of `rust/`,
  `java/`).
- `autobench/**` — the harness itself, including `run-iter`.
- All docs and specs.
- The Go module dependency list — **never add or bump a Go module dependency**;
  the quic-go stack is fixed.

Never weaken the artifact's built-in echo-byte equality check to win a number —
that is the Goodhart trap and it invalidates the comparison grid.

## CONSTRAINTS — QUIC optimization surface is narrow

**The transport I/O is owned by the QUIC library (quic-go), so the raw-socket
busy-poll used by the TCP/UDP cells is NOT reachable.** The optimization surface
is cell-level library configuration only:

- **Transport/connection config** — initial RTT hint, idle timeout, flow-control
  window sizes, stream limits, `MaxIdleTimeout`, `KeepAlivePeriod`.
- **Stream usage** — currently one reused long-lived bidirectional stream; this
  is likely already optimal, but verify.
- **Connection setup** — TLS session tickets, certificate caching (one-time cost,
  not in the hot loop).

**Never add a new dependency.** The quic-go stack is fixed — do not bump or add
modules.

Be honest in the overlay that wins may be small or absent. A clean wash or no-op
that simplifies the code is a keep. Do not contort the cell for a noise-level
gain.

## Noise

QUIC RTT is noisy over loopback — more so than TCP or UDP because the goroutine
scheduler and TLS record processing add jitter. **Always use the harness's
median-of-N** (`--samples`, default 5); never decide on a single sample. When a
delta is within run-to-run noise, re-run `run-iter` for fresh samples before
committing a KEEP.

Note: the QUIC connection setup (certificate generation + TLS handshake) is
one-time and **not** in the measured loop; noise from handshake does not affect
the per-iteration RTT.

## Gates

1. **build** — `go build -o bin/network-rtt-quic ./cmd/network-rtt-quic` (in
   `go/`).
2. **correctness** — a tiny two-process smoke (`RTT_WARMUP=20`,
   `RTT_ITERATIONS=200`) that must exit 0 and yield exactly 3 contract lines for
   `network-rtt`/`quic` with all values > 0.
3. **microbench (fitness)** — median-of-N two-process runs at standard counts
   (`RTT_WARMUP=2000`, `RTT_ITERATIONS=20000`) → metrics + primary.
4. **Gate A (tests)** — `go test ./...` (in `go/`), so an optimization can't
   pass by breaking shared code.

## TSV schema

`autobench/tasks/go-network-rtt-quic/results.tsv` (tab-separated):

```
commit	rtt_p50_ns	rtt_p99_ns	rtt_mean_ns	status	description
```

`rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash. Numeric
values are median-of-N (note N in `description`).
