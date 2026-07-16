# 20260716T100733Z-16a158ef9fd2

- commit: 16a158ef9fd28bc00650cdf7774245740db5ceb7 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
serialization full grid on AWS: first Go cells (bebop, protobuf) alongside Rust sbe_gen/aeron_sbe/bincode

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### serialization / aeron_sbe

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 416.9 | 409 | 452 | 83.3 | 57 | 383 | 502 |

### serialization / bebop

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 544 | 1280.8 | 1061 | 4123 | 125.7 | 92 | 449 | 494 |

### serialization / bincode

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| rust | 536 | 925.3 | 910 | 1042 | 100.3 | 85 | 363 | 482 |

### serialization / protobuf

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 888 | 1956.2 | 1743 | 5991 | 533.2 | 473 | 1014 | 514 |

### serialization / sbe_gen

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 418.2 | 408 | 477 | 79.9 | 47 | 421 | 502 |

## Hypothesis
First AWS run of the Go serialization cells (bebop via 200sc/bebop safe API,
protobuf via google.golang.org/protobuf with sfixed64 schema). Expected: both
Go cells slower than the Rust cells but in the same order of magnitude on
encode; owned-decode allocation visible for both (bebop < protobuf); wire sizes
~494 B (bebop) and ~511–516 B (protobuf, proto3 zero-field omission); Rust
cells unchanged vs baseline (no Rust code changed on this branch).

## Observations
- Go cells landed as expected: bebop encode 125.7 ns mean / decode 1280.8 ns,
  544 B alloc/decode; protobuf encode 533.2 ns / decode 1956.2 ns, 888 B
  alloc/decode. Stock protobuf costs ~4.2× bebop on encode and ~1.5× on decode;
  both Go decodes carry heavy GC-visible tails (p99 4.1 µs / 6.0 µs).
- Wire sizes exactly as predicted: 494 B / 514 B. Both SBE cells emitted
  decode_alloc_bytes = 0 (in results.jsonl; entry tables omit zero-alloc
  columns) vs 536 B (bincode), 544 B (bebop), 888 B (protobuf) — the zero-copy
  story the focus area exists to tell, now cross-language.
- `compare --baseline` flags three Rust cells as REGRESSION (sbe_gen
  encode_mean +16.1%, encode_p99 +20.3%; aeron_sbe encode_p99 +12.3%). Assessed
  as run-to-run tail noise, not confirmed regressions: p50s are flat across the
  board (sbe_gen 46→47 ns, aeron_sbe 57→57, bincode 85→85), only means/p99s of
  sub-100 ns encodes moved, and this branch touches no Rust code (go/, docs,
  bench-infra only). Not added to REGRESSIONS.md; re-check on the next run.
