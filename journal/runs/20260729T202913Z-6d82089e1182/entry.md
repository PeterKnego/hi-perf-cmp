# 20260729T202913Z-6d82089e1182

- commit: 6d82089e118231c5664186d5ecf9a59c4b5da7ca dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
issue #19 A/B variant B (after): ultima_db 2907f56 open_table handle caching; same fleet, batch cells -7 to -13% mean / -17 to -24% p99

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / live_ultima

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| rust | 10 | 2751256 | 2066394.9 | 2052949 | 2231470 | 197520 | 7031.3 | 6875 | 8531 |

### smr-collections / ultima_batch_insert

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 139571.2 | 138119 | 160448 | 64 | 2180.8 |

### smr-collections / ultima_batch_update

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 179430.1 | 177517 | 200142 | 64 | 2803.6 |

### smr-collections / ultima_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| rust | 8764.5 | 8598 | 17062 |

### smr-collections / ultima_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| rust | 6992.4 | 6878 | 8269 |

## Hypothesis
ultima_db issue #19 (`WriteTx::open_table` caches the per-table metrics handle
and name in the dirty map instead of re-deriving them per call) should speed up
transactions that open the same tables repeatedly — the batched apply cells,
which open 3 tables per command across a 64-command txn — while leaving
single-command-per-txn cells roughly flat (little to amortize).

## Observations
Same-fleet A/B: this run (variant B, ultima_db rev 2907f56 = #19) vs run
20260729T202653Z (variant A, rev 8ac858d, identical hi-perf-cmp tree apart from
the ultima_db pin), both on the same c6id.2xlarge host minutes apart.

| cell | metric | A (8ac858d) | B (#19) | delta |
|---|---|---|---|---|
| ultima_batch_update | per_op_mean | 3226 ns | 2804 ns | **-13.1%** |
| ultima_batch_update | batch_p99   | 241853 ns | 200142 ns | **-17.2%** |
| ultima_batch_insert | per_op_mean | 2352 ns | 2181 ns | **-7.3%** |
| ultima_batch_insert | batch_p99   | 209941 ns | 160448 ns | **-23.6%** |
| ultima_insert  | insert_mean | 8467 ns | 8765 ns | +3.5% (1 cmd/txn, within noise) |
| ultima_update  | update_mean | 6959 ns | 6992 ns | +0.5% (1 cmd/txn, within noise) |
| live_ultima    | writer_mean | 7077 ns | 7031 ns | -0.6% (1 cmd/txn, within noise) |

The two batched cells — #19's target — improve cleanly on both mean and, more
strongly, the tail (p99 -17 to -24%), consistent with dropping an allocation
and an RwLock acquisition from the per-open path. The three single-command
cells moved within the run-to-run noise floor (these are n=1 cells; the batched
effect is well outside it, the single-command deltas inside it), matching the
prediction that a one-command txn has almost nothing to amortize.

Note: this A/B is trustworthy as a *same-fleet delta*. Do not compare the
absolute numbers here against the earlier 20260727T164805Z batch figures
(2.31/2.67 us) — that was a different instance, and the batch_update cell alone
shows ~21% cross-instance variance (2.67 -> 3.23 us on identical 8ac858d code).
