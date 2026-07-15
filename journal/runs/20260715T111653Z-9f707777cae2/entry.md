# 20260715T111653Z-9f707777cae2

- commit: 9f707777cae2bef1caff0d1a483bf3e7439e300a dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
smr-collections focus area first AWS run (LOB insert/update/snapshot, Rust/Go/Java)

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### filesystem-write / batch

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 362075.6 | 42762.4 | 42724 | 54681 |
| java | 352559.2 | 41775.2 | 41451 | 50288 |
| rust | 390291.7 | 42495.5 | 42531 | 52343 |

### filesystem-write / fdatasync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7605.4 | 129042.1 | 124857 | 162811 |
| java | 7846.1 | 124976.9 | 122527 | 168959 |
| rust | 7094.8 | 138827.8 | 123943 | 226224 |

### filesystem-write / fsync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7726.7 | 126996.6 | 124521 | 171195 |
| java | 7690.2 | 127323.5 | 126044 | 171686 |
| rust | 5900.8 | 166995.1 | 166044 | 258731 |

### filesystem-write / prealloc

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 23244.3 | 40239.8 | 36657 | 58046 |
| java | 25484.7 | 36369.2 | 36590 | 45917 |
| rust | 24900.2 | 37689.5 | 36375 | 45452 |

### network-rtt / quic

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 101736.5 | 96847 | 138127 |
| java | 158355.1 | 156778 | 196455 |
| rust | 76338.8 | 73334 | 113883 |

### network-rtt / tcp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 40155.7 | 39755 | 51318 |
| java | 37791.7 | 37412 | 46808 |
| rust | 40868.8 | 40314 | 51027 |

### network-rtt / udp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 35324.7 | 34825 | 44425 |
| java | 34723.4 | 34349 | 42784 |
| rust | 36887.6 | 36368 | 47379 |

### serialization / aeron_sbe

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 416.1 | 409 | 443 | 81.2 | 57 | 365 | 502 |

### serialization / bincode

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| rust | 536 | 917.6 | 901 | 987 | 101.1 | 84 | 362 | 482 |

### serialization / sbe_gen

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 415.7 | 408 | 452 | 74.4 | 46 | 394 | 502 |

### smr-collections / insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 91.3 | 63 | 1181 |
| java | 140.9 | 124 | 440 |
| rust | 46.9 | 40 | 89 |

### smr-collections / snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 8891024.2 | 8693277 | 12111449 | 2751256 | 4977575.3 | 4954288 | 5296235 | 552730163.6 |
| java | 10666583.4 | 6192599 | 54037671 | 2751256 | 619246.7 | 560654 | 1188419 | 4442907880.4 |
| rust | 1331532.5 | 1315415 | 1557783 | 2751256 | 610179.0 | 609977 | 654320 | 4508932634.4 |

### smr-collections / update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 104.1 | 95 | 186 |
| java | 128.5 | 117 | 255 |
| rust | 86.4 | 83 | 172 |

### thread-handoff / channel

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 361.7 | 321 | 682 |
| java | 5519.3 | 420 | 23681 |
| rust | 441.2 | 401 | 1137 |

### thread-handoff / condvar

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 444.7 | 398 | 854 |
| java | 366.9 | 364 | 467 |
| rust | 330.7 | 293 | 389 |

### thread-handoff / ring

| language | handoff_throughput (ops_per_sec) |
|---|---|
| go | 38723308.0 |
| java | 6101939.5 |
| rust | 432949163.1 |

### thread-handoff / spin

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 183.6 | 182 | 188 |
| java | 227.9 | 207 | 304 |
| rust | 258.8 | 256 | 261 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
