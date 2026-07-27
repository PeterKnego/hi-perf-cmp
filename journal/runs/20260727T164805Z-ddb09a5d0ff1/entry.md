# 20260727T164805Z-ddb09a5d0ff1

- commit: ddb09a5d0ff17ac5ca65f805eb1b8a9cc80bff65 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
ultima batched-apply cells debut (B=64: 2.3/2.7us per op, 3.7x/2.7x same-run amortization) + bulk_load restore (8.6->2.4ms)

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / live_ultima

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| rust | 10 | 2751256 | 2178514.3 | 2259084 | 2320782 | 206898 | 7018.6 | 6877 | 8713 |

### smr-collections / ultima_batch_insert

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 147984.3 | 146356 | 169065 | 64 | 2312.3 |

### smr-collections / ultima_batch_update

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 170675.0 | 168625 | 186787 | 64 | 2666.8 |

### smr-collections / ultima_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| rust | 8466.9 | 8294 | 16779 |

### smr-collections / ultima_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| rust | 2429764.5 | 2447284 | 2662203 | 2751256 | 2262725.5 | 2549367 | 2892027 | 1215903555.3 |

### smr-collections / ultima_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| rust | 7105.1 | 7025 | 8453 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
