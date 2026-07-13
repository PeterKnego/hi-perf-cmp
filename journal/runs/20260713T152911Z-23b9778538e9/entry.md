# 20260713T152911Z-23b9778538e9

- commit: 23b9778538e940362af91889e5251650e6d90b4f clean
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
First serialization run (sbe_gen/aeron_sbe/bincode) + full matrix re-measured on c6id.2xlarge

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### filesystem-write / batch

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 360670.2 | 42759.6 | 42691 | 48541 |
| java | 348310.4 | 42379.6 | 42498 | 51716 |
| rust | 388247.3 | 42197.1 | 42214 | 51387 |

### filesystem-write / fdatasync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7665.9 | 127981.1 | 125595 | 167143 |
| java | 7947.2 | 123361.0 | 122050 | 159637 |
| rust | 7633.2 | 128905.9 | 123224 | 177768 |

### filesystem-write / fsync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7983.4 | 122737.7 | 123160 | 165399 |
| java | 7914.5 | 123903.3 | 122331 | 171636 |
| rust | 7813.7 | 125895.1 | 123327 | 173689 |

### filesystem-write / prealloc

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 25408.3 | 36570.0 | 36468 | 44926 |
| java | 25392.4 | 36422.6 | 36570 | 45676 |
| rust | 25748.7 | 36290.8 | 36316 | 46489 |

### network-rtt / quic

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 99756.4 | 94174 | 141253 |
| java | 162359.9 | 160662 | 195919 |
| rust | 77737.0 | 69211 | 117190 |

### network-rtt / tcp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 39894.3 | 39192 | 51388 |
| java | 35168.3 | 34780 | 44104 |
| rust | 36192.6 | 35793 | 45163 |

### network-rtt / udp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 36316.5 | 35784 | 46724 |
| java | 34656.2 | 34290 | 43185 |
| rust | 35426.7 | 34989 | 45332 |

### serialization / aeron_sbe

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 416.2 | 409 | 443 | 78.8 | 57 | 341 | 502 |

### serialization / bincode

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| rust | 536 | 971.7 | 947 | 1034 | 100.1 | 85 | 347 | 482 |

### serialization / sbe_gen

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 415.3 | 408 | 440 | 68.9 | 46 | 350 | 502 |

### thread-handoff / channel

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 361.3 | 323 | 681 |
| java | 6811.1 | 451 | 23675 |
| rust | 429.6 | 394 | 1131 |

### thread-handoff / condvar

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 451.7 | 389 | 899 |
| java | 309.0 | 287 | 475 |
| rust | 316.8 | 281 | 380 |

### thread-handoff / ring

| language | handoff_throughput (ops_per_sec) |
|---|---|
| go | 43222683.3 |
| java | 7770588.6 |
| rust | 421567387.5 |

### thread-handoff / spin

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 204.6 | 202 | 208 |
| java | 291.4 | 298 | 360 |
| rust | 259.4 | 256 | 261 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
