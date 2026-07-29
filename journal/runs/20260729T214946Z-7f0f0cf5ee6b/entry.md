# 20260729T214946Z-7f0f0cf5ee6b

- commit: 7f0f0cf5ee6b8405dafbb00616720cfee7861357 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
issue #20 A/B variant A: ultima batch cells command-major (SMRC_MULTI_TABLE=0), engine 8831c4e

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / ultima_batch_insert

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 142811.8 | 141566 | 163276 | 64 | 2231.4 |

### smr-collections / ultima_batch_update

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 199171.2 | 197986 | 248891 | 64 | 3112.0 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
