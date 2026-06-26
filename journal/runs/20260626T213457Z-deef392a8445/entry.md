# 20260626T213457Z-deef392a8445

- commit: deef392a8445a19dbbcf6d2ec3524957fdb092b5 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
First real AWS filesystem-write run (c6id.2xlarge NVMe): fsync/fdatasync/prealloc/batch ladder across rust/go/java. Note: Java fdatasync is an outlier (slower than its own fsync) — likely a single-run JIT/GC artifact.

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### filesystem-write / batch

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 355853.8 | 42967.3 | 42817 | 53033 |
| java | 345544.0 | 42871.8 | 42631 | 54408 |
| rust | 386616.4 | 42987.4 | 42329 | 54003 |

### filesystem-write / fdatasync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7554.4 | 129611.9 | 127009 | 177531 |
| java | 5048.1 | 195125.4 | 189704 | 356545 |
| rust | 7237.1 | 135111.3 | 132029 | 184657 |

### filesystem-write / fsync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7648.9 | 128121.7 | 125122 | 167564 |
| java | 7936.0 | 123249.5 | 123408 | 168501 |
| rust | 7103.6 | 137605.0 | 134387 | 188902 |

### filesystem-write / prealloc

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 25075.6 | 37052.6 | 36993 | 47747 |
| java | 23556.9 | 39525.6 | 37047 | 89616 |
| rust | 25523.6 | 36709.0 | 36449 | 46939 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
