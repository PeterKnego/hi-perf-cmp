# 20260627T193417Z-003926ca6c91

- commit: 003926ca6c91256453ace803c15bc88ea0196ae5 clean
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
Optimized SPSC ring (rust +cache-pad+cached-index, go same) on AWS c6id; thread-handoff ring throughput wins

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### filesystem-write / batch

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 359294.9 | 43340.9 | 43037 | 50953 |
| java | 346374.1 | 42830.4 | 43002 | 51841 |
| rust | 388018.6 | 42880.1 | 42687 | 52318 |

### filesystem-write / fdatasync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7138.0 | 137476.6 | 127790 | 205140 |
| java | 7285.1 | 134106.5 | 127555 | 179272 |
| rust | 7224.5 | 135572.4 | 130076 | 181087 |

### filesystem-write / fsync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7170.3 | 136331.8 | 129756 | 186361 |
| java | 7163.1 | 136972.6 | 124617 | 206582 |
| rust | 7314.5 | 133944.9 | 130387 | 186140 |

### filesystem-write / prealloc

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 25203.1 | 36867.8 | 36825 | 46218 |
| java | 24831.2 | 37291.0 | 37170 | 46815 |
| rust | 22181.2 | 42520.2 | 37026 | 198250 |

### network-rtt / quic

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 88301.9 | 84275 | 139017 |
| java | 156471.5 | 154490 | 193514 |
| rust | 62551.9 | 68227 | 100740 |

### network-rtt / tcp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 31863.2 | 31332 | 43626 |
| java | 29026.6 | 28547 | 37973 |
| rust | 28890.0 | 28483 | 36727 |

### network-rtt / udp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 29107.7 | 28628 | 39667 |
| java | 27855.2 | 27491 | 36140 |
| rust | 28472.0 | 27928 | 37476 |

### thread-handoff / channel

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 347.5 | 301 | 576 |
| java | 9371.6 | 446 | 24602 |
| rust | 22747.2 | 22752 | 34384 |

### thread-handoff / condvar

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 452.8 | 376 | 986 |
| java | 23859.4 | 24041 | 35350 |
| rust | 22279.7 | 22243 | 32894 |

### thread-handoff / ring

| language | handoff_throughput (ops_per_sec) |
|---|---|
| go | 41883606.3 |
| java | 7068732.4 |
| rust | 410790648.8 |

### thread-handoff / spin

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 197.0 | 194 | 200 |
| java | 220.6 | 220 | 266 |
| rust | 252.6 | 248 | 258 |

## Hypothesis

After the autobench optimization run, the Rust and Go `thread-handoff/ring` cells
carry the SPSC throughput optimization (cache-line-pad `head`/`tail` + LMAX
cached-opposite-index). This run should show those two ring cells massively
improved vs the pre-optimization baseline, with everything else within
cross-run/cross-instance variance.

## Observations

- **The ring optimization graduated cleanly.** vs baseline:
  - `thread-handoff/ring/rust/handoff_throughput`: 28.1M → **410.8M ops/s
    (+1360%)** — *improved*.
  - `thread-handoff/ring/go/handoff_throughput`: 9.8M → **41.9M ops/s (+327%)**
    — *improved*.
  - `thread-handoff/ring/java/handoff_throughput`: 6.57M → 7.07M (+7.5%, within
    noise — Java kept its baseline; the SPSC pattern regressed its JIT'd
    `AtomicLong` and was discarded).
- **The 12 flagged "regressions" are cross-run/cross-instance variance, not code
  regressions.** Only the two ring cells changed code since baseline; everything
  else (network-rtt, filesystem-write, spin/condvar/channel) is byte-identical.
  This fleet instance ran network-rtt ~15-28% *faster* (all "improved") and some
  filesystem-write `sync_p99` tails and `thread-handoff/spin/rust` (182→248ns,
  a noisy ±-variance cell) slower. REGRESSIONS.md is left untouched.
- **disruptor-rs comparison (separate autobench measurement, same box/harness,
  median-of-5):** our optimized `ring` ~**367.6M ops/s** vs the `disruptor` crate
  v4.3 (BusySpin SPSC) ~**148.0M ops/s** — the lean hand-rolled SPSC is **~2.5×**
  the full Disruptor framework, whose per-event handler dispatch / sequence
  barriers / multi-consumer machinery a bare `u64` SPSC handoff doesn't need.
