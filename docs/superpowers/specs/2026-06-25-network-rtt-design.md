# network-rtt Benchmark Design

**Date:** 2026-06-25
**Status:** Proposed — awaiting review

## Purpose

Implement the **network-rtt** benchmark in Rust, Java, and Go: measure
synchronous request/response **round-trip latency** over loopback, for both
**TCP** and **UDP** transports. Replaces the current placeholder stub in each
language. Emits results in the shared [result contract](../result-contract.md).

This measures **latency** (one request outstanding at a time), not throughput.

## Methodology (identical across all three languages)

For each transport (TCP, then UDP):

1. Start an **in-process echo server** bound to `127.0.0.1:<ephemeral>` on its
   own thread (Rust/Java) or goroutine (Go). The server reads a payload and
   writes the identical bytes back.
2. Open **one client connection** to the server.
   - TCP: set **`TCP_NODELAY`** (disable Nagle — otherwise RTT is dominated by
     coalescing delay and the comparison is meaningless).
   - UDP: `connect()` the socket to the server address so send/recv are used.
3. **Warmup:** run `RTT_WARMUP` round trips, discarding timings (lets the JIT,
   branch predictors, and socket buffers settle — matters most for Java).
4. **Measure:** run `RTT_ITERATIONS` round trips. Each round trip:
   - record monotonic start (`Instant::now` / `time.Now` / `System.nanoTime`),
   - write the full `RTT_PAYLOAD_BYTES`-byte payload,
   - read until the full payload has echoed back,
   - record elapsed nanoseconds into a pre-allocated sample array.
5. Compute statistics over the samples and emit result lines.

One request is outstanding at any moment (strict ping-pong). The sample array is
pre-allocated before timing begins so allocation never enters the timed path.

### Statistics (identical formula — required for comparability)

Given `n` samples of elapsed nanoseconds, sorted ascending:

- **percentile(p)** = `sorted[ floor( p/100 * (n - 1) ) ]` — nearest-rank, no
  interpolation. So p50 of 100000 → index 49999; p99 → index 98999.
- **mean** = `sum / n`.

p50 and p99 are emitted as integer nanoseconds; mean as a (possibly fractional)
number of nanoseconds.

### Configuration (env-var overrides, shared names across languages)

| env var             | default | meaning                          |
|---------------------|---------|----------------------------------|
| `RTT_PAYLOAD_BYTES` | `64`    | payload size per request, bytes  |
| `RTT_WARMUP`        | `10000` | discarded warmup round trips     |
| `RTT_ITERATIONS`    | `100000`| measured round trips (= samples) |

Invalid/non-positive values → message on stderr + non-zero exit. The future
harness sets the same env vars for all three languages to compare apples to
apples.

## Output

Six result-contract lines per run (`focus_area: "network-rtt"`, `unit: "ns"`,
`samples: RTT_ITERATIONS`):

| metric         | meaning                    |
|----------------|----------------------------|
| `tcp_rtt_p50`  | TCP median RTT             |
| `tcp_rtt_p99`  | TCP 99th-percentile RTT    |
| `tcp_rtt_mean` | TCP mean RTT               |
| `udp_rtt_p50`  | UDP median RTT             |
| `udp_rtt_p99`  | UDP 99th-percentile RTT    |
| `udp_rtt_mean` | UDP mean RTT               |

Example:
```json
{"language":"go","focus_area":"network-rtt","metric":"tcp_rtt_p50","value":12000,"unit":"ns","samples":100000}
```

## Error handling

- Any connection / IO / bind failure → descriptive message to **stderr**,
  non-zero exit. stdout stays results-only (contract requirement).
- **UDP loss:** loopback UDP is effectively lossless. The client sets a read
  timeout (1s); a timeout is treated as a hard error (stderr + exit), not a
  retransmit — retransmitting would distort the timing distribution.

## Per-language structure

Each language factors the work into small, testable units with identical logic:

**Rust** (`rust/network-rtt/src/`):
- `config.rs` — parse env into a `Config` struct.
- `stats.rs` — `percentile` + `mean` over `&[u64]`. **Unit-tested.**
- `tcp.rs` / `udp.rs` — echo server + client measurement loop, return `Vec<u64>`.
- `main.rs` — wire config → run both transports → emit via existing inline JSON.

Std-only, no external crates.

**Go** (`go/cmd/network-rtt/`, all `package main`):
- `config.go`, `stats.go` (with `stats_test.go`), `tcp.go`, `udp.go`, `main.go`.
- Reuses `internal/result` for emission.

**Java** (`java/network-rtt/src/main/java/net/knego/hiperf/networkrtt/`):
- `Config`, `Stats` (+ `StatsTest` under `src/test/java`), `TcpRtt`, `UdpRtt`,
  `Main`. Reuses `net.knego.hiperf.common.Result`.
- Add JUnit 5 to `network-rtt/build.gradle.kts` test scope for `Stats` tests.

## Testing

- **`Stats` is unit-tested in each language** (percentile indexing, mean,
  small/odd-sized inputs) — it is the pure, comparability-critical logic and the
  one piece that is easy to get subtly wrong across languages.
- The network loop is verified by **running** each benchmark and confirming six
  well-formed contract lines with plausible (non-zero, p99 ≥ p50) values.
- Network/echo correctness is implicitly checked: the client asserts the echoed
  bytes match what it sent (mismatch → hard error).

## Out of scope (YAGNI)

- Multiple concurrent connections / pipelining (that's a throughput benchmark).
- Real (non-loopback) networking, TLS, HTTP framing.
- Cross-host runs — the harness's eventual concern, not this benchmark's.
