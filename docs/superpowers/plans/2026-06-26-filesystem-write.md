# filesystem-write Benchmark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the `filesystem-write` stub into a real benchmark in Rust, Go, and Java — four experiments (`fsync`, `fdatasync`, `prealloc`, `batch`) measuring the cost of durably appending command-log entries.

**Architecture:** Each language's shared bench library gains a durable-append harness (config + file setup + warmup/timed loop + result emission); each experiment is a thin artifact named `filesystem-write-<experiment>` that supplies three parameters — sync primitive, batch size, and whether to preallocate. Mirrors the existing `network-rtt` structure exactly.

**Tech Stack:** Rust (std-only, `bench-common` crate), Go (std-only incl. `syscall.Fdatasync`, `internal/bench` package), Java 21 (`net.knego.hiperf.common`, `FileChannel.force`). Ansible/Terraform `bench-infra` wiring.

**Spec:** `docs/superpowers/specs/2026-06-26-filesystem-write-design.md`

## Global Constraints

- **Toolchains:** Rust 1.96 (edition 2024), Go 1.22, Java 21, Gradle 8.10.2 (via `./gradlew` wrapper). Std-only — **no new dependencies** in any language.
- **stdout is results-only.** One result-contract JSON line per metric; all logs/diagnostics to stderr.
- **Experiment grid (4):** `fsync` (SyncKind Full, batch 1, no prealloc), `fdatasync` (Data, 1, no prealloc), `prealloc` (Data, 1, **prealloc**), `batch` (Data, `FSW_BATCH`, **prealloc**).
- **Metrics per experiment (4 lines):** `durable_append_throughput` (`ops_per_sec`, samples = `FSW_ITERATIONS`); `sync_p50` / `sync_p99` / `sync_mean` (`ns`, samples = number of sync calls). p50/p99 integer ns, mean & throughput fractional.
- **Config (`FSW_` env):** `FSW_DIR` **required, no default** (unset → stderr + non-zero exit; guards against tmpfs); `FSW_ENTRY_BYTES`=256, `FSW_WARMUP`=5000, `FSW_ITERATIONS`=50000, `FSW_BATCH`=32. Invalid/non-positive numerics → stderr + non-zero exit.
- **Preallocation = real zero-write** of `(FSW_WARMUP + FSW_ITERATIONS) × FSW_ENTRY_BYTES` bytes + one `fsync`, then seek to 0 (never sparse `fallocate`/`set_len`). File opened **without** `O_APPEND` so `prealloc` can overwrite.
- **Naming exact:** focus area `filesystem-write`; experiments `fsync`/`fdatasync`/`prealloc`/`batch`; `language` matches the directory.
- **Per-language verification dir:** use `/var/tmp/fsw-verify` (disk-backed, not tmpfs) with small overrides `FSW_WARMUP=100 FSW_ITERATIONS=500 FSW_BATCH=8` for fast smoke runs.
- Keep Rust clippy- and rustfmt-clean; keep all existing tests green.

## File Structure

**Rust** (`rust/`):
- Modify `bench-common/src/result.rs` — add `unit` parameter to emitters.
- Modify `bench-common/src/measure.rs` — pass `"ns"` at network-rtt call sites.
- Create `bench-common/src/fswrite.rs` — `FsConfig`, `SyncKind`, harness, `emit_fs`, `run_and_emit`.
- Modify `bench-common/src/lib.rs` — `pub mod fswrite;`.
- Create `filesystem-write/{fsync,fdatasync,prealloc,batch}/{Cargo.toml,src/main.rs}`.
- Modify `rust/Cargo.toml` — workspace members.
- Delete `rust/filesystem-write/` (old single stub crate).

**Go** (`go/`):
- Create `internal/bench/fswrite.go` + `internal/bench/fswrite_test.go`.
- Create `cmd/filesystem-write-{fsync,fdatasync,prealloc,batch}/main.go`.
- Delete `cmd/filesystem-write/` (old stub) and any tracked `bin/filesystem-write`.

**Java** (`java/`):
- Create `common/src/main/java/net/knego/hiperf/common/{FsConfig.java,SyncKind.java,DurableAppend.java}`.
- Create `common/src/test/java/net/knego/hiperf/common/DurableAppendTest.java`.
- Create `filesystem-write-{fsync,fdatasync,prealloc,batch}/build.gradle.kts` + `src/main/java/net/knego/hiperf/filesystemwrite/<exp>/Main.java`.
- Modify `settings.gradle.kts` — subproject list.
- Delete `java/filesystem-write/` (old stub subproject).

**bench-infra** (`bench-infra/ansible/`):
- Modify `group_vars/all.yml` — experiments matrix + `fsw_*` params.
- Modify `roles/run/files/run_bench.sh` — export `FSW_*`.
- Modify `roles/run/tasks/local.yml` — export `fsw_*` into the run.

**Docs:**
- Modify `docs/result-contract.md` — current-state section.
- Modify `CLAUDE.md` — status + artifact-name lines.

---

## Task 1: Rust — parameterize the result `unit`

The Rust emitters hardcode `"unit":"ns"`; `filesystem-write` needs `ops_per_sec` too. Add a `unit` parameter and update the network-rtt call sites. Pure refactor — network-rtt output must be byte-identical.

**Files:**
- Modify: `rust/bench-common/src/result.rs`
- Modify: `rust/bench-common/src/measure.rs:53-55`

**Interfaces:**
- Produces: `result::emit(focus_area: &str, experiment: &str, metric: &str, value: u64, unit: &str, samples: usize)` and `result::emit_float(focus_area: &str, experiment: &str, metric: &str, value: f64, unit: &str, samples: usize)`.

- [ ] **Step 1: Add the `unit` parameter to both emitters**

In `rust/bench-common/src/result.rs`, replace the `emit` and `emit_float` functions (keep `emit_placeholder` unchanged):

```rust
/// Emit a result line with an integer `value`.
pub fn emit(focus_area: &str, experiment: &str, metric: &str, value: u64, unit: &str, samples: usize) {
    println!(
        r#"{{"language":"{LANGUAGE}","focus_area":"{focus_area}","experiment":"{experiment}","metric":"{metric}","value":{value},"unit":"{unit}","samples":{samples}}}"#
    );
}

/// Emit a result line with a (possibly fractional) numeric `value`.
pub fn emit_float(focus_area: &str, experiment: &str, metric: &str, value: f64, unit: &str, samples: usize) {
    println!(
        r#"{{"language":"{LANGUAGE}","focus_area":"{focus_area}","experiment":"{experiment}","metric":"{metric}","value":{value},"unit":"{unit}","samples":{samples}}}"#
    );
}
```

- [ ] **Step 2: Update the network-rtt call sites**

In `rust/bench-common/src/measure.rs`, the three calls inside `emit_rtt` become:

```rust
    result::emit(FOCUS_AREA, experiment, "rtt_p50", p50, "ns", n);
    result::emit(FOCUS_AREA, experiment, "rtt_p99", p99, "ns", n);
    result::emit_float(FOCUS_AREA, experiment, "rtt_mean", mean, "ns", n);
```

- [ ] **Step 3: Build + verify network-rtt output unchanged**

Run:
```bash
cd rust && cargo build --release 2>&1 | tail -2
cargo run --release -q -p network-rtt-tcp 2>/dev/null | head -1
```
Expected: builds clean; first line is a valid `rtt_p50` line with `"unit":"ns"`, e.g. `{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50",...,"unit":"ns",...}`.

- [ ] **Step 4: clippy + fmt**

Run:
```bash
cd rust && cargo clippy --all-targets 2>&1 | tail -3 && cargo fmt --check
```
Expected: no warnings, no diff.

- [ ] **Step 5: Commit**

```bash
git add rust/bench-common/src/result.rs rust/bench-common/src/measure.rs
git commit -m "rust(bench-common): parameterize result emit unit"
```

---

## Task 2: Rust — durable-append harness in bench-common

The shared harness owning config, file setup, the warmup/timed loop, and emission. TDD: a unit test drives the loop against a temp dir and asserts sync-sample counts (batch chunking, prealloc).

**Files:**
- Create: `rust/bench-common/src/fswrite.rs`
- Modify: `rust/bench-common/src/lib.rs:13-16`

**Interfaces:**
- Consumes: `crate::result::{emit, emit_float}` (Task 1), `crate::stats::{percentile, mean}`.
- Produces:
  - `pub struct FsConfig { pub dir: PathBuf, pub entry_bytes: usize, pub warmup: usize, pub iterations: usize, pub batch: usize }` with `FsConfig::from_env() -> Result<FsConfig, String>`.
  - `pub enum SyncKind { Full, Data }`.
  - `pub fn run_and_emit(experiment: &str, sync_kind: SyncKind, prealloc: bool, batched: bool) -> Result<(), String>`.

- [ ] **Step 1: Register the module**

In `rust/bench-common/src/lib.rs`, add `fswrite` to the module list (keep alphabetical):

```rust
pub mod config;
pub mod fswrite;
pub mod measure;
pub mod result;
pub mod stats;
```

- [ ] **Step 2: Write the failing test**

Create `rust/bench-common/src/fswrite.rs` with only the test module for now (it references items defined in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fswrite-test-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn batch_chunking_and_prealloc_sync_counts() {
        let dir = scratch_dir("counts");
        let cfg = FsConfig { dir: dir.clone(), entry_bytes: 64, warmup: 5, iterations: 20, batch: 4 };

        // batch 4 over 20 iterations → 5 syncs; throughput positive.
        let (samples, tput) =
            run_durable_append(&cfg, SyncKind::Data, 4, false, "test-batch").unwrap();
        assert_eq!(samples.len(), 5, "20 entries / batch 4 = 5 syncs");
        assert!(tput > 0.0);

        // non-divisible: 21 iterations, batch 4 → ceil = 6 syncs.
        let cfg2 = FsConfig { iterations: 21, ..cfg.clone() };
        let (s2, _) = run_durable_append(&cfg2, SyncKind::Data, 4, false, "test-batch2").unwrap();
        assert_eq!(s2.len(), 6, "ceil(21/4) = 6 syncs");

        // prealloc, batch 1 → 20 syncs; file at least the preallocated size.
        let (s3, _) = run_durable_append(&cfg, SyncKind::Data, 1, true, "test-prealloc").unwrap();
        assert_eq!(s3.len(), 20);
        let len = std::fs::metadata(dir.join("filesystem-write-test-prealloc.log")).unwrap().len();
        assert!(len >= ((cfg.warmup + cfg.iterations) * cfg.entry_bytes) as u64);

        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run:
```bash
cd rust && cargo test -p bench-common fswrite 2>&1 | tail -5
```
Expected: compile error — `FsConfig`, `SyncKind`, `run_durable_append` not found.

- [ ] **Step 4: Implement the harness**

Prepend the implementation above the test module in `rust/bench-common/src/fswrite.rs`:

```rust
//! filesystem-write durable-append harness, shared by the fsync/fdatasync/
//! prealloc/batch experiments. Each artifact supplies (SyncKind, batch_size,
//! prealloc); the harness owns config, file setup, warmup, the timed loop, and
//! result emission. stdout stays results-only; std-only.

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Instant;

use crate::result;
use crate::stats;

/// Focus area for every filesystem-write experiment.
pub const FOCUS_AREA: &str = "filesystem-write";

/// Which durability barrier to issue per commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// Full fsync (`File::sync_all`) — flushes data + all metadata.
    Full,
    /// fdatasync (`File::sync_data`) — flushes data + size, skips timestamps.
    Data,
}

/// Parsed, validated filesystem-write configuration (`FSW_*`).
#[derive(Debug, Clone)]
pub struct FsConfig {
    /// Directory for the bench file. Required — no default (guards against tmpfs).
    pub dir: PathBuf,
    /// Entry size in bytes.
    pub entry_bytes: usize,
    /// Discarded warmup operations.
    pub warmup: usize,
    /// Measured entries.
    pub iterations: usize,
    /// Entries per group-commit (the `batch` experiment only).
    pub batch: usize,
}

impl FsConfig {
    /// Read configuration from `FSW_*`, applying defaults and validating.
    pub fn from_env() -> Result<FsConfig, String> {
        let dir = match env::var("FSW_DIR") {
            Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim()),
            _ => {
                return Err(
                    "FSW_DIR: required (set FSW_DIR=<dir on a real disk, not tmpfs>)".to_string(),
                );
            }
        };
        Ok(FsConfig {
            dir,
            entry_bytes: parse_positive("FSW_ENTRY_BYTES", 256)?,
            warmup: parse_positive("FSW_WARMUP", 5_000)?,
            iterations: parse_positive("FSW_ITERATIONS", 50_000)?,
            batch: parse_positive("FSW_BATCH", 32)?,
        })
    }
}

fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => {
            let value: usize = raw.trim().parse().map_err(|_| {
                format!("{name}: invalid value {raw:?} (expected a positive integer)")
            })?;
            if value == 0 {
                return Err(format!("{name}: must be positive, got 0"));
            }
            Ok(value)
        }
    }
}

/// Build config from env, run the experiment, and emit its four result lines.
pub fn run_and_emit(
    experiment: &str,
    sync_kind: SyncKind,
    prealloc: bool,
    batched: bool,
) -> Result<(), String> {
    let cfg = FsConfig::from_env()?;
    let batch_size = if batched { cfg.batch } else { 1 };
    let (samples, throughput) =
        run_durable_append(&cfg, sync_kind, batch_size, prealloc, experiment)
            .map_err(|e| format!("{e}"))?;
    emit_fs(experiment, &samples, throughput, cfg.iterations);
    Ok(())
}

/// Run the durable-append loop. Returns (per-sync latencies in ns, throughput
/// in entries/sec). `batch_size` entries per sync; `prealloc` pre-writes the
/// file so a size-extending sync is avoided.
fn run_durable_append(
    cfg: &FsConfig,
    sync_kind: SyncKind,
    batch_size: usize,
    prealloc: bool,
    experiment: &str,
) -> io::Result<(Vec<u64>, f64)> {
    let path = cfg.dir.join(format!("filesystem-write-{experiment}.log"));
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    // Make the file's existence durable (file + parent dir), outside timing.
    file.sync_all()?;
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }

    let entry = vec![0xABu8; cfg.entry_bytes];

    if prealloc {
        let total = (cfg.warmup + cfg.iterations) * cfg.entry_bytes;
        let zeros = vec![0u8; 1024 * 1024];
        let mut remaining = total;
        while remaining > 0 {
            let n = remaining.min(zeros.len());
            file.write_all(&zeros[..n])?;
            remaining -= n;
        }
        file.sync_all()?;
        file.seek(SeekFrom::Start(0))?;
    }

    // Warmup — timings discarded.
    run_entries(&mut file, &entry, cfg.warmup, batch_size, sync_kind, None)?;

    // Measured — record each sync's latency; time the whole span for throughput.
    let mut samples = Vec::with_capacity(cfg.iterations.div_ceil(batch_size));
    let t_start = Instant::now();
    run_entries(&mut file, &entry, cfg.iterations, batch_size, sync_kind, Some(&mut samples))?;
    let throughput = cfg.iterations as f64 / t_start.elapsed().as_secs_f64();

    Ok((samples, throughput))
}

/// Write `entries` entries in chunks of `batch_size`, issuing one sync per chunk
/// (trailing short chunk allowed). When `samples` is `Some`, push each sync's
/// elapsed ns into it.
fn run_entries(
    file: &mut File,
    entry: &[u8],
    entries: usize,
    batch_size: usize,
    sync_kind: SyncKind,
    mut samples: Option<&mut Vec<u64>>,
) -> io::Result<()> {
    let mut remaining = entries;
    while remaining > 0 {
        let count = remaining.min(batch_size);
        for _ in 0..count {
            file.write_all(entry)?;
        }
        let start = Instant::now();
        match sync_kind {
            SyncKind::Full => file.sync_all()?,
            SyncKind::Data => file.sync_data()?,
        }
        if let Some(s) = samples.as_deref_mut() {
            s.push(start.elapsed().as_nanos() as u64);
        }
        remaining -= count;
    }
    Ok(())
}

/// Sort the sync samples and emit the four filesystem-write result lines.
fn emit_fs(experiment: &str, sync_samples: &[u64], throughput: f64, iterations: usize) {
    let mut sorted = sync_samples.to_vec();
    sorted.sort_unstable();
    let n_sync = sorted.len();

    result::emit_float(
        FOCUS_AREA, experiment, "durable_append_throughput", throughput, "ops_per_sec", iterations,
    );
    result::emit(FOCUS_AREA, experiment, "sync_p50", stats::percentile(&sorted, 50.0), "ns", n_sync);
    result::emit(FOCUS_AREA, experiment, "sync_p99", stats::percentile(&sorted, 99.0), "ns", n_sync);
    result::emit_float(FOCUS_AREA, experiment, "sync_mean", stats::mean(sync_samples), "ns", n_sync);
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run:
```bash
cd rust && cargo test -p bench-common fswrite 2>&1 | tail -5
```
Expected: `test fswrite::tests::batch_chunking_and_prealloc_sync_counts ... ok`.

- [ ] **Step 6: clippy + fmt**

Run:
```bash
cd rust && cargo clippy --all-targets 2>&1 | tail -3 && cargo fmt --check
```
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add rust/bench-common/src/fswrite.rs rust/bench-common/src/lib.rs
git commit -m "rust(bench-common): add filesystem-write durable-append harness"
```

---

## Task 3: Rust — the four artifacts + workspace wiring

Four thin binary crates over the harness; replace the single stub crate.

**Files:**
- Create: `rust/filesystem-write/{fsync,fdatasync,prealloc,batch}/Cargo.toml`
- Create: `rust/filesystem-write/{fsync,fdatasync,prealloc,batch}/src/main.rs`
- Modify: `rust/Cargo.toml:3-9`
- Delete: `rust/filesystem-write/Cargo.toml`, `rust/filesystem-write/src/main.rs`

**Interfaces:**
- Consumes: `bench_common::fswrite::{run_and_emit, SyncKind}` (Task 2).

- [ ] **Step 1: Remove the old stub crate**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
git rm rust/filesystem-write/Cargo.toml rust/filesystem-write/src/main.rs
```

- [ ] **Step 2: Update workspace members**

In `rust/Cargo.toml`, replace `"filesystem-write",` in the `members` list with the four crates:

```toml
members = [
    "bench-common",
    "network-rtt/tcp",
    "network-rtt/udp",
    "network-rtt/quic",
    "filesystem-write/fsync",
    "filesystem-write/fdatasync",
    "filesystem-write/prealloc",
    "filesystem-write/batch",
    "thread-handoff",
]
```

- [ ] **Step 3: Create the four crates**

For each experiment, create `rust/filesystem-write/<exp>/Cargo.toml` (substitute `<exp>`):

```toml
[package]
name = "filesystem-write-<exp>"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "filesystem-write-<exp>"
path = "src/main.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
```

And `rust/filesystem-write/<exp>/src/main.rs`. The four mains differ only in the doc line and the `run_and_emit` arguments:

`fsync/src/main.rs`:
```rust
//! filesystem-write **fsync** experiment (Rust): append one entry, full fsync
//! per entry. Emits four result-contract lines. See the design spec.

use bench_common::fswrite::{self, SyncKind};
use std::process::ExitCode;

const EXPERIMENT: &str = "fsync";

fn main() -> ExitCode {
    match fswrite::run_and_emit(EXPERIMENT, SyncKind::Full, false, false) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("filesystem-write-{EXPERIMENT}: {msg}");
            ExitCode::FAILURE
        }
    }
}
```

`fdatasync/src/main.rs` — identical but:
```rust
const EXPERIMENT: &str = "fdatasync";
// ...
    match fswrite::run_and_emit(EXPERIMENT, SyncKind::Data, false, false) {
```

`prealloc/src/main.rs`:
```rust
const EXPERIMENT: &str = "prealloc";
// ...
    match fswrite::run_and_emit(EXPERIMENT, SyncKind::Data, true, false) {
```

`batch/src/main.rs`:
```rust
const EXPERIMENT: &str = "batch";
// ...
    match fswrite::run_and_emit(EXPERIMENT, SyncKind::Data, true, true) {
```

(Update each doc comment's first line to name its experiment.)

- [ ] **Step 4: Build the workspace**

Run:
```bash
cd rust && cargo build --release 2>&1 | tail -3
```
Expected: all four `filesystem-write-*` binaries compile.

- [ ] **Step 5: Run each artifact and verify four well-formed lines**

Run:
```bash
mkdir -p /var/tmp/fsw-verify
cd rust
for e in fsync fdatasync prealloc batch; do
  echo "== $e =="
  FSW_DIR=/var/tmp/fsw-verify FSW_WARMUP=100 FSW_ITERATIONS=500 FSW_BATCH=8 \
    cargo run --release -q -p filesystem-write-$e 2>/dev/null
done
```
Expected per experiment: 4 lines — one `durable_append_throughput` (`ops_per_sec`, samples 500) and `sync_p50`/`sync_p99`/`sync_mean` (`ns`). `sync_p99 ≥ sync_p50`; `batch`/`prealloc` throughput visibly higher than `fdatasync`.

- [ ] **Step 6: Verify FSW_DIR-required behavior**

Run:
```bash
cd rust && (unset FSW_DIR; cargo run --release -q -p filesystem-write-fsync; echo "exit=$?")
```
Expected: stderr message `filesystem-write-fsync: FSW_DIR: required ...`, `exit=1`, no stdout.

- [ ] **Step 7: clippy + fmt + full test**

Run:
```bash
cd rust && cargo clippy --all-targets 2>&1 | tail -3 && cargo fmt --check && cargo test 2>&1 | tail -5
```
Expected: clean, tests pass.

- [ ] **Step 8: Commit**

```bash
git add rust/Cargo.toml rust/filesystem-write
git commit -m "rust: implement filesystem-write fsync/fdatasync/prealloc/batch artifacts"
```

---

## Task 4: Go — durable-append harness in internal/bench

**Files:**
- Create: `go/internal/bench/fswrite.go`
- Create: `go/internal/bench/fswrite_test.go`

**Interfaces:**
- Consumes: `positiveEnv` (config.go), `Emit`, `Percentile`, `Mean` (existing).
- Produces:
  - `type FsConfig struct { Dir string; EntryBytes, Warmup, Iterations, Batch int }` + `LoadFsConfig() (FsConfig, error)`.
  - `type SyncKind int` with `SyncFull`, `SyncData`.
  - `RunDurableAppend(cfg FsConfig, experiment string, sync SyncKind, batchSize int, prealloc bool) ([]int64, float64, error)`.
  - `EmitFS(experiment string, samples []int64, throughput float64, iterations int)`.

- [ ] **Step 1: Write the failing test**

Create `go/internal/bench/fswrite_test.go`:

```go
package bench

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRunDurableAppendBatchAndPrealloc(t *testing.T) {
	dir := t.TempDir()
	cfg := FsConfig{Dir: dir, EntryBytes: 64, Warmup: 5, Iterations: 20, Batch: 4}

	samples, tput, err := RunDurableAppend(cfg, "test-batch", SyncData, 4, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(samples) != 5 {
		t.Fatalf("want 5 syncs (20/4), got %d", len(samples))
	}
	if tput <= 0 {
		t.Fatalf("want positive throughput, got %v", tput)
	}

	// Non-divisible: ceil(21/4) = 6.
	cfg2 := cfg
	cfg2.Iterations = 21
	s2, _, err := RunDurableAppend(cfg2, "test-batch2", SyncData, 4, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(s2) != 6 {
		t.Fatalf("want 6 syncs (ceil 21/4), got %d", len(s2))
	}

	// Prealloc, batch 1 → 20 syncs; file at least the preallocated size.
	s3, _, err := RunDurableAppend(cfg, "test-prealloc", SyncData, 1, true)
	if err != nil {
		t.Fatal(err)
	}
	if len(s3) != 20 {
		t.Fatalf("want 20 syncs, got %d", len(s3))
	}
	fi, err := os.Stat(filepath.Join(dir, "filesystem-write-test-prealloc.log"))
	if err != nil {
		t.Fatal(err)
	}
	if min := int64((cfg.Warmup + cfg.Iterations) * cfg.EntryBytes); fi.Size() < min {
		t.Fatalf("prealloc file too small: %d < %d", fi.Size(), min)
	}
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```bash
cd go && go test ./internal/bench/ 2>&1 | tail -5
```
Expected: compile failure — `FsConfig`, `RunDurableAppend`, `SyncData` undefined.

- [ ] **Step 3: Implement the harness**

Create `go/internal/bench/fswrite.go`:

```go
package bench

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"syscall"
	"time"
)

// FSWFocusArea is the focus area for every filesystem-write experiment.
const FSWFocusArea = "filesystem-write"

// SyncKind selects the durability barrier issued per commit.
type SyncKind int

const (
	// SyncFull is a full fsync (data + all metadata).
	SyncFull SyncKind = iota
	// SyncData is fdatasync (data + size, skips timestamps).
	SyncData
)

// FsConfig holds the filesystem-write parameters from the FSW_* env vars.
type FsConfig struct {
	Dir        string
	EntryBytes int
	Warmup     int
	Iterations int
	Batch      int
}

// LoadFsConfig reads FSW_DIR (required), FSW_ENTRY_BYTES, FSW_WARMUP,
// FSW_ITERATIONS and FSW_BATCH, applying defaults. A missing FSW_DIR (guards
// against tmpfs) or any invalid value yields an error.
func LoadFsConfig() (FsConfig, error) {
	dir := os.Getenv("FSW_DIR")
	if dir == "" {
		return FsConfig{}, fmt.Errorf("FSW_DIR: required (set FSW_DIR=<dir on a real disk, not tmpfs>)")
	}
	entryBytes, err := positiveEnv("FSW_ENTRY_BYTES", 256)
	if err != nil {
		return FsConfig{}, err
	}
	warmup, err := positiveEnv("FSW_WARMUP", 5000)
	if err != nil {
		return FsConfig{}, err
	}
	iterations, err := positiveEnv("FSW_ITERATIONS", 50000)
	if err != nil {
		return FsConfig{}, err
	}
	batch, err := positiveEnv("FSW_BATCH", 32)
	if err != nil {
		return FsConfig{}, err
	}
	return FsConfig{Dir: dir, EntryBytes: entryBytes, Warmup: warmup, Iterations: iterations, Batch: batch}, nil
}

// RunDurableAppend runs one filesystem-write experiment and returns the per-sync
// latencies in nanoseconds plus the end-to-end throughput in entries/sec.
// batchSize entries are written per sync; prealloc pre-writes the file so a
// size-extending sync is avoided.
func RunDurableAppend(cfg FsConfig, experiment string, sync SyncKind, batchSize int, prealloc bool) ([]int64, float64, error) {
	path := filepath.Join(cfg.Dir, "filesystem-write-"+experiment+".log")
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
	if err != nil {
		return nil, 0, fmt.Errorf("open %s: %w", path, err)
	}
	defer f.Close()

	// Make the file's existence durable (file + parent dir), outside timing.
	if err := f.Sync(); err != nil {
		return nil, 0, fmt.Errorf("initial sync: %w", err)
	}
	if err := syncDir(cfg.Dir); err != nil {
		return nil, 0, err
	}

	entry := make([]byte, cfg.EntryBytes)
	for i := range entry {
		entry[i] = 0xAB
	}

	if prealloc {
		if err := preallocate(f, (cfg.Warmup+cfg.Iterations)*cfg.EntryBytes); err != nil {
			return nil, 0, err
		}
	}

	doSync := func() error {
		if sync == SyncFull {
			return f.Sync()
		}
		return syscall.Fdatasync(int(f.Fd()))
	}

	// Warmup (discarded).
	if err := runEntries(f, entry, cfg.Warmup, batchSize, doSync, nil); err != nil {
		return nil, 0, err
	}

	// Measured.
	samples := make([]int64, 0, (cfg.Iterations+batchSize-1)/batchSize)
	tStart := time.Now()
	if err := runEntries(f, entry, cfg.Iterations, batchSize, doSync, &samples); err != nil {
		return nil, 0, err
	}
	throughput := float64(cfg.Iterations) / time.Since(tStart).Seconds()
	return samples, throughput, nil
}

// runEntries writes `entries` entries in chunks of batchSize, syncing once per
// chunk (trailing short chunk allowed). When samples != nil, each sync's elapsed
// ns is appended.
func runEntries(f *os.File, entry []byte, entries, batchSize int, doSync func() error, samples *[]int64) error {
	remaining := entries
	for remaining > 0 {
		count := batchSize
		if remaining < count {
			count = remaining
		}
		for i := 0; i < count; i++ {
			if _, err := f.Write(entry); err != nil {
				return fmt.Errorf("write: %w", err)
			}
		}
		start := time.Now()
		if err := doSync(); err != nil {
			return fmt.Errorf("sync: %w", err)
		}
		if samples != nil {
			*samples = append(*samples, time.Since(start).Nanoseconds())
		}
		remaining -= count
	}
	return nil
}

// preallocate real-zero-writes `total` bytes, fsyncs once, and seeks back to 0
// so the timed loop overwrites already-written blocks (no i_size extension).
func preallocate(f *os.File, total int) error {
	zeros := make([]byte, 1024*1024)
	remaining := total
	for remaining > 0 {
		n := len(zeros)
		if remaining < n {
			n = remaining
		}
		if _, err := f.Write(zeros[:n]); err != nil {
			return fmt.Errorf("preallocate write: %w", err)
		}
		remaining -= n
	}
	if err := f.Sync(); err != nil {
		return fmt.Errorf("preallocate sync: %w", err)
	}
	if _, err := f.Seek(0, io.SeekStart); err != nil {
		return fmt.Errorf("preallocate seek: %w", err)
	}
	return nil
}

// syncDir fsyncs the directory so a newly created file's entry is durable.
func syncDir(dir string) error {
	d, err := os.Open(dir)
	if err != nil {
		return fmt.Errorf("open dir %s: %w", dir, err)
	}
	defer d.Close()
	if err := d.Sync(); err != nil {
		return fmt.Errorf("sync dir %s: %w", dir, err)
	}
	return nil
}

// EmitFS sorts the per-sync samples and emits the four filesystem-write result
// lines. samples is sorted in place.
func EmitFS(experiment string, samples []int64, throughput float64, iterations int) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	nSync := int64(len(samples))
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "durable_append_throughput", Value: throughput, Unit: "ops_per_sec", Samples: int64(iterations)})
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "sync_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: nSync})
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "sync_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: nSync})
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "sync_mean", Value: Mean(samples), Unit: "ns", Samples: nSync})
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run:
```bash
cd go && go test ./internal/bench/ 2>&1 | tail -5
```
Expected: `ok  github.com/peterknego/hi-perf-cmp/go/internal/bench`.

- [ ] **Step 5: vet**

Run:
```bash
cd go && go vet ./internal/bench/ 2>&1 | tail -3
```
Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add go/internal/bench/fswrite.go go/internal/bench/fswrite_test.go
git commit -m "go(internal/bench): add filesystem-write durable-append harness"
```

---

## Task 5: Go — the four cmd artifacts

**Files:**
- Create: `go/cmd/filesystem-write-{fsync,fdatasync,prealloc,batch}/main.go`
- Delete: `go/cmd/filesystem-write/main.go` (and any tracked `go/bin/filesystem-write`)

**Interfaces:**
- Consumes: `bench.{LoadFsConfig, RunDurableAppend, EmitFS, SyncFull, SyncData, Fatalf}` (Task 4).

- [ ] **Step 1: Remove the old stub**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
git rm go/cmd/filesystem-write/main.go
git ls-files go/bin/filesystem-write --error-unmatch >/dev/null 2>&1 && git rm go/bin/filesystem-write || true
```

- [ ] **Step 2: Create the four mains**

For each experiment create `go/cmd/filesystem-write-<exp>/main.go`. They differ only in the `experiment` const and the `(SyncKind, batchSize, prealloc)` args.

`filesystem-write-fsync/main.go`:
```go
// filesystem-write-fsync benchmark (Go): append one entry, full fsync per entry.
// Emits four result-contract JSON lines. See the filesystem-write design spec.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

const experiment = "fsync"

func main() {
	cfg, err := bench.LoadFsConfig()
	if err != nil {
		bench.Fatalf("filesystem-write-"+experiment, "%v", err)
	}
	samples, throughput, err := bench.RunDurableAppend(cfg, experiment, bench.SyncFull, 1, false)
	if err != nil {
		bench.Fatalf("filesystem-write-"+experiment, "%v", err)
	}
	bench.EmitFS(experiment, samples, throughput, cfg.Iterations)
}
```

`filesystem-write-fdatasync/main.go` — `experiment = "fdatasync"`, call:
```go
	samples, throughput, err := bench.RunDurableAppend(cfg, experiment, bench.SyncData, 1, false)
```

`filesystem-write-prealloc/main.go` — `experiment = "prealloc"`, call:
```go
	samples, throughput, err := bench.RunDurableAppend(cfg, experiment, bench.SyncData, 1, true)
```

`filesystem-write-batch/main.go` — `experiment = "batch"`, call:
```go
	samples, throughput, err := bench.RunDurableAppend(cfg, experiment, bench.SyncData, cfg.Batch, true)
```

(Update each header comment's first line to name its experiment.)

- [ ] **Step 3: Build + vet**

Run:
```bash
cd go && go build ./... && go vet ./... 2>&1 | tail -3
```
Expected: clean; the four `cmd/filesystem-write-*` packages compile.

- [ ] **Step 4: Run each artifact and verify**

Run:
```bash
mkdir -p /var/tmp/fsw-verify
cd go
for e in fsync fdatasync prealloc batch; do
  echo "== $e =="
  FSW_DIR=/var/tmp/fsw-verify FSW_WARMUP=100 FSW_ITERATIONS=500 FSW_BATCH=8 \
    go run ./cmd/filesystem-write-$e 2>/dev/null
done
```
Expected: 4 lines per experiment, `"language":"go"`, units `ops_per_sec`/`ns`, `sync_p99 ≥ sync_p50`.

- [ ] **Step 5: Verify FSW_DIR-required behavior**

Run:
```bash
cd go && (unset FSW_DIR; go run ./cmd/filesystem-write-fsync; echo "exit=$?")
```
Expected: stderr `filesystem-write-fsync: FSW_DIR: required ...`, `exit=1`, no stdout.

- [ ] **Step 6: Full test pass**

Run:
```bash
cd go && go test ./... 2>&1 | tail -5
```
Expected: all packages ok.

- [ ] **Step 7: Commit**

```bash
git add go/cmd
git commit -m "go: implement filesystem-write fsync/fdatasync/prealloc/batch artifacts"
```

---

## Task 6: Java — durable-append harness in :common

**Files:**
- Create: `java/common/src/main/java/net/knego/hiperf/common/FsConfig.java`
- Create: `java/common/src/main/java/net/knego/hiperf/common/SyncKind.java`
- Create: `java/common/src/main/java/net/knego/hiperf/common/DurableAppend.java`
- Create: `java/common/src/test/java/net/knego/hiperf/common/DurableAppendTest.java`

**Interfaces:**
- Consumes: `Result` (record ctor `(focusArea, experiment, metric, value, unit, samples, notes)`), `Stats.percentile(long[], double)`, `Stats.mean(long[])`.
- Produces:
  - `FsConfig` record `(String dir, int entryBytes, int warmup, int iterations, int batch)` + `FsConfig.fromEnv()`.
  - `enum SyncKind { FULL, DATA }`.
  - `DurableAppend.Outcome` record `(long[] syncSamples, double throughput)`; `DurableAppend.run(FsConfig, String experiment, SyncKind, int batchSize, boolean prealloc)`; `DurableAppend.emit(String experiment, long[] syncSamples, double throughput, int iterations)`.

- [ ] **Step 1: Create SyncKind**

Create `java/common/src/main/java/net/knego/hiperf/common/SyncKind.java`:
```java
package net.knego.hiperf.common;

/** Which durability barrier to issue per commit. */
public enum SyncKind {
    /** Full fsync via {@code FileChannel.force(true)} — data + all metadata. */
    FULL,
    /** fdatasync via {@code FileChannel.force(false)} — data + size, no timestamps. */
    DATA
}
```

- [ ] **Step 2: Create FsConfig**

Create `java/common/src/main/java/net/knego/hiperf/common/FsConfig.java`:
```java
package net.knego.hiperf.common;

/**
 * filesystem-write configuration from the {@code FSW_*} env vars. {@code FSW_DIR}
 * is required (no default) to avoid silently benchmarking a tmpfs; numeric values
 * must be positive integers.
 */
public record FsConfig(String dir, int entryBytes, int warmup, int iterations, int batch) {

    public static FsConfig fromEnv() {
        String dir = trimmedOrNull(System.getenv("FSW_DIR"));
        if (dir == null) {
            throw new IllegalArgumentException(
                    "FSW_DIR is required (set FSW_DIR=<dir on a real disk, not tmpfs>)");
        }
        return new FsConfig(
                dir,
                readPositiveInt("FSW_ENTRY_BYTES", 256),
                readPositiveInt("FSW_WARMUP", 5000),
                readPositiveInt("FSW_ITERATIONS", 50000),
                readPositiveInt("FSW_BATCH", 32));
    }

    private static String trimmedOrNull(String raw) {
        if (raw == null) {
            return null;
        }
        String t = raw.trim();
        return t.isEmpty() ? null : t;
    }

    private static int readPositiveInt(String name, int def) {
        String raw = System.getenv(name);
        if (raw == null || raw.isEmpty()) {
            return def;
        }
        int value;
        try {
            value = Integer.parseInt(raw.trim());
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException(name + " must be a positive integer, got: " + raw);
        }
        if (value <= 0) {
            throw new IllegalArgumentException(name + " must be a positive integer, got: " + raw);
        }
        return value;
    }
}
```

- [ ] **Step 3: Write the failing test**

Create `java/common/src/test/java/net/knego/hiperf/common/DurableAppendTest.java`:
```java
package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class DurableAppendTest {

    @Test
    void batchAndPreallocProduceExpectedSyncCounts(@TempDir Path dir) throws IOException {
        FsConfig cfg = new FsConfig(dir.toString(), 64, 5, 20, 4);

        DurableAppend.Outcome batch = DurableAppend.run(cfg, "test-batch", SyncKind.DATA, 4, false);
        assertEquals(5, batch.syncSamples().length, "20 entries / batch 4 = 5 syncs");
        assertTrue(batch.throughput() > 0);

        FsConfig odd = new FsConfig(dir.toString(), 64, 5, 21, 4);
        DurableAppend.Outcome batch2 = DurableAppend.run(odd, "test-batch2", SyncKind.DATA, 4, false);
        assertEquals(6, batch2.syncSamples().length, "ceil(21/4) = 6 syncs");

        DurableAppend.Outcome pre = DurableAppend.run(cfg, "test-prealloc", SyncKind.DATA, 1, true);
        assertEquals(20, pre.syncSamples().length);
        long min = (long) (cfg.warmup() + cfg.iterations()) * cfg.entryBytes();
        assertTrue(Files.size(dir.resolve("filesystem-write-test-prealloc.log")) >= min,
                "prealloc file must be at least the preallocated size");
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run:
```bash
cd java && ./gradlew :common:test --tests '*DurableAppendTest*' 2>&1 | tail -8
```
Expected: compilation failure — `DurableAppend` does not exist.

- [ ] **Step 5: Implement DurableAppend**

Create `java/common/src/main/java/net/knego/hiperf/common/DurableAppend.java`:
```java
package net.knego.hiperf.common;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Arrays;

/**
 * Shared durable-append harness for the filesystem-write experiments. Owns file
 * setup (incl. optional preallocation), the warmup + timed loop, and result
 * emission; each experiment supplies (SyncKind, batchSize, prealloc).
 */
public final class DurableAppend {

    /** Focus area shared by all filesystem-write experiments. */
    public static final String FOCUS_AREA = "filesystem-write";

    private DurableAppend() {}

    /** Per-sync latencies (ns) and the end-to-end throughput (entries/sec). */
    public record Outcome(long[] syncSamples, double throughput) {}

    public static Outcome run(FsConfig cfg, String experiment, SyncKind sync, int batchSize, boolean prealloc)
            throws IOException {
        Path path = Path.of(cfg.dir(), "filesystem-write-" + experiment + ".log");
        try (FileChannel ch = FileChannel.open(path,
                StandardOpenOption.CREATE, StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING)) {
            // Make the file's existence durable (file + parent dir), outside timing.
            ch.force(true);
            syncDir(cfg.dir());

            byte[] fill = new byte[cfg.entryBytes()];
            Arrays.fill(fill, (byte) 0xAB);
            ByteBuffer entry = ByteBuffer.allocateDirect(cfg.entryBytes());
            entry.put(fill).flip();

            if (prealloc) {
                preallocate(ch, (long) (cfg.warmup() + cfg.iterations()) * cfg.entryBytes());
            }

            // Warmup (discarded).
            runEntries(ch, entry, cfg.warmup(), batchSize, sync, null);

            int nSyncs = (cfg.iterations() + batchSize - 1) / batchSize;
            long[] samples = new long[nSyncs];
            long tStart = System.nanoTime();
            runEntries(ch, entry, cfg.iterations(), batchSize, sync, samples);
            double throughput = cfg.iterations() / ((System.nanoTime() - tStart) / 1e9);
            return new Outcome(samples, throughput);
        }
    }

    private static void runEntries(FileChannel ch, ByteBuffer entry, int entries, int batchSize,
            SyncKind sync, long[] samples) throws IOException {
        int remaining = entries;
        int idx = 0;
        while (remaining > 0) {
            int count = Math.min(batchSize, remaining);
            for (int i = 0; i < count; i++) {
                entry.rewind();
                while (entry.hasRemaining()) {
                    ch.write(entry);
                }
            }
            long start = System.nanoTime();
            ch.force(sync == SyncKind.FULL); // force(true)=fsync, force(false)=fdatasync
            if (samples != null) {
                samples[idx++] = System.nanoTime() - start;
            }
            remaining -= count;
        }
    }

    private static void preallocate(FileChannel ch, long total) throws IOException {
        ByteBuffer zeros = ByteBuffer.allocateDirect(1024 * 1024);
        long written = 0;
        while (written < total) {
            zeros.clear();
            zeros.limit((int) Math.min(zeros.capacity(), total - written));
            while (zeros.hasRemaining()) {
                written += ch.write(zeros);
            }
        }
        ch.force(true);
        ch.position(0);
    }

    private static void syncDir(String dir) {
        try (FileChannel dc = FileChannel.open(Path.of(dir), StandardOpenOption.READ)) {
            dc.force(true);
        } catch (IOException e) {
            // Some platforms disallow opening a directory as a channel; the file
            // fsync above is the primary durability guarantee. Best-effort.
        }
    }

    /** Sort the sync samples and emit the four filesystem-write result lines. */
    public static void emit(String experiment, long[] syncSamples, double throughput, int iterations) {
        long[] sorted = syncSamples.clone();
        Arrays.sort(sorted);
        long nSync = sorted.length;
        new Result(FOCUS_AREA, experiment, "durable_append_throughput", throughput, "ops_per_sec", iterations, "").emit();
        new Result(FOCUS_AREA, experiment, "sync_p50", Stats.percentile(sorted, 50), "ns", nSync, "").emit();
        new Result(FOCUS_AREA, experiment, "sync_p99", Stats.percentile(sorted, 99), "ns", nSync, "").emit();
        new Result(FOCUS_AREA, experiment, "sync_mean", Stats.mean(sorted), "ns", nSync, "").emit();
    }
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run:
```bash
cd java && ./gradlew :common:test --tests '*DurableAppendTest*' 2>&1 | tail -8
```
Expected: BUILD SUCCESSFUL; the test passes.

- [ ] **Step 7: Commit**

```bash
git add java/common/src/main/java/net/knego/hiperf/common/FsConfig.java \
        java/common/src/main/java/net/knego/hiperf/common/SyncKind.java \
        java/common/src/main/java/net/knego/hiperf/common/DurableAppend.java \
        java/common/src/test/java/net/knego/hiperf/common/DurableAppendTest.java
git commit -m "java(common): add filesystem-write durable-append harness"
```

---

## Task 7: Java — the four subprojects

**Files:**
- Create: `java/filesystem-write-{fsync,fdatasync,prealloc,batch}/build.gradle.kts`
- Create: `java/filesystem-write-{fsync,fdatasync,prealloc,batch}/src/main/java/net/knego/hiperf/filesystemwrite/<exp>/Main.java`
- Modify: `java/settings.gradle.kts:3-10`
- Delete: `java/filesystem-write/` (old stub subproject)

**Interfaces:**
- Consumes: `DurableAppend`, `FsConfig`, `SyncKind` (Task 6).

- [ ] **Step 1: Remove the old stub subproject**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
git rm -r java/filesystem-write
```

- [ ] **Step 2: Update settings.gradle.kts**

In `java/settings.gradle.kts`, replace `"filesystem-write",` with the four subprojects:
```kotlin
include(
    "common",
    "network-rtt-tcp",
    "network-rtt-udp",
    "network-rtt-quic",
    "filesystem-write-fsync",
    "filesystem-write-fdatasync",
    "filesystem-write-prealloc",
    "filesystem-write-batch",
    "thread-handoff",
)
```

- [ ] **Step 3: Create the four build files**

For each experiment, create `java/filesystem-write-<exp>/build.gradle.kts`:
```kotlin
plugins {
    application
}

dependencies {
    implementation(project(":common"))
}

application {
    mainClass.set("net.knego.hiperf.filesystemwrite.<exp>.Main")
}
```

- [ ] **Step 4: Create the four Main classes**

For each experiment create `java/filesystem-write-<exp>/src/main/java/net/knego/hiperf/filesystemwrite/<exp>/Main.java`. They differ only in package, `EXPERIMENT`, and the `(SyncKind, batchSize, prealloc)` args.

`fsync` (package `...filesystemwrite.fsync`):
```java
package net.knego.hiperf.filesystemwrite.fsync;

import net.knego.hiperf.common.DurableAppend;
import net.knego.hiperf.common.FsConfig;
import net.knego.hiperf.common.SyncKind;

/**
 * filesystem-write / fsync experiment (Java): append one entry, full fsync per
 * entry. Emits four result-contract JSON lines. See docs/result-contract.md.
 */
public final class Main {

    private static final String EXPERIMENT = "fsync";

    public static void main(String[] args) {
        try {
            FsConfig cfg = FsConfig.fromEnv();
            DurableAppend.Outcome out = DurableAppend.run(cfg, EXPERIMENT, SyncKind.FULL, 1, false);
            DurableAppend.emit(EXPERIMENT, out.syncSamples(), out.throughput(), cfg.iterations());
        } catch (IllegalArgumentException e) {
            System.err.println("filesystem-write-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        } catch (Exception e) {
            System.err.println("filesystem-write-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

`fdatasync` (package `...filesystemwrite.fdatasync`, `EXPERIMENT = "fdatasync"`):
```java
            DurableAppend.Outcome out = DurableAppend.run(cfg, EXPERIMENT, SyncKind.DATA, 1, false);
```

`prealloc` (package `...filesystemwrite.prealloc`, `EXPERIMENT = "prealloc"`):
```java
            DurableAppend.Outcome out = DurableAppend.run(cfg, EXPERIMENT, SyncKind.DATA, 1, true);
```

`batch` (package `...filesystemwrite.batch`, `EXPERIMENT = "batch"`):
```java
            DurableAppend.Outcome out = DurableAppend.run(cfg, EXPERIMENT, SyncKind.DATA, cfg.batch(), true);
```

- [ ] **Step 5: Build (runs tests too)**

Run:
```bash
cd java && ./gradlew build 2>&1 | tail -5
```
Expected: BUILD SUCCESSFUL; all four subprojects compile, StatsTest + DurableAppendTest pass.

- [ ] **Step 6: Run each artifact and verify**

Run:
```bash
mkdir -p /var/tmp/fsw-verify
cd java
for e in fsync fdatasync prealloc batch; do
  echo "== $e =="
  FSW_DIR=/var/tmp/fsw-verify FSW_WARMUP=100 FSW_ITERATIONS=500 FSW_BATCH=8 \
    ./gradlew ":filesystem-write-$e:run" -q --no-daemon 2>/dev/null
done
```
Expected: 4 lines per experiment, `"language":"java"`, units `ops_per_sec`/`ns`, values as doubles (e.g. `48000.0`), `sync_p99 ≥ sync_p50`.

- [ ] **Step 7: Verify FSW_DIR-required behavior**

Run:
```bash
cd java && (unset FSW_DIR; ./gradlew ":filesystem-write-fsync:run" -q --no-daemon; echo "exit=$?")
```
Expected: stderr `filesystem-write-fsync: invalid configuration: FSW_DIR is required ...`, non-zero exit, no result lines on stdout.

- [ ] **Step 8: Commit**

```bash
git add java/settings.gradle.kts java/filesystem-write-fsync java/filesystem-write-fdatasync \
        java/filesystem-write-prealloc java/filesystem-write-batch
git commit -m "java: implement filesystem-write fsync/fdatasync/prealloc/batch artifacts"
```

---

## Task 8: bench-infra — wire filesystem-write into the matrix

Replace the placeholder local row with the four experiments, add `fsw_*` params, and export `FSW_*` into the local runs (CWD is the NVMe `scratch` dir on node0).

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml:8-22`
- Modify: `bench-infra/ansible/roles/run/files/run_bench.sh:52-60`
- Modify: `bench-infra/ansible/roles/run/tasks/local.yml:13-19`

**Interfaces:**
- Consumes: existing `run_bench.sh <language> <focus_area> <experiment> <mode>` contract; `local.yml` runs in `{{ remote_home }}/scratch`.

- [ ] **Step 1: Update the experiments matrix + add fsw_ params**

In `bench-infra/ansible/group_vars/all.yml`, replace the single `filesystem-write` placeholder row with four local rows:
```yaml
  - { focus_area: filesystem-write, experiment: fsync,     kind: local }
  - { focus_area: filesystem-write, experiment: fdatasync, kind: local }
  - { focus_area: filesystem-write, experiment: prealloc,  kind: local }
  - { focus_area: filesystem-write, experiment: batch,     kind: local }
```
Then, after the `rtt_*` params block, add:
```yaml
# --- filesystem-write params (exported into every local fs-write run so all
#     three languages use identical parameters for a fair comparison) ---
fsw_entry_bytes: 256
fsw_warmup: 5000
fsw_iterations: 50000
fsw_batch: 32
```

- [ ] **Step 2: Export FSW_* in run_bench.sh**

In `bench-infra/ansible/roles/run/files/run_bench.sh`, after the block exporting `RTT_ITERATIONS` (before the `case "$LANGUAGE"`), add:
```bash
# Export the filesystem-write contract. FSW_DIR defaults to the CWD, which the
# run role points at the NVMe-backed scratch dir. tmpfs would give meaningless
# durability numbers, so a real-disk dir is required.
export FSW_DIR="${FSW_DIR:-$PWD}"
export FSW_ENTRY_BYTES="${FSW_ENTRY_BYTES:-256}"
export FSW_WARMUP="${FSW_WARMUP:-5000}"
export FSW_ITERATIONS="${FSW_ITERATIONS:-50000}"
export FSW_BATCH="${FSW_BATCH:-32}"
```
Also update the header comment line `# filesystem-write / thread-handoff ignore the RTT_* vars.` to:
```bash
# filesystem-write consumes the FSW_* vars (below); thread-handoff ignores both.
```

- [ ] **Step 3: Export fsw_* from group_vars in local.yml**

In `bench-infra/ansible/roles/run/tasks/local.yml`, inside the "run on node0" shell block, add these exports before the `{{ remote_home }}/run_bench.sh` invocation (after the existing `cd {{ remote_home }}/scratch`):
```yaml
    export FSW_DIR="{{ remote_home }}/scratch"
    export FSW_ENTRY_BYTES="{{ fsw_entry_bytes }}"
    export FSW_WARMUP="{{ fsw_warmup }}"
    export FSW_ITERATIONS="{{ fsw_iterations }}"
    export FSW_BATCH="{{ fsw_batch }}"
```

- [ ] **Step 4: Lint the shell + YAML**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
bash -n bench-infra/ansible/roles/run/files/run_bench.sh && echo "run_bench.sh OK"
python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in ['bench-infra/ansible/group_vars/all.yml','bench-infra/ansible/roles/run/tasks/local.yml']]; print('YAML OK')"
```
Expected: `run_bench.sh OK` and `YAML OK`.

- [ ] **Step 5: End-to-end smoke-test run_bench.sh via the prebuilt Rust binary**

This drives the real script path (artifact-name resolution + `FSW_*` export + exec). Requires Task 3's `cargo build --release` to have run.

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
mkdir -p /var/tmp/fsw-verify
SRC_DIR=$PWD FSW_DIR=/var/tmp/fsw-verify FSW_WARMUP=100 FSW_ITERATIONS=500 FSW_BATCH=8 \
  bash bench-infra/ansible/roles/run/files/run_bench.sh rust filesystem-write batch loopback 2>/dev/null
```
Expected: 4 result lines with `"language":"rust"`, `"experiment":"batch"` — confirming the script resolves `filesystem-write-batch` and passes `FSW_*` through to the binary (the `loopback` mode arg sets `RTT_MODE`, which fs-write ignores).

- [ ] **Step 6: Commit**

```bash
git add bench-infra/ansible/group_vars/all.yml \
        bench-infra/ansible/roles/run/files/run_bench.sh \
        bench-infra/ansible/roles/run/tasks/local.yml
git commit -m "bench-infra: add filesystem-write fsync/fdatasync/prealloc/batch to the matrix"
```

---

## Task 9: Docs — reflect the new current state

**Files:**
- Modify: `docs/result-contract.md:48-53`
- Modify: `CLAUDE.md` (Status paragraph + "Artifact names" line)

- [ ] **Step 1: Update result-contract current state**

In `docs/result-contract.md`, replace the "Current state" paragraph with:
```markdown
`network-rtt` is implemented for the `tcp`, `udp`, and `quic` experiments, and
`filesystem-write` for the `fsync`, `fdatasync`, `prealloc`, and `batch`
experiments (each a separate runnable artifact named `<focus_area>-<experiment>`).
`thread-handoff` remains a **stub** that emits a single placeholder line
(`experiment: "placeholder"`, `metric: "placeholder"`, `notes: "stub"`).
```

- [ ] **Step 2: Update CLAUDE.md status + artifact names**

In `CLAUDE.md`, update the **Status** line under "What this is" to:
```markdown
**Status:** `network-rtt` is implemented for the `tcp`, `udp`, and `quic` experiments (cross-host capable).
`filesystem-write` is implemented for the `fsync`, `fdatasync`, `prealloc`, and `batch` experiments
(single-host, local NVMe). `thread-handoff` is a stub that emits a placeholder line; `shared-memory-ipc`
is not yet scaffolded.
```
And update the **Build & run** "Artifact names" line to:
```markdown
Artifact names: `network-rtt-{tcp,udp,quic}`, `filesystem-write-{fsync,fdatasync,prealloc,batch}`, `thread-handoff`.
```

- [ ] **Step 3: Verify no other stale "filesystem-write … stub" claims**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp
grep -rn -iE 'filesystem-write.{0,40}stub|filesystem-write.{0,20}placeholder' CLAUDE.md docs/ README.md 2>/dev/null || echo "no stale stub claims"
```
Expected: no matches referring to filesystem-write as a stub (thread-handoff/shared-memory-ipc mentions are fine).

- [ ] **Step 4: Commit**

```bash
git add docs/result-contract.md CLAUDE.md
git commit -m "docs: filesystem-write is implemented (fsync/fdatasync/prealloc/batch)"
```

---

## Final verification

- [ ] **All three languages build + test + run clean**

Run:
```bash
cd /home/claude/ultima/hi-perf-cmp/rust && cargo build --release 2>&1 | tail -1 && cargo test 2>&1 | tail -1 && cargo clippy --all-targets 2>&1 | tail -1 && cargo fmt --check && echo "RUST OK"
cd /home/claude/ultima/hi-perf-cmp/go && go build ./... && go vet ./... && go test ./... 2>&1 | tail -1 && echo "GO OK"
cd /home/claude/ultima/hi-perf-cmp/java && ./gradlew build 2>&1 | tail -1 && echo "JAVA OK"
```
Expected: `RUST OK`, `GO OK`, `JAVA OK`.

- [ ] **Cross-language line shape matches**

Run:
```bash
mkdir -p /var/tmp/fsw-verify
E="FSW_DIR=/var/tmp/fsw-verify FSW_WARMUP=100 FSW_ITERATIONS=500 FSW_BATCH=8"
cd /home/claude/ultima/hi-perf-cmp/rust && env $E cargo run --release -q -p filesystem-write-batch 2>/dev/null
cd /home/claude/ultima/hi-perf-cmp/go && env $E go run ./cmd/filesystem-write-batch 2>/dev/null
cd /home/claude/ultima/hi-perf-cmp/java && env $E ./gradlew :filesystem-write-batch:run -q --no-daemon 2>/dev/null
```
Expected: each prints 4 lines with the same `focus_area`/`experiment`/`metric`/`unit` set, differing only in `language` and values.
```
