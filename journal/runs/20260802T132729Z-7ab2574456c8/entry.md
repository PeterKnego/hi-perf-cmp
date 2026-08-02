# 20260802T132729Z-7ab2574456c8

- commit: 7ab2574456c875b9992c6aa93763fca119a77d58 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
First cancel-heavy churn run: cancel op + ~1% OTR workload measured across all three languages (15 churn cells), plus the schema-v2 snapshot_bytes shift on every pre-existing smr-collections cell

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### filesystem-write / batch

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 356090.7 | 44514.7 | 44296 | 55998 |
| java | 326377.8 | 44876.8 | 44116 | 58678 |
| rust | 383933.2 | 43713.9 | 43789 | 54075 |

### filesystem-write / fdatasync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7433.0 | 131977.5 | 127474 | 167940 |
| java | 7775.7 | 125964.7 | 124479 | 179761 |
| rust | 7624.1 | 128937.0 | 126292 | 167764 |

### filesystem-write / fsync

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 7581.4 | 129354.3 | 127239 | 164393 |
| java | 7636.3 | 128171.2 | 125678 | 181681 |
| rust | 7611.2 | 129186.3 | 126120 | 167030 |

### filesystem-write / prealloc

| language | durable_append_throughput (ops_per_sec) | sync_mean (ns) | sync_p50 (ns) | sync_p99 (ns) |
|---|---|---|---|---|
| go | 24682.4 | 37761.6 | 37591 | 47558 |
| java | 24314.4 | 38191.0 | 37467 | 52523 |
| rust | 25205.4 | 37201.1 | 37248 | 46378 |

### network-rtt / quic

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 93066.9 | 88144 | 136538 |
| java | 161208.2 | 158776 | 198192 |
| rust | 77793.5 | 70447 | 125213 |

### network-rtt / tcp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 36764.8 | 36227 | 47393 |
| java | 39596.6 | 39239 | 49129 |
| rust | 38076.4 | 37652 | 47425 |

### network-rtt / udp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 36091.2 | 35638 | 45852 |
| java | 34868.8 | 34509 | 43232 |
| rust | 33683.2 | 33322 | 41264 |

### rpc-roundtrip / bebop_tcp

| language | encoded_bytes (bytes) | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|---|
| go | 252 | 45718.0 | 44594 | 81488 |

### rpc-roundtrip / grpc

| language | encoded_bytes (bytes) | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|---|
| go | 247 | 137863.8 | 137595 | 190855 |

### rpc-roundtrip / sbe_udp

| language | encoded_bytes (bytes) | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|---|
| rust | 252 | 34319.3 | 33774 | 45129 |

### serialization / aeron_sbe

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| go | 246.1 | 237 | 334 | 135.4 | 126 | 245 | 306 |
| rust | 126.2 | 120 | 186 | 68.1 | 57 | 262 | 306 |

### serialization / bebop

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 352 | 468.1 | 404 | 1402 | 134.2 | 111 | 369 | 298 |

### serialization / bincode

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| rust | 336 | 363.8 | 360 | 430 | 69.8 | 60 | 263 | 290 |

### serialization / flatbuffers

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| go | 472.7 | 458 | 685 | 832.3 | 816 | 963 | 472 |

### serialization / protobuf

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 696 | 1386.2 | 1189 | 5021 | 674.8 | 654 | 887 | 326 |

### serialization / sbe_gen

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 124.3 | 120 | 182 | 54.4 | 42 | 266 | 306 |

### serialization / sbe_struct

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 384 | 1115.3 | 946 | 4890 | 428.0 | 412 | 559 | 306 |

### smr-collections / churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 136.0 | 118 | 461 | 136.1 | 124 | 406 | 92.3 | 70 | 293 | 450560 |
| java | 239.7 | 215 | 625 | 344.6 | 231 | 693 | 247.4 | 232 | 635 | — |
| rust | 125.4 | 95 | 410 | 127.4 | 103 | 345 | 56.5 | 33 | 400 | 2465792 |

### smr-collections / insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 94.2 | 64 | 1181 |
| java | 156.2 | 140 | 487 |
| rust | 47.7 | 40 | 86 |

### smr-collections / live_mvcc

| language | snap_count (count) | snap_skipped (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 7 | 3 | 2751260 | 5213589.9 | 5138903 | 5356810 | 243739 | 201.7 | 165 | 533 |
| java | 10 | — | 2751260 | 3440492.7 | 3214616 | 4219217 | 2882121 | 204.5 | 117 | 512 |
| rust | 10 | — | 2751260 | 722297.7 | 712019 | 749038 | 124146 | 137.7 | 102 | 249 |

### smr-collections / live_mvcc_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_peak_bytes (bytes) | snap_count (count) | snap_skipped (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| go | 251.8 | 182 | 570 | 210.4 | 189 | 534 | 167.9 | 139 | 462 | 45907968 | 10 | — | 2751305 | 5198384.9 | 5145809 | 5380679 | 305156 | 209.7 | 149 | 533 |
| java | 242.3 | 179 | 661 | 404.8 | 177 | 2986 | 137.9 | 93 | 450 | 120250368 | 8 | 2 | 2751305 | 4326523.9 | 3384916 | 5902807 | 239297 | 190.9 | 135 | 585 |
| rust | 228.2 | 170 | 482 | 203.0 | 182 | 460 | 95.1 | 59 | 155 | 28409856 | 10 | — | 2751305 | 668884.6 | 655464 | 713151 | 204025 | 161.5 | 104 | 457 |

### smr-collections / live_stw

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| go | 10 | 2751260 | 4981828.1 | 4943199 | 5011603 | 5219921 | 402.9 | 151 | 500 |
| java | 10 | 2751260 | 1901844.3 | 1122820 | 3061991 | 3904033 | 233.1 | 119 | 454 |
| rust | 10 | 2751260 | 559966 | 561612 | 568354 | 574222 | 116.9 | 89 | 157 |

### smr-collections / live_stw_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_peak_bytes (bytes) | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| go | 188.6 | 164 | 543 | 200.8 | 165 | 545 | 641.5 | 133 | 454 | 37892096 | 10 | 2751305 | 5049347.2 | 5000539 | 5165653 | 5369802 | 415.1 | 143 | 512 |
| java | 176.9 | 148 | 527 | 212.7 | 153 | 587 | 392.3 | 180 | 565 | 144031744 | 10 | 2751305 | 1966690 | 648180 | 2988673 | 6043565 | 284.8 | 167 | 547 |
| rust | 106.7 | 91 | 243 | 117.7 | 102 | 250 | 109.3 | 33 | 89 | 25522176 | 10 | 2751305 | 635028 | 618804 | 677264 | 683619 | 108.0 | 56 | 228 |

### smr-collections / live_ultima

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| rust | 10 | 2751260 | 1467330.8 | 1430151 | 1640801 | 156895 | 6692.4 | 6586 | 7807 |

### smr-collections / live_ultima_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_peak_bytes (bytes) | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 10364.4 | 10209 | 22307 | 10421.1 | 10224 | 22810 | 9662.4 | 9477 | 21454 | 21848064 | 10 | 2751260 | 1698230.5 | 1703611 | 1804258 | 199218 | 10013.7 | 9819 | 21847 |

### smr-collections / mvcc_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 133.1 | 113 | 383 | 143.8 | 124 | 489 | 96.7 | 76 | 298 | 811008 |
| java | 164.0 | 142 | 489 | 254.2 | 165 | 613 | 108.6 | 95 | 245 | 24576 |
| rust | 257.5 | 237 | 668 | 253.2 | 237 | 556 | 70.5 | 58 | 145 | 2465792 |

### smr-collections / mvcc_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 105.5 | 71 | 1197 |
| java | 231.6 | 112 | 256 |
| rust | 77.7 | 67 | 181 |

### smr-collections / mvcc_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 9633464.6 | 9521144 | 12147982 | 2751260 | 6211556.4 | 6189826 | 6573879 | 442926027.7 |
| java | 5275530.0 | 5252417 | 6008845 | 2751260 | 708660.7 | 707642 | 782197 | 3882337512.0 |
| rust | 4945264.1 | 4865066 | 5703729 | 2751260 | 703537.9 | 691186 | 837404 | 3910606442.3 |

### smr-collections / mvcc_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 118.0 | 105 | 235 |
| java | 149.4 | 126 | 435 |
| rust | 98.6 | 93 | 192 |

### smr-collections / snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 9567426.1 | 9323977 | 11794389 | 2751260 | 5058116.0 | 5038227 | 5322092 | 543929798.3 |
| java | 10816549.6 | 6393244 | 53501373 | 2751260 | 797710.7 | 769271 | 1381617 | 3448944554.6 |
| rust | 1369262.8 | 1350141 | 1629920 | 2751260 | 613310.7 | 610202 | 671366 | 4485915523.7 |

### smr-collections / ultima_batch_churn

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|
| rust | 285203.5 | 283007 | 311596 | 64 | 4456.3 | 1667072 |

### smr-collections / ultima_batch_insert

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 150773.9 | 149637 | 180332 | 64 | 2355.8 |

### smr-collections / ultima_batch_update

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 199083.6 | 197507 | 228768 | 64 | 3110.7 |

### smr-collections / ultima_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 10311.8 | 10135 | 22280 | 10348.8 | 10175 | 21358 | 9607.5 | 9386 | 21541 | 2465792 |

### smr-collections / ultima_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| rust | 8767.8 | 8626 | 17764 |

### smr-collections / ultima_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| rust | 2651423.3 | 2636335 | 2953487 | 2751260 | 1373051.7 | 1239436 | 2498157 | 2003755625.0 |

### smr-collections / ultima_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| rust | 6724.8 | 6629 | 7461 |

### smr-collections / update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 105.8 | 97 | 193 |
| java | 129.6 | 119 | 275 |
| rust | 91.1 | 85 | 205 |

### thread-handoff / channel

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 348.9 | 300 | 594 |
| java | 6768.2 | 437 | 24301 |
| rust | 420.7 | 380 | 1106 |

### thread-handoff / condvar

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 432.2 | 375 | 780 |
| java | 313.2 | 280 | 586 |
| rust | 333.1 | 305 | 420 |

### thread-handoff / ring

| language | handoff_throughput (ops_per_sec) |
|---|---|
| go | 28816425.8 |
| java | 7155438.4 |
| rust | 370005846.1 |

### thread-handoff / spin

| language | handoff_rtt_mean (ns) | handoff_rtt_p50 (ns) | handoff_rtt_p99 (ns) |
|---|---|---|---|
| go | 199.8 | 196 | 205 |
| java | 267.3 | 266 | 312 |
| rust | 215.2 | 210 | 254 |

## Hypothesis

The `smr-collections` grid had only ever run insert/update workloads, in which
nothing is removed — the friendliest possible case for an MVCC engine, since a
delete there frees nothing but writes a new version. Real order flow is
cancel-dominated (~1 % of orders end in a trade). The expectation going in:

1. Cancel costs more than insert on the flat store — an unlink plus an id-map
   removal plus an occasional O(levels) best-price rescan, versus a bump
   allocation.
2. ultima_db's position gets **worse** under cancellation than the ~30-50x the
   insert/update-only workload suggested, because deletes generate dead
   versions that reclamation must chase. How much worse was the open question
   this whole extension exists to answer.
3. Chunked CoW's snapshot-stall advantage should hold, and possibly widen,
   because cancels scatter writes across chunks rather than appending to the
   newest one.

## Observations

First measurement of the cancel-heavy workload. 89/89 cells, 0 failures;
15 churn cells (7 rust, 4 go, 4 java). All within-grid comparisons below are
**same-run, same fleet** unless stated.

**Cancel costs about 2x an insert on the flat store.** Rust 125 ns cancel vs
56 ns insert in the same `churn` cell (`cancel_p99` 410 ns — the ladder rescan
surfacing in the tail exactly where it was predicted to). Go 136/92, Java
240/247. The rescan is deliberately on the timed path; hiding it would have
hidden the worst-case cancel.

**The engine trade gets worse under cancellation, but not catastrophically.**
Same-run batched figures: `ultima_batch_insert` 2,356 ns/op,
`ultima_batch_update` 3,111 ns/op, `ultima_batch_churn` **4,456 ns/op** — so a
cancel-heavy stream costs **1.4-1.9x** what the insert/update-only streams cost
on the same engine and the same host. Unbatched: `ultima_insert` 8,768 ns vs
`ultima_churn` 10,312 ns cancel, +18 %. Against the flat store's 125 ns cancel
that is ~36x batched / ~82x unbatched. The pre-churn RESULTS figure was ~30-50x
batched, so cancellation moves the engine trade by roughly a third — real, and
far short of the order-of-magnitude collapse that was possible.

**Chunked CoW pays for cancel in Rust but not in Go.** Rust `mvcc_churn` cancel
258 ns vs flat 125 ns (2.1x); Go 133 vs 136 (parity); Java 164 vs 240 (CoW
cheaper). The Rust CoW store's cancel touches the order chunk, the level chunk
and then rescans through chunk indirection; why Go's equivalent is free is not
explained by this run and is worth a look.

**Snapshot-under-churn: CoW's advantage widens sharply, and Java sees it.**
`writer_max`, same run:

| cell | rust | go | java |
|---|---|---|---|
| live_stw (no churn) | 574 us | 5.22 ms | 3.90 ms |
| live_stw_churn | 684 us | 5.37 ms | **6.04 ms** |
| live_mvcc (no churn) | 124 us | 244 us | **2.88 ms** |
| live_mvcc_churn | 204 us | 305 us | **239 us** |

Java's STW->CoW improvement is 1.35x without churn and **25x with it**. That
inverts the previous run's conclusion that Java's CoW gains are eaten by GC.
**Treat as suggestive, not established:** `writer_max` is a single-event
maximum over 10 triggers, so whether a GC pause lands in a given cell is close
to a coin flip. It wants a repeat run before it goes in RESULTS.md as a finding.

**The schema-v2 shift landed exactly as predicted.** `snapshot_bytes`
2,751,256 -> 2,751,260 (+4) on every cell that reported it in both this run and
20260727T164805Z. Churn cells read 2,751,305 — larger because a churned pool
carries freed slots.

**8 cells flagged REGRESSION vs 20260729T215021Z, none confirmed.**
`ultima_batch_insert` +20.5 %, `ultima_batch_update` +14.3 %. Three reasons not
to record these as regressions: (a) that run was a different instance draw and
RESULTS.md documents ~21 % cross-instance variance on `batch_update` alone;
(b) both cells changed composition this cycle — the adapter gained an
`order_id -> row id` map and `OrderRec` grew 4 bytes; (c) the deltas are the
same sign and rough magnitude as that documented band. `journal/REGRESSIONS.md`
stays empty.

**Caveats that must travel with these numbers:**

- **Java's `fill_mean` is 1.44x its own `cancel_mean`** (345 vs 240 ns) where
  Rust and Go sit at ~1.0x. The `fill_p50` figures are in line (231 vs 215), so
  the JVM pre-run added this cycle did warm the p50 — the residual is in the
  tail. Java's `fill_*` is not yet comparable with the other two.
- **Per-op splits inside the `live_*` cells are polluted by the trigger op.**
  `live_stw_churn` Go reports `insert_mean` 642 ns against `insert_p50` 133 ns:
  the ops that trigger a snapshot absorb the whole serialize. `writer_max`
  remains the headline metric for those cells; read the split for *which* op
  absorbed the stall, not as a latency.
- **`live_mvcc_churn` Java skipped 2 of 10 triggers**, and non-churn
  `live_mvcc` Go skipped 3. Snapshot cadence remains a per-language tunable on
  a knife edge, as previously recorded.
- **Java RSS figures are a JVM artifact.** `rss_growth_bytes` reads 0 / 24 KB
  for Java against Rust's 2.4 MB, and `rss_peak_bytes` 120-144 MB against
  Rust's 21-28 MB — heap, metaspace and code cache, not store growth. Java's
  RSS is a Java-vs-Java trend only.
- Churn per-op means are ~5-8 % faster than the older `insert`/`update` cells
  for an instrumentation reason: the churn cells exclude op generation from the
  timed region, those cells include it. Adjacent rows, systematic bias.
- The commit is recorded `dirty` — two untracked files unrelated to this work
  (`smr-collections.txt`, a network-payload spec) were present in the tree. No
  tracked file was modified.

**Deliberately still unmeasured:** version GC under *delete-heavy* churn with a
growing live set. This workload holds the live set exactly constant by
construction, so reclamation is measured at steady state only.

