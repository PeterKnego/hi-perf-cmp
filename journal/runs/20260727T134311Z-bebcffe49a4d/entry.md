# 20260727T134311Z-bebcffe49a4d

- commit: bebcffe49a4d64e4d27f8f17992664607df05f05 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
smr-collections re-measure after VersionPin patch: ultima cells at default retention (pin-at-capture), engine tax drops ~1000x -> ~100x flat store

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 100.0 | 65 | 1180 |
| java | 141.6 | 124 | 419 |
| rust | 47.9 | 40 | 89 |

### smr-collections / live_mvcc

| language | snap_count (count) | snap_skipped (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 5 | 5 | 2751256 | 5191006.6 | 5139533 | 5314031 | 233154 | 150.2 | 108 | 297 |
| java | 10 | — | 2751256 | 3830532.1 | 2060790 | 6835494 | 3040996 | 248.0 | 153 | 580 |
| rust | 10 | — | 2751256 | 672768.6 | 666383 | 701494 | 257783 | 130.6 | 99 | 239 |

### smr-collections / live_stw

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| go | 10 | 2751256 | 4899714.7 | 4872143 | 4890272 | 5150733 | 340.6 | 93 | 166 |
| java | 10 | 2751256 | 1967015.7 | 1203249 | 2993056 | 3749283 | 242.6 | 128 | 482 |
| rust | 10 | 2751256 | 638724 | 613276 | 666806 | 745676 | 119.1 | 87 | 164 |

### smr-collections / live_ultima

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| rust | 10 | 2751256 | 1873581.5 | 1847536 | 2094804 | 195992 | 6742.6 | 6568 | 8257 |

### smr-collections / mvcc_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 97.1 | 69 | 636 |
| java | 292.9 | 124 | 387 |
| rust | 77.5 | 68 | 181 |

### smr-collections / mvcc_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 9132648.9 | 8943017 | 11880571 | 2751256 | 5080639.0 | 5062134 | 5376196 | 541517709.0 |
| java | 5419305.3 | 5377818 | 6191710 | 2751256 | 706335.5 | 697455 | 840475 | 3895111980.7 |
| rust | 4807210.8 | 4769165 | 5400393 | 2751256 | 681670.2 | 677248 | 779294 | 4036051577.2 |

### smr-collections / mvcc_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 110.8 | 104 | 187 |
| java | 123.5 | 118 | 214 |
| rust | 101.6 | 97 | 194 |

### smr-collections / snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 9204566.8 | 9110903 | 10936920 | 2751256 | 5011940.9 | 4989322 | 5341090 | 548940226.5 |
| java | 10712814.1 | 6277861 | 53511978 | 2751256 | 790475.7 | 749311 | 1423044 | 3480506753.2 |
| rust | 1340394.9 | 1340566 | 1359021 | 2751256 | 610691.9 | 613738 | 623045 | 4505145767.6 |

### smr-collections / ultima_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| rust | 8404.4 | 8280 | 13844 |

### smr-collections / ultima_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| rust | 8596387.4 | 8554251 | 9345425 | 2751256 | 1452995.7 | 1264965 | 2495733 | 1893506011.8 |

### smr-collections / ultima_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| rust | 6406.1 | 6329 | 6818 |

### smr-collections / update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 103.9 | 94 | 198 |
| java | 132.0 | 121 | 269 |
| rust | 88.6 | 84 | 177 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
