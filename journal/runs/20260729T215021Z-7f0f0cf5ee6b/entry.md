# 20260729T215021Z-7f0f0cf5ee6b

- commit: 7f0f0cf5ee6b8405dafbb00616720cfee7861357 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
issue #20 A/B variant B: ultima batch cells open_tables3/2 (SMRC_MULTI_TABLE=1); same host, -12 to -13% mean / -22% p99 update

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / ultima_batch_insert

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 125112.2 | 123503 | 148910 | 64 | 1954.9 |

### smr-collections / ultima_batch_update

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 174132.3 | 172220 | 194789 | 64 | 2720.8 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
