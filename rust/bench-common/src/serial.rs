//! Shared harness for the `serialization` focus area: env config, a counting
//! global allocator (deterministic decode-time memory measurement), and the
//! journal write/replay timed loop that emits the four result-contract metrics.
//!
//! stdout stays result-only; this module prints nothing but the emit lines.

#[cfg(test)]
#[global_allocator]
static TEST_ALLOC: CountingAllocator = CountingAllocator;

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use crate::{result, stats};

const FOCUS: &str = "serialization";

/// Env-configurable knobs. `SER_ENTRIES` tunes the entry count; `SER_CMD_BYTES`
/// sizes the `cmd_text` string on each entry (default 12). The record is
/// field-heavy, dominated by the typed scalar fields rather than cmd_text.
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
            cmd_bytes: parse_env("SER_CMD_BYTES", 12)?,
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

// SAFETY: every method delegates to `System`, which is itself a valid
// `GlobalAlloc` implementation; this wrapper only adds a non-mutating
// bookkeeping side effect (an atomic counter update) around each call and
// does not alter the pointers, layouts, or safety contract passed through.
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
/// Emits, for both `encode` and `decode`: `<op>_p50`, `<op>_p99` (integer ns)
/// and `<op>_mean` (fractional ns) — mirroring the `network-rtt`
/// `rtt_{p50,p99,mean}` convention — plus `encoded_bytes` (one record) and
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

    emit_latency(experiment, "encode", &encode_ns);
    emit_latency(experiment, "decode", &decode_ns);
    result::emit(
        FOCUS,
        experiment,
        "encoded_bytes",
        record_len as u64,
        "bytes",
        1,
    );
    result::emit(
        FOCUS,
        experiment,
        "decode_alloc_bytes",
        decode_alloc_per as u64,
        "bytes",
        n,
    );
}

/// p50/p99 (integer ns) and mean (fractional ns) over `samples`. Sorts a copy
/// (percentiles need sorted input) and delegates to the shared `stats` module;
/// pure so it can be unit-tested without capturing stdout.
fn latency_stats(samples: &[u64]) -> (u64, u64, f64) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p50 = stats::percentile(&sorted, 50.0);
    let p99 = stats::percentile(&sorted, 99.0);
    (p50, p99, stats::mean(samples))
}

/// Emit `<op>_p50`, `<op>_p99` (ns) and `<op>_mean` (ns) for one operation's
/// per-iteration latency samples.
fn emit_latency(experiment: &str, op: &str, samples: &[u64]) {
    let n = samples.len();
    let (p50, p99, mean) = latency_stats(samples);
    result::emit(FOCUS, experiment, &format!("{op}_p50"), p50, "ns", n);
    result::emit(FOCUS, experiment, &format!("{op}_p99"), p99, "ns", n);
    result::emit_float(FOCUS, experiment, &format!("{op}_mean"), mean, "ns", n);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_from_env() {
        // No SER_* vars set in the test environment → from_env yields the
        // field-heavy record defaults. (edition 2024 makes set_var unsafe, so
        // we test the default path rather than mutating the environment.)
        let cfg = SerialConfig::from_env().expect("defaults");
        assert_eq!(cfg.warmup, 1000);
        assert_eq!(cfg.iterations, 100_000);
        assert_eq!(cfg.entries, 4);
        assert_eq!(cfg.cmd_bytes, 12);
    }

    #[test]
    fn counting_allocator_tracks_a_vec() {
        reset_allocated();
        let before = allocated_bytes();
        let v: Vec<u8> = Vec::with_capacity(4096);
        assert!(allocated_bytes() >= before + 4096);
        drop(v);
    }

    #[test]
    fn latency_stats_sorts_before_percentiles() {
        // Unsorted input must yield the same result as the sorted slice through
        // `stats` — this is the only logic `latency_stats` adds over `stats`.
        let unsorted = vec![50u64, 10, 99, 1, 30, 70, 20, 90, 40, 60];
        let (p50, p99, mean) = latency_stats(&unsorted);

        let mut sorted = unsorted.clone();
        sorted.sort_unstable();
        assert_eq!(p50, stats::percentile(&sorted, 50.0));
        assert_eq!(p99, stats::percentile(&sorted, 99.0));
        assert_eq!(mean, stats::mean(&unsorted));
        assert!(p50 <= p99, "p50 {p50} must not exceed p99 {p99}");
    }
}
