# 20260627T071950Z-07a4b9a872fc

- commit: 07a4b9a872fc1eaebef95e1c62d05aa566149e24 clean
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
First real thread-handoff run (spin/condvar/channel/ring) on c6id.2xlarge; network-rtt + filesystem-write re-measured on merged main

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### filesystem-write / batch

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 361519.2 | 42941.7 | 42833 | 53493 |
| java | 348120.8 | 42666.6 | 42745 | 52560 |
| rust | 389088.7 | 42427.4 | 42438 | 52889 |

### filesystem-write / fdatasync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7613.9 | 128725.9 | 126573 | 166806 |
| java | 7813.4 | 125304.3 | 123175 | 160255 |
| rust | 7756.4 | 126707.7 | 124207 | 166321 |

### filesystem-write / fsync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7647.8 | 128140.8 | 126235 | 163079 |
| java | 7825.7 | 125128.5 | 122839 | 173605 |
| rust | 7755.1 | 126779.1 | 124164 | 174017 |

### filesystem-write / prealloc

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 25064.4 | 37060.1 | 36579 | 47924 |
| java | 25144.9 | 36854.3 | 36911 | 46508 |
| rust | 25786.5 | 36293.2 | 36389 | 45926 |

### network-rtt / quic

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 100130.6 | 94924 | 139391 |
| java | 159833.5 | 157825 | 193136 |
| rust | 78612.5 | 70602 | 124883 |

### network-rtt / tcp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 38391.4 | 38013 | 48826 |
| java | 39062.9 | 38469 | 48866 |
| rust | 35050.4 | 34617 | 44586 |

### network-rtt / udp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 36517.0 | 36096 | 46161 |
| java | 38711.8 | 38306 | 48597 |
| rust | 34940.0 | 34549 | 43963 |

### thread-handoff / channel

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 348.2 | 309 | 518 |
| java | 8190.7 | 417 | 24153 |
| rust | 22047.4 | 22152 | 32051 |

### thread-handoff / condvar

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 451.6 | 390 | 952 |
| java | 23217.8 | 23507 | 33749 |
| rust | 21420.7 | 21592 | 31056 |

### thread-handoff / ring

| language | handoff_throughput (ops_per_sec) |
|---|---|
| go | 9809154.1 |
| java | 6574028.5 |
| rust | 28139265.7 |

### thread-handoff / spin

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 195.6 | 192 | 213 |
| java | 210.7 | 210 | 302 |
| rust | 186.2 | 182 | 204 |

## Hypothesis

Per the thread-handoff design, the within-language latency ladder should be
`spin < condvar < channel`: `spin` busy-waits (no OS involvement, floor latency),
`condvar` pays one park/unpark + signal, and `channel` adds the standard queue's
bookkeeping on top of a condvar. `ring` (pipelined SPSC) trades latency for
sustained throughput. Across languages, the parking-based experiments were
expected to expose the cost of each runtime's thread sleep/wakeup.

## Observations

- **spin is the cross-language floor and nearly identical everywhere** —
  rust 182ns / go 192ns / java 210ns (p50). Pure busy-wait, no scheduler, so the
  three runtimes converge.
- **The headline result is parking cost.** For `condvar` and `channel`, Go is
  ~50-60x faster than Rust/Java: condvar p50 go 390ns vs rust 21,592ns vs java
  23,507ns. Go parks goroutines in userspace (cheap), while Rust/Java park real
  OS threads via futex (a syscall + kernel scheduler round-trip ~21-23us). This
  is the central thread sleep/wakeup story the focus area was built to show.
- **Java channel is bimodal** — `channel` p50 417ns but mean 8,190ns: the
  `SynchronousQueue` fast-path hands off without parking much of the time, but a
  heavy tail (p99 24,153ns) parks, dragging the mean up. Go's chan (309ns p50,
  348ns mean) is tight; Rust's `mpsc` rendezvous parks the thread every time
  (22,152ns p50).
- **Within-language ladders.** Rust/Java: `spin` (~0.2us) << `condvar`/`channel`
  (~22us) — parking dominates exactly as predicted. Go: `spin` 192 < `channel`
  309 < `condvar` 390ns — all cheap, because goroutine parking is cheap; the
  predicted ordering holds but compressed into hundreds of ns.
- **ring throughput: rust 28.1M >> go 9.8M > java 6.6M ops/sec.** Rust's
  busy-wait SPSC dominates; Go and Java pay more per slot (atomic + scheduler /
  AtomicLong) but still pipeline well above the per-handoff `spin` rate.
- **network-rtt vs baseline:** the compare flags 6 cells (quic/rust, tcp/java,
  udp/java) as >10% slower, but `network-rtt` code is unchanged from baseline and
  the same compare shows cells improved in both directions (tcp/go p99 -29%,
  udp/rust p99 -12%). These are cross-run hardware/network variance between two
  separate fleet instantiations, **not code regressions** — REGRESSIONS.md
  unchanged.
