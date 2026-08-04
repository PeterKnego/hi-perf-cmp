# 20260804T222844Z-c80faa7ab8ea

- commit: c80faa7ab8eac072dbfdb450b5845b84a30078fd clean
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
smr-collections full re-measure: first journaled run carrying the CowBook Arc::get_mut removal (and the Go flyweight-codec fix), superseding the stale mvcc_* and Go snapshot figures

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### smr-collections / churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 98.8 | 91 | 179 | 97.9 | 92 | 177 | 68.5 | 63 | 145 | 417792 |
| java | 174.9 | 151 | 520 | 258.0 | 166 | 608 | 181.6 | 172 | 537 | 507904 |
| rust | 104.7 | 91 | 243 | 107.7 | 99 | 215 | 49.9 | 33 | 244 | 2465792 |

### smr-collections / insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 111.2 | 68 | 1181 |
| java | 158.6 | 144 | 488 |
| rust | 47.7 | 41 | 86 |

### smr-collections / live_mvcc

| language | snap_count (count) | snap_skipped (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 10 | — | 2751260 | 1429262.1 | 1105100 | 2028565 | 264759 | 176.5 | 108 | 332 |
| java | 9 | 1 | 2751260 | 3206169.3 | 3203874 | 4394096 | 2910972 | 202.9 | 116 | 532 |
| rust | 10 | — | 2751260 | 719894.7 | 710092 | 747608 | 137055 | 135.1 | 99 | 242 |

### smr-collections / live_mvcc_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_peak_bytes (bytes) | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| go | 202.2 | 118 | 399 | 144.3 | 124 | 450 | 128.3 | 74 | 253 | 44220416 | 10 | 2751305 | 1142299.5 | 1114932 | 1239931 | 419924 | 165.0 | 106 | 345 |
| java | 315.0 | 233 | 712 | 327.1 | 249 | 2615 | 185.1 | 134 | 489 | 140509184 | 10 | 2751305 | 3163886.7 | 3194847 | 3338887 | 259449 | 250.1 | 183 | 650 |
| rust | 168.6 | 118 | 351 | 142.8 | 129 | 303 | 81.8 | 50 | 147 | 28864512 | 10 | 2751305 | 728237.5 | 711440 | 775581 | 226366 | 125.1 | 77 | 316 |

### smr-collections / live_stw

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| go | 10 | 2751260 | 966438.8 | 959433 | 973736 | 1047700 | 143.4 | 92 | 165 |
| java | 10 | 2751260 | 1941750.2 | 1133218 | 3074826 | 3937942 | 228.9 | 110 | 429 |
| rust | 10 | 2751260 | 570029.8 | 562878 | 568808 | 652705 | 117.2 | 89 | 162 |

### smr-collections / live_stw_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_peak_bytes (bytes) | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| go | 99.9 | 95 | 167 | 102.5 | 98 | 173 | 176.4 | 67 | 157 | 18214912 | 10 | 2751305 | 1002680.8 | 988010 | 1013446 | 1103019 | 138.2 | 83 | 163 |
| java | 166.9 | 140 | 501 | 209.2 | 148 | 596 | 371.0 | 175 | 551 | 147066880 | 10 | 2751305 | 1833780.5 | 1168418 | 3165853 | 3986852 | 269.2 | 159 | 528 |
| rust | 108.8 | 95 | 234 | 118.7 | 107 | 232 | 107.8 | 33 | 77 | 25538560 | 10 | 2751305 | 625034.8 | 616473 | 630325 | 703202 | 108.4 | 56 | 215 |

### smr-collections / live_ultima

| language | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|
| rust | 10 | 2751260 | 1767884.8 | 1793452 | 1859396 | 153484 | 6820.3 | 6691 | 8110 |

### smr-collections / live_ultima_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_peak_bytes (bytes) | snap_count (count) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | writer_max (ns) | writer_mean (ns) | writer_p50 (ns) | writer_p99 (ns) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 10603.9 | 10436 | 22702 | 10535.4 | 10428 | 13064 | 9720.5 | 9524 | 21225 | 21860352 | 10 | 2751260 | 1855831.6 | 1878546 | 1931585 | 187996 | 10161.9 | 9943 | 21963 |

### smr-collections / mvcc_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|---|---|
| go | 118.8 | 109 | 206 | 124.9 | 115 | 242 | 81.6 | 72 | 163 | 815104 |
| java | 173.6 | 154 | 507 | 266.5 | 184 | 638 | 111.8 | 98 | 274 | 733184 |
| rust | 118.4 | 107 | 242 | 125.3 | 115 | 228 | 60.7 | 45 | 147 | 2465792 |

### smr-collections / mvcc_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| go | 90.9 | 68 | 1106 |
| java | 227.4 | 108 | 239 |
| rust | 59.0 | 51 | 112 |

### smr-collections / mvcc_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 3862557.2 | 3521405 | 8697699 | 2751260 | 1035418.3 | 1026334 | 1130139 | 2657148271.8 |
| java | 5355541.0 | 5328218 | 6103185 | 2751260 | 705314.9 | 701664 | 772569 | 3900754114.2 |
| rust | 4736001.1 | 4684661 | 5309794 | 2751260 | 655890.3 | 648427 | 751699 | 4194695564.4 |

### smr-collections / mvcc_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 113.4 | 107 | 190 |
| java | 126.0 | 118 | 245 |
| rust | 96.0 | 93 | 175 |

### smr-collections / snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| go | 3681614.8 | 3438455 | 5997884 | 2751260 | 977889.9 | 973099 | 1071781 | 2813466028.5 |
| java | 11030936.0 | 6754413 | 53883243 | 2751260 | 795743.7 | 760263 | 1277747 | 3457470093.7 |
| rust | 1701007.3 | 1729792 | 2118463 | 2751260 | 684333.7 | 685338 | 797063 | 4020348749.4 |

### smr-collections / ultima_batch_churn

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|
| rust | 298858.9 | 296846 | 322726 | 64 | 4669.7 | 1667072 |

### smr-collections / ultima_batch_insert

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 151686.8 | 150757 | 178150 | 64 | 2370.1 |

### smr-collections / ultima_batch_update

| language | batch_mean (ns) | batch_p50 (ns) | batch_p99 (ns) | batch_size (count) | per_op_mean (ns) |
|---|---|---|---|---|---|
| rust | 182942.4 | 180761 | 201212 | 64 | 2858.5 |

### smr-collections / ultima_churn

| language | cancel_mean (ns) | cancel_p50 (ns) | cancel_p99 (ns) | fill_mean (ns) | fill_p50 (ns) | fill_p99 (ns) | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) | rss_growth_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 10541.3 | 10393 | 22038 | 10508.3 | 10414 | 12589 | 9662.1 | 9483 | 20589 | 2465792 |

### smr-collections / ultima_insert

| language | insert_mean (ns) | insert_p50 (ns) | insert_p99 (ns) |
|---|---|---|---|
| rust | 8754.0 | 8614 | 16311 |

### smr-collections / ultima_snapshot

| language | restore_mean (ns) | restore_p50 (ns) | restore_p99 (ns) | snapshot_bytes (bytes) | snapshot_mean (ns) | snapshot_p50 (ns) | snapshot_p99 (ns) | snapshot_throughput (bytes_per_sec) |
|---|---|---|---|---|---|---|---|---|
| rust | 2663748.6 | 2656659 | 2848197 | 2751260 | 1334130.8 | 1277314 | 2213845 | 2062211665.1 |

### smr-collections / ultima_update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| rust | 6799.5 | 6690 | 7701 |

### smr-collections / update

| language | update_mean (ns) | update_p50 (ns) | update_p99 (ns) |
|---|---|---|---|
| go | 108.4 | 98 | 220 |
| java | 138.5 | 122 | 356 |
| rust | 89.6 | 84 | 185 |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
