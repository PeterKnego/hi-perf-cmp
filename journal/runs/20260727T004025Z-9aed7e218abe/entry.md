# 20260727T004025Z-9aed7e218abe

- commit: 9aed7e218abeaa3500f1f9fc7de9566fc6c4fe57 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
smr-collections MVCC grid first AWS run: STW vs chunked-CoW vs ultima_db (insert/update/snapshot + live snapshot-under-writes), Rust/Go/Java

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 93.7 | 62 | 1183 |
| java | 211.3 | 182 | 585 |
| rust | 48.1 | 40 | 93 |

### smr-collections / live_mvcc

| language | snap_count (count) | snap_skipped (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 8 | 2 | 2751256 | 5224268.2 | 5178599 | 5407560 | 202956 | 203.7 | 163 | 523 |
| java | 10 | — | 2751256 | 3538032.6 | 3156183 | 4779971 | 2681888 | 191.9 | 114 | 468 |
| rust | 10 | — | 2751256 | 667021.2 | 658762 | 681586 | 161994 | 140.5 | 103 | 248 |

### smr-collections / live_stw

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| go | 10 | 2751256 | 4973914.4 | 4938893 | 4995677 | 5241110 | 396.2 | 138 | 487 |
| java | 10 | 2751256 | 1963104 | 1163243 | 3085712 | 3913270 | 245.2 | 128 | 482 |
| rust | 10 | 2751256 | 621913.6 | 611345 | 641675 | 721042 | 119.0 | 88 | 150 |

### smr-collections / live_ultima

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| rust | 10 | 2751256 | 2837781.7 | 2811769 | 2890208 | 297355 | 110817.2 | 109059 | 126989 |

### smr-collections / mvcc_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 98.3 | 67 | 1205 |
| java | 205.6 | 112 | 255 |
| rust | 77.4 | 68 | 174 |

### smr-collections / mvcc_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 9199987.4 | 8963242 | 11889268 | 2751256 | 5099583.8 | 5051327 | 5555654 | 539505985.6 |
| java | 5410386.5 | 5380145 | 6215445 | 2751256 | 714953.5 | 712003 | 830959 | 3848160939.0 |
| rust | 5000323.5 | 4890082 | 5716012 | 2751256 | 706399.4 | 688553 | 841789 | 3894759835.6 |

### smr-collections / mvcc_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 113.0 | 106 | 191 |
| java | 132.5 | 121 | 277 |
| rust | 101.4 | 97 | 191 |

### smr-collections / snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 8589833.5 | 8414937 | 11700475 | 2751256 | 4973653.8 | 4955762 | 5184799 | 553165966.9 |
| java | 11308991.4 | 7088343 | 53957074 | 2751256 | 1126418.6 | 1119772 | 1691984 | 2442480964.8 |
| rust | 1335050.9 | 1322180 | 1379237 | 2751256 | 610574.7 | 609293 | 628846 | 4506010548.3 |

### smr-collections / ultima_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| rust | 103519.5 | 108066 | 128509 |

### smr-collections / ultima_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| rust | 7729075.6 | 7788133 | 8286075 | 2751256 | 1671906.5 | 1584480 | 3092663 | 1645580072.7 |

### smr-collections / ultima_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| rust | 113299.7 | 111452 | 130160 |

### smr-collections / update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 100.9 | 94 | 178 |
| java | 154.3 | 131 | 481 |
| rust | 84.2 | 82 | 162 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
