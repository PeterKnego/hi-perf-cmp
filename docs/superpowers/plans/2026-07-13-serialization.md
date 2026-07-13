# serialization Focus Area Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Rust-only `serialization` focus area that benchmarks encode/decode latency **and** decode-time heap allocation for a ~500-byte SMR journal record across three codecs: `sbe_gen` (zerocopy SBE), `aeron_sbe` (real-logic SBE tool → Rust), and `bincode` (serde + bincode v2).

**Architecture:** Four new Cargo members under `rust/serialization/` (`common`, `bincode`, `sbe_gen`, `aeron_sbe`) plus a test-only `conformance` crate. A shared logical record + deterministic builder + checksum live in `serialization-common`. A new `bench-common::serial` module owns the journal write/replay timing harness, the env config, and a counting global allocator. Each codec cell is a thin `lib + bin`: the lib exposes `encode(record, &mut buf) -> usize` and `decode_checksum(bytes) -> u64`; the bin wires those into the harness and emits four result-contract lines.

**Tech Stack:** Rust edition 2024, `serde` 1 (derive), `bincode` 2 (serde bridge), `zerocopy` 0.8, `sbe_gen` 0.7.3 (build-dependency), real-logic `sbe-all` 1.38.1 (vendored jar, regenerated via committed script), JDK 21 (regeneration only).

## Global Constraints

- **stdout is result lines only.** All logs/progress/diagnostics go to stderr. Codegen (`build.rs`, the jar) runs at compile time and never touches benchmark stdout.
- **Emit via `bench-common`** `result::emit` (integer) / `result::emit_float` (fractional); never hand-roll JSON. Every line carries `language:"rust"`, `focus_area:"serialization"`, and `experiment ∈ {sbe_gen, aeron_sbe, bincode}`.
- **Record building is deterministic:** derived from the record index only — **no RNG, no wall-clock** (`Math.random`/`Date.now`/`Instant::now` never feed field values). A run is byte-reproducible.
- **Rust edition 2024**; member crates inherit `[workspace.package]` via `field.workspace = true`; shared deps go in `[workspace.dependencies]`.
- **Workspace stays clippy- and rustfmt-clean:** `cargo clippy --all-targets` and `cargo fmt --check` must pass.
- **Journaling rule:** only real AWS single-host runs are recorded via `tools/journal`; local runs are fitness checks. This plan does not journal anything.
- **Version pins:** `sbe_gen = "0.7.3"`, `zerocopy = "0.8"` (features `["derive"]`), `serde = "1"` (features `["derive"]`), `bincode = "2"` (features `["serde"]`), vendored `sbe-all-1.38.1.jar`.

---

### Task 1: `serialization-common` — logical record, deterministic builder, checksum

**Files:**
- Create: `rust/serialization/common/Cargo.toml`
- Create: `rust/serialization/common/src/lib.rs`
- Modify: `rust/Cargo.toml` (workspace `members` + `[workspace.dependencies]`)

**Interfaces:**
- Produces:
  - `pub struct Entry { pub entry_term_id: i64, pub entry_index: i64, pub entry_timestamp: i64, pub command_key: i32, pub command: Vec<u8> }` (derives `Serialize, Deserialize, Clone, PartialEq, Debug`)
  - `pub struct JournalRecord { pub leadership_term_id: i64, pub log_position: i64, pub timestamp: i64, pub cluster_session_id: i64, pub correlation_id: i64, pub leader_member_id: i32, pub service_id: i32, pub event_type: u8, pub flags: u8, pub entries: Vec<Entry> }` (same derives)
  - `pub fn build_record(index: u64, entries: usize, cmd_bytes: usize) -> JournalRecord` — deterministic from `index`
  - `pub struct Checksum(u64)` with `new()`, `add_i64(i64)`, `add_i32(i32)`, `add_u8(u8)`, `add_bytes(&[u8])`, `finish() -> u64` (all `#[inline]`)
  - `pub fn checksum_record(r: &JournalRecord) -> u64` — canonical field-order fold; the value every codec's `decode_checksum` must reproduce

- [ ] **Step 1: Add the workspace member and shared deps**

In `rust/Cargo.toml`, add `"serialization/common"` to `members`, and add to `[workspace.dependencies]`:

```toml
serde = { version = "1", features = ["derive"] }
bincode = { version = "2", features = ["serde"] }
zerocopy = { version = "0.8", features = ["derive"] }
sbe_gen = "0.7.3"
```

- [ ] **Step 2: Write `rust/serialization/common/Cargo.toml`**

```toml
[package]
name = "serialization-common"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = { workspace = true }
```

- [ ] **Step 3: Write the failing test (append to `rust/serialization/common/src/lib.rs`)**

Create `rust/serialization/common/src/lib.rs` with the test module first so it fails to compile (types absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_record_is_deterministic() {
        let a = build_record(42, 4, 78);
        let b = build_record(42, 4, 78);
        assert_eq!(a, b);
        assert_eq!(a.entries.len(), 4);
        assert_eq!(a.entries[0].command.len(), 78);
    }

    #[test]
    fn build_record_varies_by_index() {
        assert_ne!(build_record(1, 4, 78), build_record(2, 4, 78));
    }

    #[test]
    fn checksum_matches_manual_fold() {
        let r = build_record(7, 2, 8);
        let mut c = Checksum::new();
        c.add_i64(r.leadership_term_id);
        c.add_i64(r.log_position);
        c.add_i64(r.timestamp);
        c.add_i64(r.cluster_session_id);
        c.add_i64(r.correlation_id);
        c.add_i32(r.leader_member_id);
        c.add_i32(r.service_id);
        c.add_u8(r.event_type);
        c.add_u8(r.flags);
        for e in &r.entries {
            c.add_i64(e.entry_term_id);
            c.add_i64(e.entry_index);
            c.add_i64(e.entry_timestamp);
            c.add_i32(e.command_key);
            c.add_bytes(&e.command);
        }
        assert_eq!(checksum_record(&r), c.finish());
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cd rust && cargo test -p serialization-common`
Expected: FAIL — `cannot find function build_record` / `cannot find type Checksum`.

- [ ] **Step 5: Implement the types above the test module**

Prepend to `rust/serialization/common/src/lib.rs`:

```rust
//! Shared logical model for the `serialization` focus area: one ~500-byte SMR
//! journal record, a deterministic index-seeded builder, and a canonical
//! checksum every codec's decode must reproduce (the full-materialization proof).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub entry_term_id: i64,
    pub entry_index: i64,
    pub entry_timestamp: i64,
    pub command_key: i32,
    pub command: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JournalRecord {
    pub leadership_term_id: i64,
    pub log_position: i64,
    pub timestamp: i64,
    pub cluster_session_id: i64,
    pub correlation_id: i64,
    pub leader_member_id: i32,
    pub service_id: i32,
    pub event_type: u8,
    pub flags: u8,
    pub entries: Vec<Entry>,
}

/// Deterministic splitmix64 step — used only to spread field values from the
/// record index. Not cryptographic; chosen so a record is byte-reproducible
/// without any RNG state or wall-clock input.
#[inline]
fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Build one journal record deterministically from `index`, with `entries`
/// group members each carrying a `cmd_bytes`-long command payload. Defaults of
/// `entries = 4`, `cmd_bytes = 78` encode to ~500 bytes.
pub fn build_record(index: u64, entries: usize, cmd_bytes: usize) -> JournalRecord {
    let h = mix(index);
    let mut group = Vec::with_capacity(entries);
    for k in 0..entries as u64 {
        let e = mix(h ^ k.wrapping_mul(0x1000_0000_1B3));
        let mut command = vec![0u8; cmd_bytes];
        for (i, b) in command.iter_mut().enumerate() {
            *b = (e >> (i % 8 * 8)) as u8 ^ i as u8;
        }
        group.push(Entry {
            entry_term_id: e as i64,
            entry_index: (index * entries as u64 + k) as i64,
            entry_timestamp: mix(e) as i64,
            command_key: (e >> 32) as i32,
            command,
        });
    }
    JournalRecord {
        leadership_term_id: h as i64,
        log_position: (index as i64) << 8,
        timestamp: mix(h) as i64,
        cluster_session_id: (h >> 16) as i64,
        correlation_id: mix(h ^ 0xABCD) as i64,
        leader_member_id: (h >> 8) as i32,
        service_id: (h >> 24) as i32,
        event_type: (h & 1) as u8, // 0 = APPEND, 1 = SNAPSHOT
        flags: (h >> 1) as u8,
        entries: group,
    }
}

/// Order-sensitive checksum accumulator. Every codec folds the decoded fields
/// in the same order; equal outputs prove identical materialization.
pub struct Checksum(u64);

impl Checksum {
    #[inline]
    pub fn new() -> Self {
        Checksum(0xcbf2_9ce4_8422_2325) // FNV-1a offset basis
    }
    #[inline]
    fn step(&mut self, v: u64) {
        self.0 = (self.0 ^ v).wrapping_mul(0x0000_0100_0000_01B3);
    }
    #[inline]
    pub fn add_i64(&mut self, v: i64) {
        self.step(v as u64);
    }
    #[inline]
    pub fn add_i32(&mut self, v: i32) {
        self.step(v as u32 as u64);
    }
    #[inline]
    pub fn add_u8(&mut self, v: u8) {
        self.step(v as u64);
    }
    #[inline]
    pub fn add_bytes(&mut self, b: &[u8]) {
        self.step(b.len() as u64);
        for &x in b {
            self.step(x as u64);
        }
    }
    #[inline]
    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Checksum {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical fold over a fully-owned record (the bincode path uses this after
/// decoding to an owned struct). SBE cells fold the same order manually.
pub fn checksum_record(r: &JournalRecord) -> u64 {
    let mut c = Checksum::new();
    c.add_i64(r.leadership_term_id);
    c.add_i64(r.log_position);
    c.add_i64(r.timestamp);
    c.add_i64(r.cluster_session_id);
    c.add_i64(r.correlation_id);
    c.add_i32(r.leader_member_id);
    c.add_i32(r.service_id);
    c.add_u8(r.event_type);
    c.add_u8(r.flags);
    for e in &r.entries {
        c.add_i64(e.entry_term_id);
        c.add_i64(e.entry_index);
        c.add_i64(e.entry_timestamp);
        c.add_i32(e.command_key);
        c.add_bytes(&e.command);
    }
    c.finish()
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p serialization-common`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
git add rust/Cargo.toml rust/Cargo.lock rust/serialization/common
git commit -m "feat(serialization): common logical record, deterministic builder, checksum"
```

---

### Task 2: `bench-common::serial` — journal harness, config, counting allocator

**Files:**
- Create: `rust/bench-common/src/serial.rs`
- Modify: `rust/bench-common/src/lib.rs:16-22` (add `pub mod serial;`)

**Interfaces:**
- Consumes: `bench_common::result::{emit, emit_float}` (existing), `bench_common::stats` (existing).
- Produces:
  - `pub struct SerialConfig { pub warmup: usize, pub iterations: usize, pub entries: usize, pub cmd_bytes: usize }` with `pub fn from_env() -> Result<SerialConfig, String>`
  - `pub struct CountingAllocator;` implementing `std::alloc::GlobalAlloc` (wraps `System`), plus `pub fn allocated_bytes() -> usize` and `pub fn reset_allocated()`
  - `pub fn run_journal<R, B, E, D>(experiment: &str, cfg: &SerialConfig, build: B, encode: E, decode: D)` where `B: Fn(u64) -> R`, `E: FnMut(&R, &mut [u8]) -> usize`, `D: FnMut(&[u8]) -> u64` — generic over the record type `R` (so `bench-common` stays serde-free and focus-neutral); runs encode-timing, builds the in-memory journal, runs decode-timing + allocation measurement, and emits `encode_ns`, `decode_ns`, `encoded_bytes`, `decode_alloc_bytes`.

- [ ] **Step 1: Look at existing config + emit patterns**

Run: `sed -n '1,60p' rust/bench-common/src/shmem.rs` and `sed -n '1,45p' rust/bench-common/src/result.rs`
Purpose: mirror `ShmemConfig::from_env` env-parsing style and confirm `emit`/`emit_float` signatures: `emit(focus_area, experiment, metric, value:u64, unit, samples)`, `emit_float(..., value:f64, ...)`.

- [ ] **Step 2: Write the failing test (bottom of `rust/bench-common/src/serial.rs`)**

Create `rust/bench-common/src/serial.rs` containing only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        // With no env vars set, defaults land at the ~500-byte record shape.
        let cfg = SerialConfig {
            warmup: 1000,
            iterations: 100_000,
            entries: 4,
            cmd_bytes: 78,
        };
        assert_eq!(cfg.entries, 4);
        assert_eq!(cfg.cmd_bytes, 78);
    }

    #[test]
    fn counting_allocator_tracks_a_vec() {
        reset_allocated();
        let before = allocated_bytes();
        let v: Vec<u8> = Vec::with_capacity(4096);
        assert!(allocated_bytes() >= before + 4096);
        drop(v);
    }
}
```

Note: the allocator test only passes when the test binary installs `CountingAllocator` as its `#[global_allocator]`. Add at the very top of `rust/bench-common/src/serial.rs`:

```rust
#[cfg(test)]
#[global_allocator]
static TEST_ALLOC: CountingAllocator = CountingAllocator;
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd rust && cargo test -p bench-common serial`
Expected: FAIL — `cannot find type SerialConfig` / `CountingAllocator`.

- [ ] **Step 4: Implement the module (above the test block)**

```rust
//! Shared harness for the `serialization` focus area: env config, a counting
//! global allocator (deterministic decode-time memory measurement), and the
//! journal write/replay timed loop that emits the four result-contract metrics.
//!
//! stdout stays result-only; this module prints nothing but the emit lines.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::result;

const FOCUS: &str = "serialization";

/// Env-configurable knobs. `SER_ENTRIES`/`SER_CMD_BYTES` tune record size;
/// defaults (4 / 78) encode to ~500 bytes.
pub struct SerialConfig {
    pub warmup: usize,
    pub iterations: usize,
    pub entries: usize,
    pub cmd_bytes: usize,
}

impl SerialConfig {
    pub fn from_env() -> Result<SerialConfig, String> {
        Ok(SerialConfig {
            warmup: parse_env("SER_WARMUP", 1_000)?,
            iterations: parse_env("SER_ITERS", 100_000)?,
            entries: parse_env("SER_ENTRIES", 4)?,
            cmd_bytes: parse_env("SER_CMD_BYTES", 78)?,
        })
    }
}

fn parse_env(key: &str, default: usize) -> Result<usize, String> {
    match std::env::var(key) {
        Ok(v) => v
            .parse::<usize>()
            .map_err(|_| format!("{key}: expected a positive integer, got {v:?}")),
        Err(_) => Ok(default),
    }
}

// ---- counting global allocator ------------------------------------------------

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// Wraps the system allocator and sums the byte size of every allocation
/// request. Deterministic and repeatable — the memory signal the focus area
/// exists to compare. Install in each bench binary with `#[global_allocator]`.
pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            ALLOCATED.fetch_add(new_size - layout.size(), Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

pub fn allocated_bytes() -> usize {
    ALLOCATED.load(Ordering::Relaxed)
}

pub fn reset_allocated() {
    ALLOCATED.store(0, Ordering::Relaxed);
}

// ---- journal write/replay harness --------------------------------------------

/// Encode-time then replay-time driver. `build(index) -> R` produces one logical
/// record deterministically; `encode(record, &mut scratch) -> len` serializes
/// into a caller buffer; `decode(bytes) -> checksum` decodes and **fully
/// materializes** (folds every field) so lazy codecs pay for the reads.
///
/// Generic over the record type `R` so this module (and `bench-common` as a
/// whole) stays serde-free and focus-neutral — the codec cell supplies the
/// concrete builder.
///
/// Emits: `encode_ns` (mean), `decode_ns` (mean), `encoded_bytes` (one record),
/// `decode_alloc_bytes` (heap bytes allocated per decode, via the counting
/// allocator). All samples/scratch are preallocated so the allocator counter
/// reflects codec allocation only.
pub fn run_journal<R, B, E, D>(
    experiment: &str,
    cfg: &SerialConfig,
    build: B,
    mut encode: E,
    mut decode: D,
) where
    B: Fn(u64) -> R,
    E: FnMut(&R, &mut [u8]) -> usize,
    D: FnMut(&[u8]) -> u64,
{
    let n = cfg.iterations;
    // Pre-build the records (untimed); building is deterministic from index.
    let records: Vec<R> = (0..(cfg.warmup + n) as u64).map(&build).collect();

    let mut scratch = vec![0u8; 64 * 1024];
    let mut encode_ns: Vec<u64> = Vec::with_capacity(n);
    let mut record_len = 0usize;

    // Warmup encode.
    for r in records.iter().take(cfg.warmup) {
        record_len = encode(r, &mut scratch);
    }
    // Timed encode.
    for r in records.iter().skip(cfg.warmup) {
        let t0 = Instant::now();
        let len = encode(r, &mut scratch);
        let dt = t0.elapsed().as_nanos() as u64;
        black_box(&scratch[..len]);
        record_len = len;
        encode_ns.push(dt);
    }

    // Build the contiguous in-memory journal from the timed records.
    let mut journal = Vec::with_capacity(record_len * n + 64);
    let mut frames: Vec<(usize, usize)> = Vec::with_capacity(n);
    for r in records.iter().skip(cfg.warmup) {
        let start = journal.len();
        let len = encode(r, &mut scratch);
        journal.extend_from_slice(&scratch[..len]);
        frames.push((start, len));
    }

    let mut decode_ns: Vec<u64> = Vec::with_capacity(n);
    let mut sink = 0u64;

    // Warmup decode (also warms any lazy statics before we start counting).
    for &(off, len) in frames.iter().take(cfg.warmup.min(frames.len())) {
        sink ^= decode(&journal[off..off + len]);
    }

    reset_allocated();
    let alloc_before = allocated_bytes();
    for &(off, len) in &frames {
        let t0 = Instant::now();
        let sum = decode(&journal[off..off + len]);
        let dt = t0.elapsed().as_nanos() as u64;
        sink ^= sum;
        decode_ns.push(dt);
    }
    let alloc_after = allocated_bytes();
    black_box(sink);

    let decode_alloc_per = (alloc_after - alloc_before) / n.max(1);

    result::emit_float(FOCUS, experiment, "encode_ns", mean(&encode_ns), "ns", n);
    result::emit_float(FOCUS, experiment, "decode_ns", mean(&decode_ns), "ns", n);
    result::emit(FOCUS, experiment, "encoded_bytes", record_len as u64, "bytes", 1);
    result::emit(
        FOCUS,
        experiment,
        "decode_alloc_bytes",
        decode_alloc_per as u64,
        "bytes",
        n,
    );
}

fn mean(xs: &[u64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().map(|&x| x as f64).sum::<f64>() / xs.len() as f64
}
```

- [ ] **Step 5: Register the module**

In `rust/bench-common/src/lib.rs`, add `pub mod serial;` alongside the existing `pub mod shmem;` (keep alphabetical: after `pub mod result;`). `bench-common` gains **no** new dependency — the harness is generic over the record type, so it stays serde-free.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p bench-common serial`
Expected: PASS (2 tests). If the allocator test flakes on capacity rounding, it asserts `>=`, so it holds.

- [ ] **Step 7: Commit**

```bash
git add rust/bench-common
git commit -m "feat(serialization): bench-common serial harness, config, counting allocator"
```

---

### Task 3: `serialization-bincode` cell

**Files:**
- Create: `rust/serialization/bincode/Cargo.toml`
- Create: `rust/serialization/bincode/src/lib.rs`
- Create: `rust/serialization/bincode/src/main.rs`
- Modify: `rust/Cargo.toml` (`members`)

**Interfaces:**
- Consumes: `serialization_common::{JournalRecord, checksum_record}`; `bincode::config::standard()`; `bincode::serde::{encode_into_slice, decode_from_slice}`.
- Produces (library): `pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize`, `pub fn decode_checksum(bytes: &[u8]) -> u64`.

- [ ] **Step 1: Add workspace member**

In `rust/Cargo.toml`, add `"serialization/bincode"` to `members`.

- [ ] **Step 2: Write `rust/serialization/bincode/Cargo.toml`**

```toml
[package]
name = "serialization-bincode"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "serialization-bincode"
path = "src/main.rs"

[lib]
name = "serialization_bincode"
path = "src/lib.rs"

[dependencies]
serialization-common = { path = "../common" }
bench-common = { path = "../../bench-common" }
bincode = { workspace = true }
```

- [ ] **Step 3: Write the failing round-trip test in `src/lib.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serialization_common::{build_record, checksum_record};

    #[test]
    fn round_trip_checksum_matches() {
        let r = build_record(9, 4, 78);
        let mut buf = vec![0u8; 64 * 1024];
        let n = encode(&r, &mut buf);
        assert!(n > 400 && n < 700, "unexpected encoded size {n}");
        assert_eq!(decode_checksum(&buf[..n]), checksum_record(&r));
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cd rust && cargo test -p serialization-bincode`
Expected: FAIL — `cannot find function encode`.

- [ ] **Step 5: Implement `encode`/`decode_checksum` above the test**

```rust
//! bincode (serde + bincode v2) codec cell — the ergonomic derive baseline.

use bincode::config::{standard, Configuration};
use serialization_common::{checksum_record, JournalRecord};

#[inline]
fn cfg() -> Configuration {
    standard()
}

/// Serialize into a reused caller buffer (zero-alloc encode path).
pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize {
    bincode::serde::encode_into_slice(r, buf, cfg()).expect("bincode encode")
}

/// Decode to an owned record, then fold via the canonical checksum. bincode
/// reaches full materialization by constructing the owned struct (Vecs and all).
pub fn decode_checksum(bytes: &[u8]) -> u64 {
    let (r, _len): (JournalRecord, usize) =
        bincode::serde::decode_from_slice(bytes, cfg()).expect("bincode decode");
    checksum_record(&r)
}
```

- [ ] **Step 6: Run to verify it passes**

Run: `cd rust && cargo test -p serialization-bincode`
Expected: PASS.

- [ ] **Step 7: Write `src/main.rs`**

```rust
//! serialization **bincode** experiment binary.

use bench_common::serial::{run_journal, CountingAllocator, SerialConfig};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

const EXPERIMENT: &str = "bincode";

fn main() {
    let cfg = match SerialConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("serialization-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let (entries, cmd) = (cfg.entries, cfg.cmd_bytes);
    run_journal(
        EXPERIMENT,
        &cfg,
        |i| serialization_common::build_record(i, entries, cmd),
        serialization_bincode::encode,
        serialization_bincode::decode_checksum,
    );
}
```

- [ ] **Step 8: Build + smoke-run (tiny iteration count)**

Run: `cd rust && cargo build -p serialization-bincode && SER_WARMUP=10 SER_ITERS=100 cargo run -q -p serialization-bincode`
Expected: four JSON lines on stdout with `"experiment":"bincode"` and metrics `encode_ns`, `decode_ns`, `encoded_bytes` (~500), `decode_alloc_bytes` (> 0, since bincode allocates on decode).

- [ ] **Step 9: Commit**

```bash
git add rust/Cargo.toml rust/Cargo.lock rust/serialization/bincode
git commit -m "feat(serialization): bincode codec cell + bench binary"
```

---

### Task 4: `serialization-sbe_gen` cell (build.rs codegen)

**Files:**
- Create: `rust/serialization/sbe_gen/Cargo.toml`
- Create: `rust/serialization/sbe_gen/build.rs`
- Create: `rust/serialization/sbe_gen/schema/journal.xml`
- Create: `rust/serialization/sbe_gen/src/lib.rs`
- Create: `rust/serialization/sbe_gen/src/main.rs`
- Modify: `rust/Cargo.toml` (`members`)

**Interfaces:**
- Consumes: generated module `journal_record` (from `sbe_gen`), `serialization_common::{JournalRecord, Checksum}`.
- Produces (library): `pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize`, `pub fn decode_checksum(bytes: &[u8]) -> u64`, and `pub fn encode_body(r, buf) -> usize` (used by conformance for byte-identity — body only, no header).

- [ ] **Step 1: Add workspace member**

In `rust/Cargo.toml`, add `"serialization/sbe_gen"` to `members`.

- [ ] **Step 2: Write the shared schema `schema/journal.xml`**

Sets both `id` and `schemaId` so both SBE toolchains agree on the header (see plan header note). This exact schema is copied verbatim into the `aeron_sbe` cell in Task 5.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<sbe:messageSchema xmlns:sbe="http://fixprotocol.io/2016/sbe"
                   package="journal" id="7" schemaId="7" version="1"
                   byteOrder="littleEndian">
  <types>
    <composite name="messageHeader">
      <type name="blockLength" primitiveType="uint16"/>
      <type name="templateId"  primitiveType="uint16"/>
      <type name="schemaId"    primitiveType="uint16"/>
      <type name="version"     primitiveType="uint16"/>
    </composite>
    <composite name="groupSizeEncoding">
      <type name="blockLength" primitiveType="uint16"/>
      <type name="numInGroup"  primitiveType="uint16"/>
    </composite>
    <composite name="varDataEncoding">
      <type name="length"  primitiveType="uint32" maxValue="1073741824"/>
      <type name="varData" primitiveType="uint8" length="0"/>
    </composite>
    <enum name="EventType" encodingType="uint8">
      <validValue name="APPEND">0</validValue>
      <validValue name="SNAPSHOT">1</validValue>
    </enum>
  </types>
  <sbe:message name="JournalRecord" id="1" blockLength="50">
    <field name="leadershipTermId" id="1" type="int64"/>
    <field name="logPosition"      id="2" type="int64"/>
    <field name="timestamp"        id="3" type="int64"/>
    <field name="clusterSessionId" id="4" type="int64"/>
    <field name="correlationId"    id="5" type="int64"/>
    <field name="leaderMemberId"   id="6" type="int32"/>
    <field name="serviceId"        id="7" type="int32"/>
    <field name="eventType"        id="8" type="EventType"/>
    <field name="flags"            id="9" type="uint8"/>
    <group name="entries" id="10" dimensionType="groupSizeEncoding" blockLength="28">
      <field name="entryTermId"    id="11" type="int64"/>
      <field name="entryIndex"     id="12" type="int64"/>
      <field name="entryTimestamp" id="13" type="int64"/>
      <field name="commandKey"     id="14" type="int32"/>
      <data  name="command"        id="15" type="varDataEncoding"/>
    </group>
  </sbe:message>
</sbe:messageSchema>
```

- [ ] **Step 3: Write `Cargo.toml`**

```toml
[package]
name = "serialization-sbe_gen"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
build = "build.rs"

[[bin]]
name = "serialization-sbe_gen"
path = "src/main.rs"

[lib]
name = "serialization_sbe_gen"
path = "src/lib.rs"

[dependencies]
serialization-common = { path = "../common" }
bench-common = { path = "../../bench-common" }
zerocopy = { workspace = true }

[build-dependencies]
sbe_gen = { workspace = true }
```

- [ ] **Step 4: Write `build.rs`**

Generates the codec into `OUT_DIR/sbe`; the XML is the single source of truth.

```rust
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema = manifest.join("schema/journal.xml");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("sbe");

    println!("cargo:rerun-if-changed={}", schema.display());
    let xml = fs::read_to_string(&schema).expect("read schema");
    fs::create_dir_all(&out).expect("create out dir");
    sbe_gen::generate_to(&xml, &out, &sbe_gen::GeneratorOptions::default())
        .expect("sbe_gen generate");
}
```

- [ ] **Step 5: Write the failing round-trip test in `src/lib.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serialization_common::{build_record, checksum_record};

    #[test]
    fn round_trip_checksum_matches() {
        let r = build_record(9, 4, 78);
        let mut buf = vec![0u8; 64 * 1024];
        let n = encode(&r, &mut buf);
        assert!(n > 400 && n < 700, "unexpected encoded size {n}");
        assert_eq!(decode_checksum(&buf[..n]), checksum_record(&r));
    }
}
```

- [ ] **Step 6: Run to verify it fails**

Run: `cd rust && cargo test -p serialization-sbe_gen`
Expected: FAIL — `cannot find function encode` (build.rs still runs and generates the module).

- [ ] **Step 7: Implement `src/lib.rs` above the test**

Uses the verified generated API: `JournalRecord::encode_body_into` + `JournalRecordEncoder`/`entries` for encode; `parse_prefix` + `parse_entries`/`iter` for decode. Fixed fields read via `rec.<field>.get()`; group entry accessors return `Option<&I64>`; `entry.command` is a `VarData`.

```rust
//! sbe_gen (zerocopy SBE) codec cell.
//!
//! The generated modules are produced by build.rs into `OUT_DIR/sbe`. They
//! cross-reference via `super::types` / `super::message_header`, so they must be
//! declared as sibling submodules of one parent `mod sbe` (a single
//! `include!(mod.rs)` would not resolve the `pub mod` paths against OUT_DIR).

#[allow(dead_code, non_camel_case_types, unused_imports, clippy::all)]
mod sbe {
    pub mod types {
        include!(concat!(env!("OUT_DIR"), "/sbe/types.rs"));
    }
    pub mod message_header {
        include!(concat!(env!("OUT_DIR"), "/sbe/message_header.rs"));
    }
    pub mod journal_record {
        include!(concat!(env!("OUT_DIR"), "/sbe/journal_record.rs"));
    }
    pub use message_header::MessageHeader;
}

use sbe::journal_record::{self, JournalRecord as SbeRecord};
use sbe::types::EventType;
use serialization_common::{Checksum, JournalRecord};

/// Encode a full framed message (header + body) into `buf`.
pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize {
    let header = sbe::MessageHeader {
        block_length: zerocopy::byteorder::little_endian::U16::new(SbeRecord::BLOCK_LENGTH),
        template_id: zerocopy::byteorder::little_endian::U16::new(SbeRecord::TEMPLATE_ID),
        schema_id: zerocopy::byteorder::little_endian::U16::new(SbeRecord::SCHEMA_ID),
        version: zerocopy::byteorder::little_endian::U16::new(SbeRecord::SCHEMA_VERSION),
    };
    SbeRecord::encode_with_header_into(buf, header, |enc| {
        write_fields(r, enc);
        Ok(())
    })
    .expect("sbe_gen encode")
}

/// Encode only the SBE body (no header) — used by the byte-identity test, since
/// header framing depends on tool-specific schema-id attribute handling.
pub fn encode_body(r: &JournalRecord, buf: &mut [u8]) -> usize {
    SbeRecord::encode_body_into(buf, |enc| {
        write_fields(r, enc);
        Ok(())
    })
    .expect("sbe_gen encode body")
}

fn write_fields(
    r: &JournalRecord,
    enc: &mut journal_record::JournalRecordEncoder<'_>,
) {
    enc.leadership_term_id(r.leadership_term_id)
        .log_position(r.log_position)
        .timestamp(r.timestamp)
        .cluster_session_id(r.cluster_session_id)
        .correlation_id(r.correlation_id)
        .leader_member_id(r.leader_member_id)
        .service_id(r.service_id)
        .event_type(EventType(r.event_type))
        .flags(r.flags);
    enc.entries(|g| {
        for e in &r.entries {
            g.entry(|ee| {
                ee.entry_term_id(e.entry_term_id)
                    .entry_index(e.entry_index)
                    .entry_timestamp(e.entry_timestamp)
                    .command_key(e.command_key);
                ee.command(&e.command)?;
                Ok(())
            })?;
        }
        Ok(())
    })
    .expect("sbe_gen entries");
}

/// Decode the framed message and fold every field (full materialization).
pub fn decode_checksum(bytes: &[u8]) -> u64 {
    // Skip the 8-byte SBE message header to reach the body.
    let body = &bytes[8..];
    let (rec, rest) = SbeRecord::parse_prefix(body).expect("sbe_gen header/body");
    let mut c = Checksum::new();
    c.add_i64(rec.leadership_term_id.get());
    c.add_i64(rec.log_position.get());
    c.add_i64(rec.timestamp.get());
    c.add_i64(rec.cluster_session_id.get());
    c.add_i64(rec.correlation_id.get());
    c.add_i32(rec.leader_member_id.get());
    c.add_i32(rec.service_id.get());
    c.add_u8(rec.event_type.0);
    c.add_u8(rec.flags);
    let group = journal_record::parse_entries(rest).expect("sbe_gen entries parse");
    let mut it = group.iter();
    while let Some(entry) = it.next() {
        c.add_i64(entry.entry_term_id().map(|v| v.get()).unwrap_or(0));
        c.add_i64(entry.entry_index().map(|v| v.get()).unwrap_or(0));
        c.add_i64(entry.entry_timestamp().map(|v| v.get()).unwrap_or(0));
        c.add_i32(entry.command_key().map(|v| v.get()).unwrap_or(0));
        c.add_bytes(entry.command.bytes);
    }
    c.finish()
}
```

Note on execution: the generated `EventType` is a newtype `struct EventType(pub u8)` (confirmed from generated `types.rs`), so encode uses `EventType(r.event_type)` and decode reads `rec.event_type.0`. The round-trip test pins correctness if a generated identifier differs.

- [ ] **Step 8: Run to verify it passes**

Run: `cd rust && cargo test -p serialization-sbe_gen`
Expected: PASS. If a generated identifier differs from the excerpt, fix the reference until the round-trip test is green.

- [ ] **Step 9: Write `src/main.rs`**

```rust
//! serialization **sbe_gen** experiment binary.

use bench_common::serial::{run_journal, CountingAllocator, SerialConfig};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

const EXPERIMENT: &str = "sbe_gen";

fn main() {
    let cfg = match SerialConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("serialization-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let (entries, cmd) = (cfg.entries, cfg.cmd_bytes);
    run_journal(
        EXPERIMENT,
        &cfg,
        |i| serialization_common::build_record(i, entries, cmd),
        serialization_sbe_gen::encode,
        serialization_sbe_gen::decode_checksum,
    );
}
```

- [ ] **Step 10: Build + smoke-run**

Run: `cd rust && SER_WARMUP=10 SER_ITERS=100 cargo run -q -p serialization-sbe_gen`
Expected: four JSON lines with `"experiment":"sbe_gen"`; `decode_alloc_bytes` should be `0` (zero-copy decode) — the headline contrast with bincode.

- [ ] **Step 11: Commit**

```bash
git add rust/Cargo.toml rust/Cargo.lock rust/serialization/sbe_gen
git commit -m "feat(serialization): sbe_gen (zerocopy SBE) codec cell + bench binary"
```

---

### Task 5: `serialization-aeron_sbe` cell (vendored jar → committed generated crate)

**Files:**
- Create: `rust/serialization/aeron_sbe/Cargo.toml`
- Create: `rust/serialization/aeron_sbe/schema/journal.xml` (copy of Task 4 schema)
- Create: `rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar` (copied from the Gradle cache)
- Create: `rust/serialization/aeron_sbe/regen.sh` (regenerates the vendored crate)
- Create: `rust/serialization/aeron_sbe/generated/**` (committed generator output)
- Create: `rust/serialization/aeron_sbe/src/lib.rs`
- Create: `rust/serialization/aeron_sbe/src/main.rs`
- Modify: `rust/Cargo.toml` (`members`)

**Interfaces:**
- Consumes: the generated crate `journal` (real-logic Rust: `JournalRecordEncoder`/`JournalRecordDecoder`, `ReadBuf`/`WriteBuf`), `serialization_common::{JournalRecord, Checksum}`.
- Produces (library): `pub fn encode(r, buf) -> usize`, `pub fn encode_body(r, buf) -> usize`, `pub fn decode_checksum(bytes) -> u64`.

Rationale for committing generated output: the real-logic tool emits a whole crate with crate-root inner attributes (`#![forbid(unsafe_code)]`), which cannot be `include!`d; a build.rs → OUT_DIR path dependency is impossible because Cargo resolves path deps before build.rs runs. So the generator output is a committed path-dependency crate, refreshed by `regen.sh`. The jar is vendored so regeneration is hermetic.

- [ ] **Step 1: Add workspace member and vendor the jar + schema**

```bash
mkdir -p rust/serialization/aeron_sbe/vendor rust/serialization/aeron_sbe/schema
cp /home/claude/.gradle/caches/modules-2/files-2.1/uk.co.real-logic/sbe-all/1.38.1/ef7dd43a54a0269854ac2a296c2f6ba25edbaeff/sbe-all-1.38.1.jar rust/serialization/aeron_sbe/vendor/
cp rust/serialization/sbe_gen/schema/journal.xml rust/serialization/aeron_sbe/schema/journal.xml
```

Add `"serialization/aeron_sbe"` and `"serialization/aeron_sbe/generated/journal"` to `members` in `rust/Cargo.toml` (the generated crate is a workspace member so it shares the lockfile and lints).

- [ ] **Step 2: Write `regen.sh`**

```sh
#!/usr/bin/env sh
# Regenerate the vendored real-logic SBE Rust crate from schema/journal.xml.
# Requires a JDK (only for regeneration; normal builds use the committed output).
set -eu
here=$(dirname "$0")
jar="$here/vendor/sbe-all-1.38.1.jar"
out="$here/generated"
rm -rf "$out"
mkdir -p "$out"
java -Dsbe.target.language=Rust -Dsbe.output.dir="$out" -jar "$jar" "$here/schema/journal.xml"
echo "regenerated $out/journal" 1>&2
```

- [ ] **Step 3: Generate the crate**

Run: `sh rust/serialization/aeron_sbe/regen.sh && ls rust/serialization/aeron_sbe/generated/journal/src`
Expected: `lib.rs journal_record_codec.rs message_header_codec.rs group_size_encoding_codec.rs var_data_encoding_codec.rs event_type.rs`.

- [ ] **Step 4: Make the generated crate a workspace member**

Overwrite `rust/serialization/aeron_sbe/generated/journal/Cargo.toml` so it inherits workspace fields and does not fight the release profile:

```toml
[package]
name = "journal-aeron-sbe"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
name = "journal"
path = "src/lib.rs"
```

Note: if the generator's `Cargo.toml` sets `edition = "2021"`, keep the workspace edition (2024) — the generated code is edition-agnostic Rust. If a compile error surfaces from the edition bump, pin `edition = "2021"` here instead (isolated to this generated crate).

- [ ] **Step 5: Write the cell `Cargo.toml`**

```toml
[package]
name = "serialization-aeron_sbe"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "serialization-aeron_sbe"
path = "src/main.rs"

[lib]
name = "serialization_aeron_sbe"
path = "src/lib.rs"

[dependencies]
serialization-common = { path = "../common" }
bench-common = { path = "../../bench-common" }
journal = { package = "journal-aeron-sbe", path = "generated/journal" }
```

- [ ] **Step 6: Write the failing round-trip test in `src/lib.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serialization_common::{build_record, checksum_record};

    #[test]
    fn round_trip_checksum_matches() {
        let r = build_record(9, 4, 78);
        let mut buf = vec![0u8; 64 * 1024];
        let n = encode(&r, &mut buf);
        assert!(n > 400 && n < 700, "unexpected encoded size {n}");
        assert_eq!(decode_checksum(&buf[..n]), checksum_record(&r));
    }
}
```

- [ ] **Step 7: Run to verify it fails**

Run: `cd rust && cargo test -p serialization-aeron_sbe`
Expected: FAIL — `cannot find function encode`.

- [ ] **Step 8: Implement `src/lib.rs` using the real-logic flyweight API**

The real-logic API (verified from generation): encoders/decoders wrap a `WriteBuf`/`ReadBuf`; groups via `entries_encoder(count, EntriesEncoder::default())` + `advance()`, decode via `entries_decoder()` + `count()`/`advance()` + `command_decoder()`/`command_slice()`. Header via `MessageHeaderEncoder`. Field/method names match the schema (snake_case).

```rust
//! aeron_sbe (real-logic SBE tool → Rust) codec cell.

use journal::journal_record_codec::{
    EntriesDecoder, EntriesEncoder, JournalRecordDecoder, JournalRecordEncoder,
};
use journal::message_header_codec::{MessageHeaderDecoder, MessageHeaderEncoder};
use journal::{ReadBuf, WriteBuf, SBE_SCHEMA_VERSION};
use serialization_common::{Checksum, JournalRecord};

const HEADER_LEN: usize = 8;
const BLOCK_LENGTH: u16 = 50;

/// Encode header + body into `buf`, returning total framed length.
pub fn encode(r: &JournalRecord, buf: &mut [u8]) -> usize {
    // Header.
    let mut header = MessageHeaderEncoder::default().wrap(WriteBuf::new(buf), 0);
    header.block_length(BLOCK_LENGTH);
    header.template_id(1);
    header.schema_id(journal::SBE_SCHEMA_ID);
    header.version(SBE_SCHEMA_VERSION);
    HEADER_LEN + encode_body_at(r, buf, HEADER_LEN)
}

/// Encode only the SBE body starting at offset 0 (byte-identity comparison).
pub fn encode_body(r: &JournalRecord, buf: &mut [u8]) -> usize {
    encode_body_at(r, buf, 0)
}

fn encode_body_at(r: &JournalRecord, buf: &mut [u8], offset: usize) -> usize {
    let mut enc = JournalRecordEncoder::default().wrap(WriteBuf::new(buf), offset);
    enc.leadership_term_id(r.leadership_term_id);
    enc.log_position(r.log_position);
    enc.timestamp(r.timestamp);
    enc.cluster_session_id(r.cluster_session_id);
    enc.correlation_id(r.correlation_id);
    enc.leader_member_id(r.leader_member_id);
    enc.service_id(r.service_id);
    enc.event_type(journal::event_type::EventType::from(r.event_type));
    enc.flags(r.flags);
    let mut group = enc.entries_encoder(r.entries.len() as u16, EntriesEncoder::default());
    for e in &r.entries {
        group.advance().expect("advance");
        group.entry_term_id(e.entry_term_id);
        group.entry_index(e.entry_index);
        group.entry_timestamp(e.entry_timestamp);
        group.command_key(e.command_key);
        group.command(&e.command);
    }
    // Encoded length = current encoder position minus the starting offset.
    group.get_limit() - offset
}

/// Decode header + body and fold every field.
pub fn decode_checksum(bytes: &[u8]) -> u64 {
    let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(bytes), 0);
    let block_length = header.block_length();
    let version = header.version();
    let mut dec = JournalRecordDecoder::default().header(header, 0);
    let _ = (block_length, version);

    let mut c = Checksum::new();
    c.add_i64(dec.leadership_term_id());
    c.add_i64(dec.log_position());
    c.add_i64(dec.timestamp());
    c.add_i64(dec.cluster_session_id());
    c.add_i64(dec.correlation_id());
    c.add_i32(dec.leader_member_id());
    c.add_i32(dec.service_id());
    c.add_u8(u8::from(dec.event_type()));
    c.add_u8(dec.flags());
    let mut group: EntriesDecoder<_> = dec.entries_decoder();
    let count = group.count();
    for _ in 0..count {
        group.advance().expect("advance").expect("entry present");
        c.add_i64(group.entry_term_id());
        c.add_i64(group.entry_index());
        c.add_i64(group.entry_timestamp());
        c.add_i32(group.command_key());
        let coords = group.command_decoder();
        c.add_bytes(group.command_slice(coords));
    }
    c.finish()
}
```

Note on execution: the real-logic Rust accessor set is stable, but exact helper names (`get_limit`, `header(...)`, `command_slice`) can vary slightly by generator version. The generated `journal_record_codec.rs` in `generated/journal/src/` is the source of truth — read it and adjust method calls until the round-trip test passes. Do not change the checksum field order.

- [ ] **Step 9: Run to verify it passes**

Run: `cd rust && cargo test -p serialization-aeron_sbe`
Expected: PASS (fixing generated-method-name references as needed against `generated/journal/src/journal_record_codec.rs`).

- [ ] **Step 10: Write `src/main.rs`**

```rust
//! serialization **aeron_sbe** experiment binary.

use bench_common::serial::{run_journal, CountingAllocator, SerialConfig};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

const EXPERIMENT: &str = "aeron_sbe";

fn main() {
    let cfg = match SerialConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("serialization-{EXPERIMENT}: {msg}");
            std::process::exit(1);
        }
    };
    let (entries, cmd) = (cfg.entries, cfg.cmd_bytes);
    run_journal(
        EXPERIMENT,
        &cfg,
        |i| serialization_common::build_record(i, entries, cmd),
        serialization_aeron_sbe::encode,
        serialization_aeron_sbe::decode_checksum,
    );
}
```

- [ ] **Step 11: Build + smoke-run**

Run: `cd rust && SER_WARMUP=10 SER_ITERS=100 cargo run -q -p serialization-aeron_sbe`
Expected: four JSON lines with `"experiment":"aeron_sbe"`; `decode_alloc_bytes` should be `0` (zero-copy flyweight decode).

- [ ] **Step 12: Commit**

```bash
git add rust/Cargo.toml rust/Cargo.lock rust/serialization/aeron_sbe
git commit -m "feat(serialization): aeron_sbe (real-logic SBE tool) codec cell + vendored generator"
```

---

### Task 6: `serialization-conformance` — cross-codec byte-identity + field equality

**Files:**
- Create: `rust/serialization/conformance/Cargo.toml`
- Create: `rust/serialization/conformance/tests/conformance.rs`
- Modify: `rust/Cargo.toml` (`members`)

**Interfaces:**
- Consumes: `serialization_sbe_gen`, `serialization_aeron_sbe`, `serialization_bincode` (all three cell libs), `serialization_common::{build_record, checksum_record}`.
- Produces: no library API — integration tests only.

- [ ] **Step 1: Add workspace member**

Add `"serialization/conformance"` to `members`.

- [ ] **Step 2: Write `Cargo.toml`**

```toml
[package]
name = "serialization-conformance"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
publish = false

[dependencies]

[dev-dependencies]
serialization-common = { path = "../common" }
serialization-bincode = { path = "../bincode" }
serialization-sbe_gen = { path = "../sbe_gen" }
serialization-aeron_sbe = { path = "../aeron_sbe" }
```

Also create an empty `src/lib.rs` (`// conformance: tests only`) so the package is valid.

- [ ] **Step 3: Write the tests**

```rust
//! Cross-codec conformance: all three codecs materialize the same record to the
//! same checksum, and the two SBE codecs produce byte-identical SBE bodies.

use serialization_common::{build_record, checksum_record};

fn scratch() -> Vec<u8> {
    vec![0u8; 64 * 1024]
}

#[test]
fn all_codecs_agree_on_checksum() {
    for i in 0..64u64 {
        let r = build_record(i, 4, 78);
        let want = checksum_record(&r);

        let mut b = scratch();
        let n = serialization_bincode::encode(&r, &mut b);
        assert_eq!(serialization_bincode::decode_checksum(&b[..n]), want, "bincode i={i}");

        let mut s = scratch();
        let n = serialization_sbe_gen::encode(&r, &mut s);
        assert_eq!(serialization_sbe_gen::decode_checksum(&s[..n]), want, "sbe_gen i={i}");

        let mut a = scratch();
        let n = serialization_aeron_sbe::encode(&r, &mut a);
        assert_eq!(serialization_aeron_sbe::decode_checksum(&a[..n]), want, "aeron_sbe i={i}");
    }
}

#[test]
fn sbe_bodies_are_byte_identical() {
    // Both SBE toolchains implement the same wire spec, so the encoded BODY
    // (fixed block + group + var-data, excluding the header frame) must match
    // byte-for-byte for the same record.
    for i in 0..64u64 {
        let r = build_record(i, 4, 78);
        let mut a = scratch();
        let mut b = scratch();
        let na = serialization_sbe_gen::encode_body(&r, &mut a);
        let nb = serialization_aeron_sbe::encode_body(&r, &mut b);
        assert_eq!(&a[..na], &b[..nb], "SBE body mismatch at i={i}");
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cd rust && cargo test -p serialization-conformance`
Expected: PASS. If `sbe_bodies_are_byte_identical` fails, diff the two byte slices — a mismatch means a schema/blockLength disagreement between the tools, fixable in `schema/journal.xml` (keep both copies identical).

- [ ] **Step 5: Commit**

```bash
git add rust/Cargo.toml rust/serialization/conformance
git commit -m "test(serialization): cross-codec checksum + byte-identity conformance"
```

---

### Task 7: Wire the focus area into bench-infra and docs

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml` (`experiments` matrix)
- Modify: `docs/result-contract.md` (mention the new focus area/experiments)
- Modify: `CLAUDE.md` (status line + artifact-names line)
- Modify: `README.md` (status section, if it enumerates focus areas)

**Interfaces:** none (config + docs).

- [ ] **Step 1: Add three rows to the `experiments` matrix**

Rows use the `{ focus_area, experiment, kind }` schema; `serialization` is single-host, so `kind: local`. Append after the `thread-handoff` rows:

```yaml
  - { focus_area: serialization,    experiment: sbe_gen,     kind: local }
  - { focus_area: serialization,    experiment: aeron_sbe,   kind: local }
  - { focus_area: serialization,    experiment: bincode,     kind: local }
```

- [ ] **Step 2: Add a `serialization` params block**

After the `thread-handoff params` block, mirror the existing param-block style so all three codecs run with identical parameters (the `serialization` cells are Rust-only, but the block keeps run config in one place):

```yaml
# --- serialization params (single-host, node0). Rust-only focus area: three
#     codecs (sbe_gen, aeron_sbe, bincode) over one shared ~500-byte journal
#     record. aeron_sbe ships a committed generated crate, so NO JDK is needed
#     at bench time (only regen.sh regenerates it). ---
ser_warmup: 1000
ser_iterations: 100000
ser_entries: 4
ser_cmd_bytes: 78
```

Then inspect the playbook that exports params into runs (`grep -rn "th_warmup\|fsw_warmup" bench-infra/ansible`) and add the parallel `SER_WARMUP`/`SER_ITERS`/`SER_ENTRIES`/`SER_CMD_BYTES` env exports for `serialization` rows, following exactly how `thread-handoff` maps `th_*` → `TH_*`.

- [ ] **Step 3: Update `docs/result-contract.md`**

In the focus-area enumeration, add: `serialization` implemented for `sbe_gen`, `aeron_sbe`, `bincode` (Rust only); metrics `encode_ns`, `decode_ns` (ns), `encoded_bytes`, `decode_alloc_bytes` (bytes).

- [ ] **Step 4: Update `CLAUDE.md`**

- Add `serialization-{sbe_gen,aeron_sbe,bincode}` to the artifact-names line.
- Update the Status paragraph: `serialization` implemented in Rust (three codecs, single-host); Go/Java not planned for this focus area.
- Add the build/run example: `cargo run --release -p serialization-bincode` (and note `-p serialization-sbe_gen | -p serialization-aeron_sbe`).

- [ ] **Step 5: Commit**

```bash
git add bench-infra/ansible/group_vars/all.yml docs/result-contract.md CLAUDE.md README.md
git commit -m "docs(serialization): bench-infra matrix rows + contract/status updates"
```

---

### Task 8: Workspace-wide verification

**Files:** none (verification only).

- [ ] **Step 1: Full build + test**

Run: `cd rust && cargo build --release && cargo test`
Expected: all crates build; all tests pass (common, bench-common serial, three cells, conformance).

- [ ] **Step 2: Lints + formatting**

Run: `cd rust && cargo clippy --all-targets && cargo fmt --check`
Expected: no warnings, no diffs. Fix any clippy findings in the new code (the generated crates are `#![allow(clippy::all)]` / covered by the `mod sbe { #![allow(clippy::all)] }` wrapper, so lints target only hand-written code).

- [ ] **Step 3: Contract sanity across all three binaries**

Run:
```bash
cd rust
for e in bincode sbe_gen aeron_sbe; do
  SER_WARMUP=100 SER_ITERS=2000 cargo run -q --release -p serialization-$e
done
```
Expected: 12 JSON lines total (4 per codec). Verify by eye: every line has `"focus_area":"serialization"`, correct `experiment`, and that `decode_alloc_bytes` is `0` for both SBE codecs and clearly `> 0` for `bincode` — the memory story the focus area is built to show. Encode/decode `ns` are positive.

- [ ] **Step 4: Final commit (if any fmt/clippy fixes were needed)**

```bash
git add -A rust
git commit -m "chore(serialization): clippy/fmt cleanup; workspace green"
```

---

## Notes for the implementer

- **Read the generated code, not just this plan.** For `sbe_gen`, the source of truth is `OUT_DIR/sbe/journal_record.rs` (visible after the first build under `target/`); for `aeron_sbe`, it is `rust/serialization/aeron_sbe/generated/journal/src/journal_record_codec.rs`. The API excerpts here were captured from real generation of this exact schema, but if a method name differs by version, the generated file wins — adjust the wrapper and let the round-trip test gate correctness.
- **Never change the checksum field order** across cells — the conformance test depends on all codecs folding identically.
- **Keep the two `schema/journal.xml` copies identical** (sbe_gen cell + aeron_sbe cell). The byte-identity test enforces that they encode the same body; if you edit one, edit both.
- **stdout hygiene:** if you add debugging, use `eprintln!`. A stray `println!` breaks the result contract.
