# filesystem-write Benchmark Design

**Date:** 2026-06-26
**Status:** Proposed — awaiting review

## Purpose

Implement the **filesystem-write** focus area in Rust, Java, and Go: measure the
cost of **durably appending command-log entries** — the Raft/Paxos hot path where
a log entry must be on stable storage *before* it is acknowledged. Replaces the
current placeholder stub in each language. Emits results in the shared
[result contract](../result-contract.md).

This is a **single-host** focus area (`kind: local`): it runs on node0 against
local NVMe. There is no cross-host component.

The design is informed by the production WAL in the sibling `ultima_journal`
crate, whose durable-append path this benchmark distills:

- its per-commit durability barrier is **`fdatasync` (`sync_data`)**, *not* full
  `fsync` — chosen deliberately to drop the per-commit inode-timestamp write and
  lower the submitted→persisted P99 tail. Full `fsync`/`sync_all` is reserved for
  *segment creation*, where a new directory entry must be made durable.
- its writer performs **group commit**: it drains queued appends into one
  coalesced `write_all`, then issues **one `sync_data` barrier covering the whole
  batch**.

The three experiments below measure exactly those choices.

## Experiment grid

Three runnable artifacts per language, named `filesystem-write-<experiment>`
(mirroring `network-rtt-<experiment>`):

| experiment  | per timed unit                                            | models                                            |
|-------------|-----------------------------------------------------------|---------------------------------------------------|
| `fsync`     | append one entry, then **full `fsync`** (`sync_all`)      | strict durability *including* file metadata       |
| `fdatasync` | append one entry, then **`fdatasync`** (`sync_data`)      | the WAL commit primitive `ultima_journal` uses    |
| `batch`     | append `FSW_BATCH` entries (one coalesced write), then **one `fdatasync`** | group commit — the production path |

`fsync` vs `fdatasync` isolates the cost of the metadata/timestamp commit;
`fdatasync` vs `batch` isolates the win from amortizing one barrier over many
entries.

## Methodology (identical across all three languages)

For each experiment artifact:

1. **Setup (outside timing).** Resolve `FSW_DIR` (required — see Configuration).
   Create/truncate a fresh file `filesystem-write-<experiment>.log` in it
   (`O_CREAT | O_WRONLY | O_TRUNC`). `fsync` the file *and its parent directory*
   once so the file's existence is durable before the loop. This one-time cost
   never enters the timed path.
2. **Entry.** An opaque `FSW_ENTRY_BYTES`-byte buffer, allocated once and
   reused. **No CRC, no seq/meta framing** — the benchmark measures the I/O +
   durability-syscall path, not record encoding. (Framing + CRC would inject
   per-language CRC-implementation differences — Rust `crc32fast` SIMD vs Java
   `CRC32` vs Go `hash/crc32` — into a *filesystem-write* comparison. See
   Out of scope.)
3. **Warmup.** Run `FSW_WARMUP` discarded durable-append operations (lets the
   filesystem journal, device write cache, and — for Java — the JIT settle).
4. **Measure.** A single monotonic span wraps the measured loop:
   - record `t_start` (monotonic clock),
   - run `FSW_ITERATIONS` entries' worth of durable-append operations; for each
     **sync call**, record its elapsed nanoseconds into a pre-allocated sample
     buffer,
   - record `t_end`.
5. Compute statistics and emit the result lines (below).

One durable-append operation is outstanding at a time (no concurrency, no
pipelining). For `fsync`/`fdatasync`, one operation = one entry = one sync, so
there are `FSW_ITERATIONS` sync samples. For `batch`, the `FSW_ITERATIONS`
entries are written in chunks of `FSW_BATCH` (a trailing short chunk is allowed),
each chunk followed by one sync — so there are `ceil(FSW_ITERATIONS / FSW_BATCH)`
sync samples, and exactly `FSW_ITERATIONS` entries are persisted. All sample
buffers are pre-allocated before timing so allocation never enters the timed
path.

### What each metric measures

- **`sync_*` latency** isolates the **durability barrier alone** — only the
  `fsync`/`fdatasync` call is timed, mirroring how `ultima_journal`'s microbench
  isolates the fsync barrier from the page-cache write. This is the dominant and
  most interesting cost.
- **`durable_append_throughput`** is the **end-to-end durable rate**:
  `FSW_ITERATIONS / (t_end − t_start)` entries per second, covering the whole
  append+sync loop. For `batch`, this is where group commit visibly wins.

### Statistics (identical formula — required for comparability)

Reuses the existing shared `Stats` in each language (the same code that backs
`network-rtt`). Given `n` sync samples of elapsed nanoseconds, sorted ascending:

- **percentile(p)** = `sorted[ floor( p/100 * (n − 1) ) ]` — nearest-rank, no
  interpolation.
- **mean** = `sum / n`.

`sync_p50`/`sync_p99` are emitted as integer nanoseconds; `sync_mean` as a
(possibly fractional) number of nanoseconds. Throughput is a fractional
`ops_per_sec`.

### Configuration (env-var overrides, `FSW_` prefix)

| env var            | default      | meaning                                              |
|--------------------|--------------|------------------------------------------------------|
| `FSW_DIR`          | **required** | directory for the bench file; **no default**         |
| `FSW_ENTRY_BYTES`  | `256`        | entry size in bytes                                  |
| `FSW_WARMUP`       | `5000`       | discarded warmup operations                          |
| `FSW_ITERATIONS`   | `50000`      | measured entries                                     |
| `FSW_BATCH`        | `32`         | entries per group-commit (the `batch` experiment only) |

`FSW_DIR` is **required with no default**: an unset/temp default risks landing on
a `tmpfs` (memory-backed) filesystem, where "durable" writes never touch a device
and the numbers are meaningless. Unset `FSW_DIR` → descriptive message on stderr +
non-zero exit. The directory must be a real disk; the bench fleet points it at the
NVMe-backed bench home. `FSW_BATCH` is parsed by all three artifacts (uniform
config type) but only consumed by `batch`.

Invalid/non-positive numeric values → message on stderr + non-zero exit, exactly
like the `RTT_*` config. The harness sets the same env vars for all three
languages so the comparison is apples-to-apples.

## Output

Four result-contract lines per experiment (`focus_area: "filesystem-write"`):

| metric                       | unit          | meaning                                  | `samples`            |
|------------------------------|---------------|------------------------------------------|----------------------|
| `durable_append_throughput`  | `ops_per_sec` | entries persisted per second (end-to-end)| `FSW_ITERATIONS`     |
| `sync_p50`                   | `ns`          | median durability-barrier latency        | number of sync calls |
| `sync_p99`                   | `ns`          | 99th-percentile barrier latency          | number of sync calls |
| `sync_mean`                  | `ns`          | mean barrier latency                     | number of sync calls |

Twelve lines per language per run (4 × 3 experiments). The grid aligns on
`(focus_area, experiment, language, metric)` exactly like `network-rtt`.

Example:
```json
{"language":"rust","focus_area":"filesystem-write","experiment":"fdatasync","metric":"sync_p50","value":48000,"unit":"ns","samples":50000}
{"language":"rust","focus_area":"filesystem-write","experiment":"batch","metric":"durable_append_throughput","value":740000.0,"unit":"ops_per_sec","samples":50000}
```

## Error handling

- Missing `FSW_DIR`, invalid config, or any file create / write / sync / IO
  failure → descriptive message to **stderr**, non-zero exit. stdout stays
  results-only (contract requirement).
- A sync failure mid-loop is a **hard error** (stderr + exit), not retried —
  retrying would distort the latency distribution and a failed barrier means the
  durability guarantee is already broken.
- Best-effort: remove the bench file on success. (Re-runs truncate on open, so
  leftover files are harmless; cleanup just avoids leaving NVMe litter.)

## Cross-language sync primitives (all std, no new dependencies)

| operation   | Rust                 | Go                                       | Java                          |
|-------------|----------------------|------------------------------------------|-------------------------------|
| `fsync`     | `File::sync_all()`   | `os.File.Sync()`                         | `FileChannel.force(true)`     |
| `fdatasync` | `File::sync_data()`  | `syscall.Fdatasync(int(f.Fd()))` (Linux) | `FileChannel.force(false)`    |

`syscall.Fdatasync` is in Go's standard `syscall` package on Linux, so Go stays
dependency-free. All three `fdatasync` primitives flush data + the size growth
needed to read it back while skipping inode timestamps — the same guarantee
`ultima_journal` relies on.

## Per-language structure

The three experiments differ only in (a) the sync primitive and (b) the batch
size. To avoid triplicating the harness, the **shared bench library owns the
durable-append harness**; each artifact is a thin `main` that names its
experiment and supplies those two parameters. This mirrors `network-rtt`, where
`bench-common` owns the timed loop and each transport crate supplies its
operation.

A small sync-kind selector keeps the primitive choice declarative:
`Full` → `fsync`, `Data` → `fdatasync`.

**Rust** — replace the single `filesystem-write` workspace member with three:
`filesystem-write/{fsync,fdatasync,batch}` (binaries `filesystem-write-fsync`,
`-fdatasync`, `-batch`). Add a `fswrite` module to **`bench-common`**:
- `FsConfig::from_env()` — parse `FSW_*`, validate, require `FSW_DIR`.
- `SyncKind { Full, Data }`.
- `run_durable_append(cfg, sync_kind, batch_size) -> io::Result<(Vec<u64>, f64)>`
  — file setup, warmup, timed loop; returns sync-latency samples + throughput.
- `emit_fs(experiment, samples, throughput)` — emit the four lines.

Each `main.rs` is ~3 lines: build config, call the harness with its
`(SyncKind, batch_size)`, emit. Std-only.

**Go** — replace `cmd/filesystem-write` with `cmd/filesystem-write-{fsync,fdatasync,batch}`
(all `package main`). Add to **`internal/bench`**: `FsConfig`, `SyncKind`,
`RunDurableAppend(...)`, `EmitFS(...)`. Each `main.go` is thin. Uses std
`syscall.Fdatasync` on Linux.

**Java** — replace the `:filesystem-write` subproject with
`:filesystem-write-fsync`, `:filesystem-write-fdatasync`, `:filesystem-write-batch`
(register all three in `settings.gradle.kts`; each a one-line `build.gradle.kts`
applying `application` + depending on `:common`, `mainClass` =
`net.knego.hiperf.filesystemwrite.<exp>.Main`). Add to **`:common`**
(`net.knego.hiperf.common`): `FsConfig`, `SyncKind`, a `DurableAppend` harness,
and an FS emit path. Each `Main` is thin.

### Shared-library change: parameterize the result unit

`bench-common`'s `result::emit` (Rust) currently hardcodes `"unit":"ns"`. Add a
`unit: &str` parameter to the integer and float emitters and update the
`network-rtt` call sites in `measure.rs` to pass `"ns"`. Go (`bench.Result.Unit`)
and Java (`Result` ctor) already carry an explicit unit, so only Rust needs this.
Stats is reused unchanged in all three.

## bench-infra integration

In `bench-infra/ansible/group_vars/all.yml`:

- Replace the single
  `{ focus_area: filesystem-write, experiment: placeholder, kind: local }`
  row with three `kind: local` rows: `fsync`, `fdatasync`, `batch`.
- Add `fsw_*` params (mirroring the `rtt_*` block) exported into the local runs
  so all three languages use identical parameters:
  `fsw_dir` (a subdir of the NVMe bench home, e.g. `{{ remote_home }}/fsw`),
  `fsw_entry_bytes`, `fsw_warmup`, `fsw_iterations`, `fsw_batch`.
- Ensure the run step creates `fsw_dir` and exports `FSW_*` for the
  filesystem-write artifacts (the existing per-experiment env-export mechanism
  used for `RTT_*`).

## Testing

- **`Stats` stays unit-tested** in each language (unchanged; already covers the
  comparability-critical percentile/mean logic shared with `network-rtt`).
- Each artifact is verified by **running** it (against a real-disk `FSW_DIR`)
  and confirming four well-formed contract lines per experiment with plausible
  values: `sync_p99 ≥ sync_p50`, throughput > 0, and `batch` throughput
  materially above `fdatasync` throughput (group commit must win).
- Durability correctness is implicit: each operation only records its sample
  after the sync call returns success; a sync error aborts.
- A short smoke run (small `FSW_ITERATIONS`) keeps the verification fast.

## Out of scope (YAGNI / future experiments)

- **Preallocated-`fdatasync`** variant (etcd-style: zero-fill + sync the file up
  front so a per-commit `fdatasync` carries no `i_size`/extent-map metadata
  change). The most insightful next experiment, but the preallocation is fiddly
  and uneven across the three std libraries — deferred.
- A **write-only** (page-cache, no durability) baseline.
- `O_DIRECT` / `O_DSYNC`, CRC/record framing, multi-file segment rotation,
  concurrent writers / pipelining, and any cross-host component.
