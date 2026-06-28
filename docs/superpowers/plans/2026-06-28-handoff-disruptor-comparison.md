# Handoff vs disruptor-rs Comparison — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a rust-only criterion benchmark crate comparing our hand-rolled SPSC/MPSC handoff rings against disruptor-rs (and crossbeam, std-mpsc) across burst sizes × pauses.

**Architecture:** A new self-contained `rust/handoff-compare` crate (lib + two criterion benches). The lib holds our optimized SPSC ring (mirrored from `thread-handoff/ring`, plus a batch API) and a new lock-free multi-producer (MPSC) ring. The benches mirror disruptor-rs's own `benches/{spsc,mpsc}.rs` methodology so numbers are directly comparable. Shipped `thread-handoff-*` cells are untouched.

**Tech Stack:** Rust 1.96 (edition 2024 workspace). Bench/dev-deps only: `disruptor 4.3`, `criterion 0.5`, `crossbeam 0.8`, `core_affinity 0.8`. The lib itself is std-only.

## Global Constraints

- New crate `rust/handoff-compare`, added to the rust workspace `members`. The lib is **std-only**; `disruptor`/`criterion`/`crossbeam`/`core_affinity` are **`[dev-dependencies]`** (bench-only) — the shipped `thread-handoff` cells stay std-only and are NOT modified.
- The crate is **excluded** from the autobench task registry and the bench-infra matrix (rust-only study; no result-contract coupling).
- Matrix mirrors disruptor-rs: **burst ∈ [1, 10, 100] × pause ∈ [0, 1, 10] ms**, plus a `base` overhead row. SPSC `cap=128`; MPSC `cap=256`, 2 producers.
- Busy-wait wait-strategy everywhere (our rings use `std::hint::spin_loop()`; disruptor uses `BusySpin`) for a fair comparison. Threads pinned with `core_affinity` (skip gracefully if unavailable).
- Ring correctness is the comparability-critical code: SPSC order+count (single/batch/wrap), MPSC multi-producer no-loss/no-dup/exact-count stress. Keep the crate clippy- and rustfmt-clean (`cargo clippy --all-targets`, `cargo fmt --check` within the crate).
- All cargo commands run from `rust/` (the workspace). Bursts are always `<= cap`.

---

## Task 1: Scaffold crate + SPSC ring (mirror + batch API) + tests

**Files:**
- Create: `rust/handoff-compare/Cargo.toml`
- Create: `rust/handoff-compare/src/lib.rs`
- Create: `rust/handoff-compare/src/spsc.rs`
- Modify: `rust/Cargo.toml` (workspace members)

**Interfaces:**
- Produces: `handoff_compare::spsc::channel(cap: usize) -> (Producer, Consumer)`;
  `Producer::{push(&mut self, u64), batch_publish(&mut self, n: usize, fill: impl Fn(usize)->u64), consumed(&self)->usize}`;
  `Consumer::{pop(&mut self)->u64, drain(&mut self, max: usize, f: impl FnMut(u64))->usize}`.

- [ ] **Step 1: Add the workspace member** — in `rust/Cargo.toml`, add to `members` (after `"thread-handoff/disruptor",`):

```toml
    "handoff-compare",
```

- [ ] **Step 2: Create** `rust/handoff-compare/Cargo.toml`:

```toml
[package]
name = "handoff-compare"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

# Library is std-only. disruptor/criterion/crossbeam/core_affinity are
# bench-only dev-dependencies — the shipped thread-handoff cells stay std-only.
[dev-dependencies]
disruptor = "4.3"
criterion = "0.5"
crossbeam = "0.8"
core_affinity = "0.8"

[[bench]]
name = "spsc"
harness = false

[[bench]]
name = "mpsc"
harness = false
```

- [ ] **Step 3: Create** `rust/handoff-compare/src/lib.rs`:

```rust
//! Benchmark-only comparison of our hand-rolled SPSC/MPSC handoff rings against
//! the `disruptor` crate (and crossbeam / std-mpsc references). NOT part of the
//! cross-language result-contract grid; the library is std-only.

pub mod mpsc;
pub mod spsc;
```

- [ ] **Step 4: Create** `rust/handoff-compare/src/spsc.rs` — the optimized SPSC ring mirrored verbatim from `rust/thread-handoff/ring/src/spsc.rs`, with `batch_publish`/`drain` added and batch tests:

```rust
//! Bounded single-producer single-consumer ring of `u64` tokens, busy-wait.
//!
//! Mirrored verbatim from `rust/thread-handoff/ring/src/spsc.rs` (the shipped,
//! AWS-validated optimized ring is the source of truth) — a pinned snapshot for
//! this comparison study — plus a batch `batch_publish`/`drain` API for bursts.
//!
//! Layout: `head` (consumer) and `tail` (producer) on separate 64-byte cache
//! lines; each side caches the opposite index and only re-loads the contended
//! atomic when the ring appears full/empty.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[repr(align(64))]
struct CacheLine(AtomicUsize);

struct Spsc {
    buf: Box<[AtomicU64]>,
    cap: usize,
    head: CacheLine, // total popped (consumer writes)
    tail: CacheLine, // total pushed (producer writes)
}

impl Spsc {
    fn new(cap: usize) -> Self {
        assert!(cap > 0, "ring capacity must be positive");
        let buf = (0..cap)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Spsc {
            buf,
            cap,
            head: CacheLine(AtomicUsize::new(0)),
            tail: CacheLine(AtomicUsize::new(0)),
        }
    }
}

/// Create a bounded SPSC ring and return its single producer / consumer handles.
pub fn channel(cap: usize) -> (Producer, Consumer) {
    let shared = Arc::new(Spsc::new(cap));
    let producer = Producer {
        shared: Arc::clone(&shared),
        tail: 0,
        cached_head: 0,
    };
    let consumer = Consumer {
        shared,
        head: 0,
        cached_tail: 0,
    };
    (producer, consumer)
}

/// The single producer. Owns `tail` and a cached snapshot of the consumer head.
pub struct Producer {
    shared: Arc<Spsc>,
    tail: usize,
    cached_head: usize,
}

impl Producer {
    /// Push one token, busy-waiting while the ring is full.
    pub fn push(&mut self, v: u64) {
        let shared = &*self.shared;
        if self.tail - self.cached_head == shared.cap {
            loop {
                self.cached_head = shared.head.0.load(Ordering::Acquire);
                if self.tail - self.cached_head < shared.cap {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        shared.buf[self.tail % shared.cap].store(v, Ordering::Relaxed);
        self.tail += 1;
        shared.tail.0.store(self.tail, Ordering::Release);
    }

    /// Reserve `n` contiguous slots (busy-waiting for space), fill them via
    /// `fill(k)` for k in 0..n, then publish all `n` with a single Release. One
    /// barrier per burst instead of per element. `n` must be `<= cap`.
    pub fn batch_publish<F: Fn(usize) -> u64>(&mut self, n: usize, fill: F) {
        let shared = &*self.shared;
        debug_assert!(n <= shared.cap, "burst exceeds ring capacity");
        // Need `n` free slots: outstanding (tail-head) + n <= cap.
        if self.tail + n - self.cached_head > shared.cap {
            loop {
                self.cached_head = shared.head.0.load(Ordering::Acquire);
                if self.tail + n - self.cached_head <= shared.cap {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        for k in 0..n {
            shared.buf[(self.tail + k) % shared.cap].store(fill(k), Ordering::Relaxed);
        }
        self.tail += n;
        shared.tail.0.store(self.tail, Ordering::Release);
    }

    /// Total tokens popped so far (reads the real shared head).
    pub fn consumed(&self) -> usize {
        self.shared.head.0.load(Ordering::Acquire)
    }
}

/// The single consumer. Owns `head` and a cached snapshot of the producer tail.
pub struct Consumer {
    shared: Arc<Spsc>,
    head: usize,
    cached_tail: usize,
}

impl Consumer {
    /// Pop one token, busy-waiting while the ring is empty.
    pub fn pop(&mut self) -> u64 {
        let shared = &*self.shared;
        if self.head == self.cached_tail {
            loop {
                self.cached_tail = shared.tail.0.load(Ordering::Acquire);
                if self.head != self.cached_tail {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        let v = shared.buf[self.head % shared.cap].load(Ordering::Relaxed);
        self.head += 1;
        shared.head.0.store(self.head, Ordering::Release);
        v
    }

    /// Drain up to `max` currently-available tokens, calling `f` per token;
    /// advance `head` once at the end. Returns the number drained (0 if empty).
    /// Non-blocking: refreshes the cached tail only when it appears empty.
    pub fn drain<F: FnMut(u64)>(&mut self, max: usize, mut f: F) -> usize {
        let shared = &*self.shared;
        if self.head == self.cached_tail {
            self.cached_tail = shared.tail.0.load(Ordering::Acquire);
        }
        let take = (self.cached_tail - self.head).min(max);
        for k in 0..take {
            f(shared.buf[(self.head + k) % shared.cap].load(Ordering::Relaxed));
        }
        self.head += take;
        if take > 0 {
            shared.head.0.store(self.head, Ordering::Release);
        }
        take
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn spsc_preserves_order_and_count() {
        let n = 100_000usize;
        let (mut prod, mut cons) = channel(64);
        let consumer = thread::spawn(move || {
            let mut got = Vec::with_capacity(n);
            for _ in 0..n {
                got.push(cons.pop());
            }
            got
        });
        for i in 0..n {
            prod.push(i as u64);
        }
        let got = consumer.join().unwrap();
        assert_eq!(got.len(), n);
        for (i, v) in got.iter().enumerate() {
            assert_eq!(*v, i as u64, "token {i} out of order");
        }
        assert_eq!(prod.consumed(), n);
    }

    #[test]
    fn spsc_batch_preserves_order_and_count_with_wrap() {
        // n far exceeds cap (wrap-around); burst is non-divisible vs cap and n.
        let n = 100_000usize;
        let burst = 7usize;
        let (mut prod, mut cons) = channel(64);
        let consumer = thread::spawn(move || {
            let mut got = Vec::with_capacity(n);
            while got.len() < n {
                cons.drain(usize::MAX, |v| got.push(v));
            }
            got
        });
        let mut sent = 0usize;
        while sent < n {
            let b = burst.min(n - sent);
            let base = sent;
            prod.batch_publish(b, |k| (base + k) as u64);
            sent += b;
        }
        let got = consumer.join().unwrap();
        assert_eq!(got.len(), n);
        for (i, v) in got.iter().enumerate() {
            assert_eq!(*v, i as u64, "batch token {i} out of order");
        }
    }
}
```

- [ ] **Step 5: Build + test + lint**

Run: `cd rust && cargo test -p handoff-compare && cargo clippy -p handoff-compare --all-targets 2>&1 | tail -3 && cargo fmt -p handoff-compare --check`
Expected: both `spsc_*` tests PASS; no clippy warnings; no fmt diff.

- [ ] **Step 6: Commit**

```bash
git add rust/Cargo.toml rust/handoff-compare/Cargo.toml rust/handoff-compare/src/lib.rs rust/handoff-compare/src/spsc.rs
git commit -m "handoff-compare: scaffold crate + SPSC ring (mirrored) with batch API + tests"
```

---

## Task 2: Lock-free multi-producer (MPSC) ring + stress tests

**Files:**
- Create: `rust/handoff-compare/src/mpsc.rs`

**Interfaces:**
- Consumes: nothing from Task 1 (independent module).
- Produces: `handoff_compare::mpsc::ring(cap: usize) -> (MpProducer, MpConsumer)`;
  `MpProducer: Clone`, `MpProducer::batch_publish(&mut self, n: usize, fill: impl Fn(usize)->u64)`;
  `MpConsumer::drain(&mut self, max: usize, f: impl FnMut(u64)) -> usize`.

- [ ] **Step 1: Write the failing stress test** — create `rust/handoff-compare/src/mpsc.rs` with the full module below (it includes its stress test):

```rust
//! Lock-free multi-producer single-consumer ring of `u64` tokens (the LMAX
//! Disruptor multi-producer algorithm), busy-wait. Producers claim a contiguous
//! range with `fetch_add` on a shared cursor, write their slots, then publish
//! via a per-slot **availability buffer** recording the round number; the single
//! consumer scans the contiguous published prefix. std-only.
//!
//! Invariant: every claimed sequence is delivered to the consumer exactly once,
//! in sequence order. Items from different producers interleave by claim order
//! (no per-producer global FIFO — same as disruptor multi-producer). No loss, no
//! duplication, no slot overwritten before it is consumed.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

#[repr(align(64))]
struct CacheLine(AtomicUsize);

struct Mpsc {
    buf: Box<[AtomicU64]>,
    /// Per-slot published round number (`seq / cap`); -1 = never published.
    avail: Box<[AtomicI64]>,
    cap: usize,
    claim: CacheLine, // next sequence to claim (producers fetch_add)
    head: CacheLine,  // consumer cursor (total consumed)
}

impl Mpsc {
    fn new(cap: usize) -> Self {
        assert!(
            cap > 0 && cap.is_power_of_two(),
            "cap must be a power of two"
        );
        let buf = (0..cap)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let avail = (0..cap)
            .map(|_| AtomicI64::new(-1))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Mpsc {
            buf,
            avail,
            cap,
            claim: CacheLine(AtomicUsize::new(0)),
            head: CacheLine(AtomicUsize::new(0)),
        }
    }
}

/// Create a bounded MPSC ring: one consumer, any number of (cloned) producers.
pub fn ring(cap: usize) -> (MpProducer, MpConsumer) {
    let shared = Arc::new(Mpsc::new(cap));
    (
        MpProducer {
            shared: Arc::clone(&shared),
            cached_head: 0,
        },
        MpConsumer { shared, head: 0 },
    )
}

/// A producer handle. `Clone` it once per producer thread; all clones share the
/// claim cursor and availability buffer. Each clone keeps its own cached head.
#[derive(Clone)]
pub struct MpProducer {
    shared: Arc<Mpsc>,
    cached_head: usize, // per-producer cached consumer head (backpressure)
}

impl MpProducer {
    /// Claim `n` contiguous slots, fill via `fill(k)` for k in 0..n, and publish
    /// each. `n` must be `<= cap`. Busy-waits for backpressure (the slots it will
    /// overwrite must already have been consumed).
    pub fn batch_publish<F: Fn(usize) -> u64>(&mut self, n: usize, fill: F) {
        let shared = &*self.shared;
        debug_assert!(n <= shared.cap, "burst exceeds ring capacity");
        // Claim a disjoint contiguous range [seq, seq+n) (Relaxed: ordering of
        // the data is established by the availability buffer, not this counter).
        let seq = shared.claim.0.fetch_add(n, Ordering::Relaxed);
        // Backpressure: the slot for the highest sequence (seq+n-1) aliases the
        // occupant of sequence (seq+n-1) - cap, which must be consumed first =>
        // wait until consumer head >= seq + n - cap.
        let need = (seq + n).saturating_sub(shared.cap);
        if self.cached_head < need {
            loop {
                self.cached_head = shared.head.0.load(Ordering::Acquire);
                if self.cached_head >= need {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        for k in 0..n {
            let s = seq + k;
            shared.buf[s % shared.cap].store(fill(k), Ordering::Relaxed);
            // Publish: Release pairs with the consumer's Acquire load of avail.
            shared.avail[s % shared.cap].store((s / shared.cap) as i64, Ordering::Release);
        }
    }
}

/// The single consumer.
pub struct MpConsumer {
    shared: Arc<Mpsc>,
    head: usize,
}

impl MpConsumer {
    /// Drain the contiguous published prefix (up to `max`), calling `f` per
    /// token; advance the consumer cursor once at the end. Returns the count
    /// drained (0 if the next sequence is not yet published).
    pub fn drain<F: FnMut(u64)>(&mut self, max: usize, mut f: F) -> usize {
        let shared = &*self.shared;
        let mut count = 0usize;
        while count < max {
            let s = self.head;
            let expected = (s / shared.cap) as i64;
            // `s` is published iff its slot carries `s`'s round number.
            if shared.avail[s % shared.cap].load(Ordering::Acquire) != expected {
                break;
            }
            f(shared.buf[s % shared.cap].load(Ordering::Relaxed));
            self.head += 1;
            count += 1;
        }
        if count > 0 {
            shared.head.0.store(self.head, Ordering::Release);
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::thread;

    fn run_stress(producers: usize, per: usize, cap: usize) {
        let total = producers * per;
        let (prod, mut cons) = ring(cap);
        let mut handles = Vec::new();
        for p in 0..producers {
            let mut pr = prod.clone();
            handles.push(thread::spawn(move || {
                let base = (p * per) as u64; // unique value range per producer
                let burst = 13usize;
                let mut sent = 0usize;
                while sent < per {
                    let b = burst.min(per - sent);
                    let s0 = sent;
                    pr.batch_publish(b, |k| base + (s0 + k) as u64);
                    sent += b;
                }
            }));
        }
        drop(prod); // only the clones produce

        let mut seen: HashSet<u64> = HashSet::with_capacity(total);
        let mut dups = 0usize;
        let mut received = 0usize;
        while received < total {
            received += cons.drain(usize::MAX, |v| {
                if !seen.insert(v) {
                    dups += 1;
                }
            });
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(dups, 0, "duplicate delivery");
        assert_eq!(seen.len(), total, "missing elements (loss)");
        for p in 0..producers {
            for i in 0..per {
                assert!(seen.contains(&((p * per + i) as u64)), "missing value");
            }
        }
    }

    #[test]
    fn mpsc_no_loss_no_dup_under_contention() {
        // Small cap vs large volume exercises wrap + backpressure heavily.
        // Repeat to shake out races in the lock-free publish/availability path.
        for _ in 0..5 {
            run_stress(4, 50_000, 256);
        }
    }
}
```

- [ ] **Step 2: Run the stress test to verify it passes**

Run: `cd rust && cargo test -p handoff-compare mpsc_no_loss_no_dup_under_contention -- --nocapture`
Expected: PASS (4 producers × 50k, repeated 5×; no loss, no dup, exact count).

- [ ] **Step 3: Run it a few more times** (lock-free — confirm stability):

Run: `cd rust && for i in 1 2 3; do cargo test -p handoff-compare mpsc_no_loss -q || break; done`
Expected: PASS all three runs.

- [ ] **Step 4: Lint**

Run: `cd rust && cargo clippy -p handoff-compare --all-targets 2>&1 | tail -3 && cargo fmt -p handoff-compare --check`
Expected: no warnings, no diff.

- [ ] **Step 5: Commit**

```bash
git add rust/handoff-compare/src/mpsc.rs
git commit -m "handoff-compare: lock-free multi-producer (MPSC) ring + contention stress test"
```

---

## Task 3: SPSC criterion bench

**Files:**
- Create: `rust/handoff-compare/benches/spsc.rs`

**Interfaces:**
- Consumes: `handoff_compare::spsc::channel` (Task 1); `disruptor::{build_single_producer, BusySpin, Producer}`; `crossbeam::channel::bounded`; `std::sync::mpsc::sync_channel`.
- Produces: a criterion bench group `spsc` (run with `cargo bench -p handoff-compare --bench spsc`).

- [ ] **Step 1: Create** `rust/handoff-compare/benches/spsc.rs` — mirrors disruptor-rs's `benches/spsc.rs` (same matrix, base row, sink-wait), adding `our-ring` and `std-mpsc`, with `core_affinity` pinning:

```rust
//! SPSC throughput comparison: our-ring | disruptor | crossbeam | std-mpsc,
//! across burst sizes × pauses. Methodology mirrors disruptor-rs benches/spsc.rs.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use criterion::measurement::WallTime;
use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use crossbeam::channel::TryRecvError::{Disconnected, Empty};
use crossbeam::channel::TrySendError::Full;
use crossbeam::channel::bounded;
use disruptor::{BusySpin, Producer};

const CAP: usize = 128;
const BURST_SIZES: [u64; 3] = [1, 10, 100];
const PAUSES_MS: [u64; 3] = [0, 1, 10];

struct Event {
    data: i64,
}

fn pause(millis: u64) {
    if millis > 0 {
        thread::sleep(Duration::from_millis(millis));
    }
}

/// Pin the current thread to core index `idx` (modulo available cores). No-op if
/// affinity is unavailable.
fn pin(idx: usize) {
    if let Some(cores) = core_affinity::get_core_ids() {
        if !cores.is_empty() {
            core_affinity::set_for_current(cores[idx % cores.len()]);
        }
    }
}

pub fn spsc_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("spsc");
    for burst_size in BURST_SIZES {
        group.throughput(Throughput::Elements(burst_size));
        base(&mut group, burst_size as i64);
        for pause_ms in PAUSES_MS {
            let inputs = (burst_size as i64, pause_ms);
            let param = format!("burst: {}, pause: {} ms", burst_size, pause_ms);
            our_ring(&mut group, inputs, &param);
            disruptor(&mut group, inputs, &param);
            crossbeam(&mut group, inputs, &param);
            std_mpsc(&mut group, inputs, &param);
        }
    }
    group.finish();
}

fn base(group: &mut BenchmarkGroup<WallTime>, burst_size: i64) {
    let sink = Arc::new(AtomicI64::new(0));
    let id = BenchmarkId::new("base", burst_size);
    group.bench_with_input(id, &burst_size, move |b, size| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                for data in 1..=*size {
                    sink.store(black_box(data), Ordering::Release);
                }
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
}

fn our_ring(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (mut prod, mut cons) = handoff_compare::spsc::channel(CAP);
    let consumer = {
        let sink = Arc::clone(&sink);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            pin(1);
            while !stop.load(Ordering::Relaxed) {
                cons.drain(usize::MAX, |v| sink.store(v as i64, Ordering::Release));
            }
        })
    };
    pin(0);
    let id = BenchmarkId::new("our-ring", param);
    group.bench_with_input(id, &inputs, |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                prod.batch_publish(*size as usize, |k| black_box(k as u64 + 1));
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
    stop.store(true, Ordering::Relaxed);
    consumer.join().expect("consumer panicked");
}

fn disruptor(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let processor = {
        let sink = Arc::clone(&sink);
        move |event: &Event, _seq: i64, _eob: bool| {
            sink.store(event.data, Ordering::Release);
        }
    };
    let mut producer = disruptor::build_single_producer(CAP, || Event { data: 0 }, BusySpin)
        .handle_events_with(processor)
        .build();
    let id = BenchmarkId::new("disruptor", param);
    group.bench_with_input(id, &inputs, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                producer.batch_publish(*size as usize, |iter| {
                    for (i, e) in iter.enumerate() {
                        e.data = black_box(i as i64 + 1);
                    }
                });
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
}

fn crossbeam(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = bounded::<Event>(CAP);
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(1);
            loop {
                match r.try_recv() {
                    Ok(event) => sink.store(event.data, Ordering::Release),
                    Err(Empty) => continue,
                    Err(Disconnected) => break,
                }
            }
        })
    };
    pin(0);
    let id = BenchmarkId::new("crossbeam", param);
    group.bench_with_input(id, &inputs, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                for data in 1..=*size {
                    let mut event = Event { data: black_box(data) };
                    loop {
                        match s.try_send(event) {
                            Err(Full(e)) => event = e,
                            _ => break,
                        }
                    }
                }
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
    receiver.join().expect("receiver panicked");
}

fn std_mpsc(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    use std::sync::mpsc::{TrySendError, sync_channel};
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = sync_channel::<Event>(CAP);
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(1);
            while let Ok(event) = r.recv() {
                sink.store(event.data, Ordering::Release);
            }
        })
    };
    pin(0);
    let id = BenchmarkId::new("std-mpsc", param);
    group.bench_with_input(id, &inputs, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                for data in 1..=*size {
                    let mut event = Event { data: black_box(data) };
                    loop {
                        match s.try_send(event) {
                            Err(TrySendError::Full(e)) => event = e,
                            _ => break,
                        }
                    }
                }
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
    drop(s);
    receiver.join().expect("receiver panicked");
}

criterion_group!(spsc, spsc_benchmark);
criterion_main!(spsc);
```

- [ ] **Step 2: Smoke-run the bench (quick)**

Run: `cd rust && cargo bench -p handoff-compare --bench spsc -- --quick 2>&1 | tail -20`
Expected: compiles; every cell (`base`, `our-ring`, `disruptor`, `crossbeam`, `std-mpsc` × burst × pause) runs and reports a throughput estimate with no panic.

- [ ] **Step 3: Lint the bench**

Run: `cd rust && cargo clippy -p handoff-compare --all-targets 2>&1 | tail -3 && cargo fmt -p handoff-compare --check`
Expected: no warnings, no diff.

- [ ] **Step 4: Commit**

```bash
git add rust/handoff-compare/benches/spsc.rs
git commit -m "handoff-compare: SPSC criterion bench (our-ring | disruptor | crossbeam | std-mpsc)"
```

---

## Task 4: MPSC criterion bench

**Files:**
- Create: `rust/handoff-compare/benches/mpsc.rs`

**Interfaces:**
- Consumes: `handoff_compare::mpsc::ring` (Task 2); `disruptor::{build_multi_producer, BusySpin, Producer}`; `crossbeam::channel::bounded`; `std::sync::mpsc::channel`.
- Produces: a criterion bench group `mpsc`.

- [ ] **Step 1: Create** `rust/handoff-compare/benches/mpsc.rs` — mirrors disruptor-rs's `benches/mpsc.rs` (2 producers, persistent `BurstProducer` released by a barrier, sink counts to `burst×producers`), adding `our-mp-ring` and `std-mpsc`:

```rust
//! MPSC throughput comparison: our-mp-ring | disruptor | crossbeam | std-mpsc,
//! 2 producers, across burst sizes × pauses. Mirrors disruptor-rs benches/mpsc.rs.

use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool, AtomicI64,
    Ordering::{Acquire, Relaxed, Release},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use criterion::measurement::WallTime;
use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use crossbeam::channel::TryRecvError::{Disconnected, Empty};
use crossbeam::channel::TrySendError::Full;
use crossbeam::channel::bounded;
use crossbeam_utils::CachePadded;
use disruptor::{BusySpin, Producer};

const PRODUCERS: usize = 2;
const CAP: usize = 256;
const BURST_SIZES: [u64; 3] = [1, 10, 100];
const PAUSES_MS: [u64; 3] = [0, 1, 10];

struct Event {
    data: i64,
}

fn pause(millis: u64) {
    if millis > 0 {
        thread::sleep(Duration::from_millis(millis));
    }
}

fn pin(idx: usize) {
    if let Some(cores) = core_affinity::get_core_ids() {
        if !cores.is_empty() {
            core_affinity::set_for_current(cores[idx % cores.len()]);
        }
    }
}

/// Persistent producer thread released by a barrier each iteration, so we don't
/// pay thread-spawn cost per sample. (From disruptor-rs benches/mpsc.rs.)
struct BurstProducer {
    start_barrier: Arc<CachePadded<AtomicBool>>,
    stop: Arc<CachePadded<AtomicBool>>,
    join_handle: Option<JoinHandle<()>>,
}

impl BurstProducer {
    fn new<P: 'static + Send + FnMut()>(core: usize, mut produce_one_burst: P) -> Self {
        let start_barrier = Arc::new(CachePadded::new(AtomicBool::new(false)));
        let stop = Arc::new(CachePadded::new(AtomicBool::new(false)));
        let join_handle = {
            let stop = Arc::clone(&stop);
            let start_barrier = Arc::clone(&start_barrier);
            thread::spawn(move || {
                pin(core);
                while !stop.load(Acquire) {
                    while start_barrier
                        .compare_exchange(true, false, Acquire, Relaxed)
                        .is_err()
                    {
                        if stop.load(Acquire) {
                            return;
                        }
                    }
                    produce_one_burst();
                }
            })
        };
        Self {
            start_barrier,
            stop,
            join_handle: Some(join_handle),
        }
    }
    fn start(&self) {
        self.start_barrier.store(true, Release);
    }
    fn stop(&mut self) {
        self.stop.store(true, Release);
        self.join_handle.take().unwrap().join().expect("panic");
    }
}

fn run_benchmark(
    group: &mut BenchmarkGroup<WallTime>,
    id: BenchmarkId,
    burst_size: Arc<AtomicI64>,
    sink: Arc<AtomicI64>,
    params: (i64, u64),
    burst_producers: &[BurstProducer],
) {
    group.bench_with_input(id, &params, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            burst_size.store(*size, Release);
            let count = black_box(*size * burst_producers.len() as i64);
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                sink.store(0, Release);
                burst_producers.iter().for_each(BurstProducer::start);
                while sink.load(Acquire) != count {}
            }
            start.elapsed()
        })
    });
}

pub fn mpsc_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpsc");
    for burst_size in BURST_SIZES {
        group.throughput(Throughput::Elements(burst_size));
        base(&mut group, burst_size as i64);
        for pause_ms in PAUSES_MS {
            let params = (burst_size as i64, pause_ms);
            let desc = format!("burst: {}, pause: {} ms", burst_size, pause_ms);
            our_mp_ring(&mut group, params, &desc);
            disruptor(&mut group, params, &desc);
            crossbeam(&mut group, params, &desc);
            std_mpsc(&mut group, params, &desc);
        }
    }
    group.finish();
}

fn base(group: &mut BenchmarkGroup<WallTime>, size: i64) {
    let sink = Arc::new(AtomicI64::new(0));
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let sink = Arc::clone(&sink);
            let burst_size = Arc::clone(&burst_size);
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire);
                for _ in 0..n {
                    sink.fetch_add(1, Release);
                }
            })
        })
        .collect::<Vec<_>>();
    run_benchmark(
        group,
        BenchmarkId::new("base", size),
        burst_size,
        sink,
        (size, 0),
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
}

fn our_mp_ring(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (prod, mut cons) = handoff_compare::mpsc::ring(CAP);
    let consumer = {
        let sink = Arc::clone(&sink);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            pin(0);
            while !stop.load(Relaxed) {
                let n = cons.drain(usize::MAX, |v| {
                    black_box(v);
                });
                if n > 0 {
                    sink.fetch_add(n as i64, Release);
                }
            }
        })
    };
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let mut pr = prod.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire) as usize;
                pr.batch_publish(n, |k| black_box(k as u64));
            })
        })
        .collect::<Vec<_>>();
    drop(prod);
    run_benchmark(
        group,
        BenchmarkId::new("our-mp-ring", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
    stop.store(true, Relaxed);
    consumer.join().expect("consumer panicked");
}

fn disruptor(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let processor = {
        let sink = Arc::clone(&sink);
        move |event: &Event, _seq: i64, _eob: bool| {
            black_box(event.data);
            sink.fetch_add(1, Release);
        }
    };
    let producer = disruptor::build_multi_producer(CAP, || Event { data: 0 }, BusySpin)
        .handle_events_with(processor)
        .build();
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let mut producer = producer.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire) as usize;
                producer.batch_publish(n, |iter| {
                    for (i, e) in iter.enumerate() {
                        e.data = black_box(i as i64);
                    }
                });
            })
        })
        .collect::<Vec<_>>();
    drop(producer);
    run_benchmark(
        group,
        BenchmarkId::new("disruptor", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
}

fn crossbeam(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = bounded::<Event>(CAP);
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(0);
            loop {
                match r.try_recv() {
                    Ok(event) => {
                        black_box(event.data);
                        sink.fetch_add(1, Release);
                    }
                    Err(Empty) => continue,
                    Err(Disconnected) => break,
                }
            }
        })
    };
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let s = s.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire);
                for data in 0..n {
                    let mut event = Event { data: black_box(data) };
                    loop {
                        match s.try_send(event) {
                            Err(Full(e)) => event = e,
                            _ => break,
                        }
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    drop(s);
    run_benchmark(
        group,
        BenchmarkId::new("crossbeam", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
    receiver.join().expect("receiver panicked");
}

fn std_mpsc(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    use std::sync::mpsc::channel;
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = channel::<Event>();
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(0);
            while let Ok(event) = r.recv() {
                black_box(event.data);
                sink.fetch_add(1, Release);
            }
        })
    };
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let s = s.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire);
                for data in 0..n {
                    s.send(Event { data: black_box(data) }).expect("send");
                }
            })
        })
        .collect::<Vec<_>>();
    drop(s);
    run_benchmark(
        group,
        BenchmarkId::new("std-mpsc", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
    receiver.join().expect("receiver panicked");
}

criterion_group!(mpsc, mpsc_benchmark);
criterion_main!(mpsc);
```

Note: `crossbeam_utils` is re-exported via the `crossbeam` crate (`crossbeam::utils::CachePadded`); if the direct `crossbeam_utils` import fails to resolve, change the import to `use crossbeam::utils::CachePadded;`. Verify in Step 2.

- [ ] **Step 2: Smoke-run the bench (quick)**

Run: `cd rust && cargo bench -p handoff-compare --bench mpsc -- --quick 2>&1 | tail -20`
Expected: compiles; every cell runs (2 producers) and reports throughput; no panic/deadlock. (If `crossbeam_utils` import errors, switch to `use crossbeam::utils::CachePadded;` and re-run.)

- [ ] **Step 3: Lint**

Run: `cd rust && cargo clippy -p handoff-compare --all-targets 2>&1 | tail -3 && cargo fmt -p handoff-compare --check`
Expected: no warnings, no diff.

- [ ] **Step 4: Commit**

```bash
git add rust/handoff-compare/benches/mpsc.rs
git commit -m "handoff-compare: MPSC criterion bench (our-mp-ring | disruptor | crossbeam | std-mpsc)"
```

---

## Task 5: Full run + written analysis

**Files:**
- Modify: `docs/superpowers/specs/2026-06-28-handoff-disruptor-comparison-design.md` (append a "## Results" section), OR create `docs/handoff-disruptor-results.md`.

**Interfaces:** none (analysis).

- [ ] **Step 1: Full SPSC run** (pinned; the full criterion run, not `--quick`)

Run: `cd rust && cargo bench -p handoff-compare --bench spsc 2>&1 | tee /tmp/spsc-bench.txt | tail -40`
Expected: criterion prints a throughput estimate + 95% CI for each (impl × burst × pause) cell. Capture the table.

- [ ] **Step 2: Full MPSC run**

Run: `cd rust && cargo bench -p handoff-compare --bench mpsc 2>&1 | tee /tmp/mpsc-bench.txt | tail -40`
Expected: same, for the MPSC cells.

- [ ] **Step 3: Sanity-check the numbers**

Confirm: throughput > 0 everywhere; `our-ring`/`our-mp-ring` and `disruptor` both clearly beat `std-mpsc` at burst≥10; numbers are stable run-to-run (pinning working). If `our-*` looks implausible (e.g. orders of magnitude beyond disruptor at burst=1), re-read the measurement to rule out a stale-sink early-exit artifact before trusting it.

- [ ] **Step 4: Write the analysis** — append a `## Results` section to the design doc capturing, for SPSC and MPSC: a throughput table (impl × burst × pause), and 4-8 sentences of interpretation — where batch amortization closes/opens gaps (burst 1 vs 100), how pause>0 (the idle→wakeup path) reorders busy-spin vs blocking impls, our-ring vs disruptor head-to-head, and MPSC claim-contention cost vs SPSC. Reference the captured tables.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-06-28-handoff-disruptor-comparison-design.md
git commit -m "docs: handoff vs disruptor-rs comparison results + analysis"
```

---

## Final verification

- [ ] **Step 1: Crate green**

Run: `cd rust && cargo test -p handoff-compare && cargo clippy -p handoff-compare --all-targets && cargo fmt -p handoff-compare --check`
Expected: all tests pass (SPSC single/batch, MPSC stress); no warnings; no fmt diff.

- [ ] **Step 2: Workspace unaffected**

Run: `cd rust && cargo build --release 2>&1 | tail -2`
Expected: the whole workspace (incl. the untouched `thread-handoff-*` cells) builds.

- [ ] **Step 3: Both benches smoke clean**

Run: `cd rust && cargo bench -p handoff-compare -- --quick 2>&1 | tail -5`
Expected: both `spsc` and `mpsc` groups run end-to-end without panic.
