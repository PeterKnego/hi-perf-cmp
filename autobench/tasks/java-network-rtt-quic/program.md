# Task: java-network-rtt-quic

Task overlay for the Java QUIC RTT cell. Read this overlay, then run the loop
per `autobench/program.md`.

## Objective

**Minimize QUIC round-trip latency** for the Java `network-rtt`/`quic` cell —
the synchronous request/response RTT measured over `127.0.0.1` by the existing
artifact (`java/network-rtt-quic`), which emits the result-contract lines
`rtt_p50` / `rtt_p99` / `rtt_mean` (`experiment="quic"`, `language="java"`,
unit `ns`). The RTT is a write+read over **one long-lived bidirectional QUIC
stream**; the connection and TLS handshake happen once, outside the measured
loop.

## Metrics

| Metric | TSV column | Direction | Role |
|--------|-----------|-----------|------|
| `rtt_p50` | `rtt_p50_ns` | minimize | **primary** (drives KEEP/DISCARD) |
| `rtt_p99` | `rtt_p99_ns` | minimize | secondary (must not regress) |
| `rtt_mean` | `rtt_mean_ns` | minimize | secondary (must not regress) |

Values are median-of-N across `--samples` two-process runs (note N in the
`description` column). Java emits `value` as a double (e.g. `34151.0`) and
always includes `notes` — both are valid contract JSON.

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

- `java/network-rtt-quic/src/**`

## Frozen paths (never edit)

- `java/common/**` — the shared `:common` emitter, `Stats`, and `Config`.
- `docs/result-contract.md` — the JSON line contract.
- Every other benchmark cell (`java/network-rtt-tcp`, `.../network-rtt-udp`,
  `java/filesystem-write`, `java/thread-handoff`, and all of `rust/`, `go/`).
- `autobench/**` — the harness itself, including `run-iter`.
- All docs and specs.
- The cell's `build.gradle.kts` dependency list — **never add or bump a
  dependency** in the cell's build file; the Kwik stack is fixed.

Never weaken the artifact's built-in echo-byte equality check to win a number —
that is the Goodhart trap and it invalidates the comparison grid.

## CONSTRAINTS — QUIC optimization surface is narrow

**The transport I/O is owned by the QUIC library (Kwik), so the raw-socket
busy-poll used by the TCP/UDP cells is NOT reachable.** The optimization surface
is cell-level library configuration only:

- **Transport/connection config** — initial RTT hint, idle timeout, flow-control
  window sizes, stream limits, connection keep-alive settings.
- **Stream usage** — currently one reused long-lived bidirectional stream; this
  is likely already optimal, but verify.
- **Connection setup** — TLS session handling, certificate caching (one-time
  cost, not in the hot loop).

**Never add a new dependency.** The Kwik stack is fixed — do not add or bump
entries in the cell's `build.gradle.kts`.

Be honest in the overlay that wins may be small or absent. A clean wash or no-op
that simplifies the code is a keep. Do not contort the cell for a noise-level
gain.

## Noise

QUIC RTT is noisy over loopback — more so than TCP or UDP because the JVM
thread scheduler, TLS record processing, and Kwik's internal event loop all add
jitter. **Always use the harness's median-of-N** (`--samples`, default 5); never
decide on a single sample. When a delta is within run-to-run noise, re-run
`run-iter` for fresh samples before committing a KEEP.

Note: the QUIC connection setup (certificate generation + TLS handshake) is
one-time and **not** in the measured loop; noise from handshake does not affect
the per-iteration RTT.

Note: JVM JIT warmup is real — the 2000 warmup iterations in the fitness gate
are essential; do not reduce them.

## Gates

1. **build** — `./gradlew --quiet :network-rtt-quic:installDist` (in `java/`).
2. **correctness** — a tiny two-process smoke (`RTT_WARMUP=20`,
   `RTT_ITERATIONS=200`) that must exit 0 and yield exactly 3 contract lines for
   `network-rtt`/`quic` with all values > 0.
3. **microbench (fitness)** — median-of-N two-process runs at standard counts
   (`RTT_WARMUP=2000`, `RTT_ITERATIONS=20000`) → metrics + primary. The 2000
   warmup iterations allow the JVM JIT to reach steady state before measurement.
4. **Gate A (tests)** — `./gradlew --quiet :network-rtt-quic:test :common:test`
   (in `java/`), so an optimization can't pass by breaking shared code.

## TSV schema

`autobench/tasks/java-network-rtt-quic/results.tsv` (tab-separated):

```
commit	rtt_p50_ns	rtt_p99_ns	rtt_mean_ns	status	description
```

`rtt_p50_ns` is primary (minimize). `status` ∈ keep | discard | crash. Numeric
values are median-of-N (note N in `description`).
