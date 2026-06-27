# thread-handoff Benchmark Design

**Date:** 2026-06-27
**Status:** Proposed — awaiting review

## Purpose

Implement the **thread-handoff** focus area in Rust, Java, and Go: measure the cost
of **passing a log entry from one thread to another that is parked waiting for it** —
the in-process analogue of `network-rtt`, and the SMR hot path where the IO/reactor
thread hands a received or committed entry to the apply/consensus worker. Replaces
the current placeholder stub in each language. Emits results in the shared
[result contract](../result-contract.md).

This is a **single-host** focus area (`kind: local`): it runs on node0. There is no
cross-host component — the two endpoints are two threads in one process.

`thread-handoff` is deliberately the in-process mirror of `network-rtt`: the cost
that matters in an SMR system is *wake a sleeping thread and pass it the value*, so
the methodology reuses `network-rtt`'s ping-pong almost verbatim, swapping the socket
for an in-process transport.

## Measurement model — two-thread ping-pong

A **timer** thread sends a token to a **responder** thread and waits for it to come
back; the timer reads the monotonic clock at both ends and records the **round-trip**.

```
timer thread              responder thread
   |  --- token --->          (parked)
   |                          wakes, echoes
   |  <-- token ----
  measure round-trip (one clock: Instant start..end)
```

One ping-pong = **2 handoffs + 2 wakeups**. The timer alone reads the clock at both
ends, so there is no cross-thread timestamp comparison and no clock-skew concern —
exactly why `network-rtt` uses ping-pong, and why the same timed loop is reused here.

The token is a fixed **8-byte sequence number** (a `u64`), not a sized payload.
Handoff cost is dominated by wakeup/synchronization, not 8-vs-64-byte copies, and a
`u64` avoids a copy-vs-reference asymmetry across languages (Rust/Go copy a value
through a channel; a Java generic queue would pass a reference). So there is **no
`*_PAYLOAD_BYTES` knob** (YAGNI).

## Experiment grid

Four runnable artifacts per language, named `thread-handoff-<experiment>`
(mirroring `network-rtt-<experiment>`). All std-only. Each artifact owns its
mechanism, exactly as each `network-rtt` transport crate owns its socket code.

| experiment | mechanism                                            | isolates                                                       |
|------------|------------------------------------------------------|----------------------------------------------------------------|
| `spin`     | atomic single-slot, busy-wait (depth 1)              | floor latency — no OS park, burns a core                       |
| `condvar`  | mutex + condition-variable rendezvous                | the raw **park/unpark + signal** cost ("cost to sleep & wake") |
| `channel`  | idiomatic blocking queue                             | overhead of the **standard tool** (queue bookkeeping on a condvar) |
| `ring`     | bounded SPSC ring, atomic indices, busy-wait, depth N| **pipelining/amortization** — `spin`'s mechanism, depth 1 → N  |

The grid is an **optimization ladder** — each step is a clean single-variable delta:

- `spin` → `condvar` — swap busy-wait for **park/unpark + signal**. This is the cost
  of letting the waiter *sleep* instead of burning a core: the central
  sleep/wakeup number.
- `condvar` → `channel` — swap the minimal hand-rolled rendezvous for the **standard
  blocking queue** each language reaches for. A channel is typically a condvar plus
  queue bookkeeping (and bounded-buffer logic), so this isolates the overhead of
  convenience over a bare rendezvous.
- `spin` → `ring` — hold the mechanism (atomic indices, busy-wait) and go **depth 1 →
  depth N**. This is `spin` pipelined: the per-handoff synchronization cost is
  amortized across many in-flight tokens, converting latency into throughput.

`spin`, `condvar`, and `channel` are the same single-token ping-pong differing only in
the wait/wakeup primitive; `ring` is `spin` differing only in depth. Expected latency
order is **`spin` < `condvar` < `channel`**; `ring` turns `spin`'s mechanism into
sustained throughput.

## Methodology (identical across all three languages)

### Latency experiments (`spin`, `condvar`, `channel`)

These reuse the **existing** `network-rtt` timed loop unchanged (`measure::run` in
Rust and the Go/Java equivalents):

1. **Setup (outside timing).** Build the in-process transport (atomic slot / mutex+
   condvar / blocking queue) and spawn the **responder** thread. The responder loops:
   receive a token, send it straight back. This is the in-process echo responder.
2. **Token.** A single `u64` sequence number, reused — no allocation in the loop.
3. **Warmup.** Run `TH_WARMUP` discarded round-trips (lets the OS scheduler and — for
   Java — the JIT settle).
4. **Measure.** Time `TH_ITERATIONS` round-trips, recording each round-trip's elapsed
   nanoseconds into a **pre-allocated** sample buffer so allocation never enters the
   timed path. One token is outstanding at a time (no pipelining).
5. Compute statistics and emit the latency lines (below). Join the responder thread.

### Throughput experiment (`ring`)

`ring` is pipelined, so a single-token round-trip latency is not meaningful; it
measures sustained one-way handoff rate instead:

1. **Setup (outside timing).** Allocate a bounded SPSC ring of capacity `TH_RING_CAP`
   (atomic head/tail indices, busy-wait on full/empty). Spawn the **consumer** thread,
   which drains tokens as fast as they appear.
2. **Warmup.** Push `TH_WARMUP` tokens through the ring (discarded).
3. **Measure.** Record `t_start`; the producer pushes `TH_ITERATIONS` `u64` tokens
   (busy-waiting when the ring is full); the consumer drains all of them; record
   `t_end` once the consumer has received the last token.
4. `handoff_throughput = TH_ITERATIONS / (t_end − t_start)` handoffs/sec. Join the
   consumer.

### Thread placement

No CPU pinning — std-only in all three languages (Java has no std affinity API).
Placement is left to the OS scheduler. For `spin`/`ring` both threads are always
runnable, so on a **≥2-core host** they naturally occupy two cores; `condvar`/
`channel` park the waiter, so co-location is harmless. The bench fleet's `c6id`
instances are multi-vCPU and satisfy the **≥2-core requirement** (documented).
Go runs with default `GOMAXPROCS` (= `NumCPU`), so its two goroutines get real
parallelism; do not run the Go artifacts with `GOMAXPROCS=1`.

### Statistics (identical formula — required for comparability)

Reuses the existing shared `Stats` in each language (the same code that backs
`network-rtt` and `filesystem-write`). Given `n` round-trip samples of elapsed
nanoseconds, sorted ascending:

- **percentile(p)** = `sorted[ floor( p/100 * (n − 1) ) ]` — nearest-rank, no
  interpolation.
- **mean** = `sum / n`.

`handoff_rtt_p50`/`handoff_rtt_p99` are emitted as integer nanoseconds;
`handoff_rtt_mean` as a (possibly fractional) number of nanoseconds.
`handoff_throughput` is a fractional `ops_per_sec`.

### Configuration (env-var overrides, `TH_` prefix)

| env var          | default  | meaning                                                      |
|------------------|----------|--------------------------------------------------------------|
| `TH_WARMUP`      | `10000`  | discarded warmup round-trips / handoffs                      |
| `TH_ITERATIONS`  | `100000` | measured round-trips (latency) / handoffs (throughput)       |
| `TH_RING_CAP`    | `1024`   | ring capacity (the `ring` experiment only)                   |

Defaults mirror `network-rtt`'s `RTT_WARMUP`/`RTT_ITERATIONS`. `TH_RING_CAP` is
parsed by **all four** artifacts (one uniform config type) but only consumed by
`ring`, exactly as `FSW_BATCH` is parsed by all filesystem-write artifacts and only
consumed by `batch`. Invalid/non-positive numeric values → descriptive message on
**stderr** + non-zero exit, exactly like the `RTT_*`/`FSW_*` config. There is **no
`TH_DIR`-style requirement** — nothing touches disk. The harness sets the same env
vars for all three languages so the comparison is apples-to-apples.

## Output

Result-contract lines per experiment (`focus_area: "thread-handoff"`):

| experiment                 | metric              | unit          | meaning                              | `samples`       |
|----------------------------|---------------------|---------------|--------------------------------------|-----------------|
| `spin`/`condvar`/`channel` | `handoff_rtt_p50`   | `ns`          | median round-trip handoff latency    | `TH_ITERATIONS` |
| `spin`/`condvar`/`channel` | `handoff_rtt_p99`   | `ns`          | 99th-percentile round-trip latency   | `TH_ITERATIONS` |
| `spin`/`condvar`/`channel` | `handoff_rtt_mean`  | `ns`          | mean round-trip latency              | `TH_ITERATIONS` |
| `ring`                     | `handoff_throughput`| `ops_per_sec` | sustained one-way handoffs/sec       | `TH_ITERATIONS` |

`handoff_rtt_*` is the **round-trip** (no `/2` — one ping-pong is 2 handoffs + 2
wakeups, and we make no symmetry assumption, mirroring `network-rtt`'s `rtt_*`).
`ring` sits in its own metric row; the contract aligns per `(focus_area, experiment,
language, metric)`, so a non-uniform metric set across experiments is legal — each
experiment reports the metric its mechanism actually measures.

Per language per run: three latency experiments × 3 lines + `ring` × 1 line =
**10 lines**. The grid aligns on `(focus_area, experiment, language, metric)` exactly
like `network-rtt`.

Example:
```json
{"language":"rust","focus_area":"thread-handoff","experiment":"spin","metric":"handoff_rtt_p50","value":420,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"thread-handoff","experiment":"condvar","metric":"handoff_rtt_p50","value":3100,"unit":"ns","samples":100000}
{"language":"rust","focus_area":"thread-handoff","experiment":"ring","metric":"handoff_throughput","value":12500000.0,"unit":"ops_per_sec","samples":100000}
```

## Cross-language handoff primitives (all std, no new dependencies)

| mechanism | Rust                                  | Go                        | Java                                   |
|-----------|---------------------------------------|---------------------------|----------------------------------------|
| `spin`    | `AtomicU64` single slot, busy-wait    | `sync/atomic`, busy-wait  | `AtomicLong`/`VarHandle`, busy-wait    |
| `condvar` | `Mutex<…>` + `Condvar`                | `sync.Cond`               | `synchronized` + `wait`/`notify`       |
| `channel` | `std::sync::mpsc::sync_channel`       | `chan uint64`             | `SynchronousQueue<Long>`               |
| `ring`    | `AtomicUsize` head/tail + boxed slots | `sync/atomic` head/tail   | `AtomicLong` head/tail + `long[]`/`VarHandle` |

The responder/consumer is a plain `std::thread` (Rust), goroutine (Go), or `Thread`
(Java).

**Java `channel` allocation note.** A generic `SynchronousQueue<Long>` boxes the
`long` per handoff. To keep the handoff allocation-free, the `channel` experiment uses
a reused boxed carrier (or stays within the `Long`-cache range); any unavoidable
boxing is recorded in the result `notes`. `condvar`/`spin`/`ring` store a primitive
`long` field, so they never box.

## Per-language structure

The latency experiments differ only in the wait/wakeup primitive, and `ring` only in
depth; the **shared bench library owns the comparability-critical loop and emission**,
and each artifact is a thin `main` that builds its transport and supplies the
operation. This mirrors `network-rtt` (shared timed loop, per-transport operation) and
`filesystem-write` (shared durable-append harness, per-experiment parameters).

A new `handoff` module is added to each shared library:

- **Config** — parse/validate `TH_*` (`TH_WARMUP`, `TH_ITERATIONS`, `TH_RING_CAP`).
- **Latency path** — reuses the existing timed loop unchanged: warmup + time
  `iterations` round-trips into a pre-allocated buffer, driven by a per-experiment
  `round_trip` closure. `emit_handoff(experiment, samples)` emits the three
  `handoff_rtt_*` lines (parallel to `emit_rtt`).
- **Throughput path** — `emit_handoff_throughput(experiment, ops_per_sec, samples)`
  for `ring`.

**Rust** — replace the single `thread-handoff` workspace member with four:
`rust/thread-handoff/{spin,condvar,channel,ring}` (binaries `thread-handoff-spin`,
`-condvar`, `-channel`, `-ring`). Add a `handoff` module to **`bench-common`**:
- `HandoffConfig::from_env()` — parse `TH_*`, validate.
- `emit_handoff(experiment, &[u64])` — emit the three `handoff_rtt_*` lines.
- `emit_handoff_throughput(experiment, f64, samples)` — emit the throughput line.

The latency `main.rs` files reuse `measure::run` (it already times a
`FnMut() -> io::Result<()>` closure; the in-process round-trip is infallible and
returns `Ok(())`). Each `main.rs` builds its transport, spawns the responder thread,
calls the loop, and emits. Std-only: `std::sync::mpsc`, `Mutex`+`Condvar`,
`std::sync::atomic`, `std::thread`.

**Go** — replace `cmd/thread-handoff` with
`cmd/thread-handoff-{spin,condvar,channel,ring}` (all `package main`). Add to
**`internal/bench`**: `HandoffConfig`, the round-trip timed-loop helper,
`EmitHandoff`, `EmitHandoffThroughput`. Each `main.go` is thin. Mechanisms:
`chan uint64`, `sync.Cond`, `sync/atomic`, goroutine responder. Default `GOMAXPROCS`.

**Java** — replace the `:thread-handoff` subproject with
`:thread-handoff-spin`, `:thread-handoff-condvar`, `:thread-handoff-channel`,
`:thread-handoff-ring` (register all four in `settings.gradle.kts`; each a one-line
`build.gradle.kts` applying `application` + depending on `:common`; `mainClass` =
`net.knego.hiperf.threadhandoff.<exp>.Main`). Add to **`:common`**
(`net.knego.hiperf.common`): `HandoffConfig`, the round-trip driver, and the two emit
paths. Each `Main` is thin. Mechanisms: `SynchronousQueue<Long>`,
`synchronized`/`wait`/`notify`, `AtomicLong`/`VarHandle`, a `Thread` responder.

## bench-infra integration

In `bench-infra/ansible/group_vars/all.yml`:

- Replace the single
  `{ focus_area: thread-handoff, experiment: placeholder, kind: local }` row with
  four `kind: local` rows: `spin`, `condvar`, `channel`, `ring`.
- Add a `th_*` param block (mirroring the `rtt_*`/`fsw_*` blocks) exported into the
  local runs so all three languages use identical parameters: `th_warmup`,
  `th_iterations`, `th_ring_cap` (exported as `TH_WARMUP`/`TH_ITERATIONS`/
  `TH_RING_CAP`).
- No directory or disk setup is needed (unlike `filesystem-write`'s `fsw_dir`). The
  run step exports `TH_*` for the thread-handoff artifacts using the existing
  per-experiment env-export mechanism.

`thread-handoff` runs on node0 (single-host), like `filesystem-write`.

## Error handling

- Invalid/non-positive config → descriptive message to **stderr**, non-zero exit.
  stdout stays results-only (contract requirement).
- A responder/consumer thread that panics or fails is detected on **join** →
  descriptive message to stderr, non-zero exit. No mid-loop retries — retrying would
  distort the latency distribution.

## Testing

- **`Stats` stays unit-tested** in each language (unchanged; already covers the
  comparability-critical percentile/mean logic shared with `network-rtt` and
  `filesystem-write`).
- Add a **ring SPSC correctness** test per language: pushing `N` tokens through the
  bounded ring yields exactly `N` tokens received, each exactly once and in order
  (monotonic sequence). This guards the busy-wait full/empty logic.
- Each artifact is verified by **running** it and confirming well-formed contract
  lines with plausible values: `handoff_rtt_p99 ≥ handoff_rtt_p50`, the expected
  ladder `spin ≤ condvar ≤ channel` on round-trip latency, and `ring`
  `handoff_throughput` materially above `spin`'s implied per-handoff rate (pipelining
  must win). A short smoke run (small `TH_ITERATIONS`) keeps verification fast.
- These are **fitness checks only** and are never journaled — the first journal
  entry for `thread-handoff` comes from a genuine AWS `bench-infra` run, like every
  other focus area.

## Out of scope (YAGNI / future experiments)

- **CPU pinning / affinity** (no std API in Java; would break std-only parity).
- **MPSC/MPMC and multi-producer fan-in** — this focus area is single-producer
  single-consumer (the SMR reactor→worker handoff).
- A separate **`LockSupport.park` / `futex`-direct** rung (covered closely enough by
  `condvar`).
- **Sized payloads** (`TH_PAYLOAD_BYTES`) — handoff cost is dominated by
  synchronization, not the 8-byte token copy.
- **Cross-NUMA placement**, and any **cross-host** component (that is `network-rtt`).
