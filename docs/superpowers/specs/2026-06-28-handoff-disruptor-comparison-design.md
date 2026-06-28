# Handoff vs disruptor-rs Comparison — Design

**Date:** 2026-06-28
**Status:** Proposed — awaiting review

## Purpose

Thoroughly compare **our Rust handoff ring** against the **disruptor-rs** crate
(v4.3, an LMAX Disruptor port) across **SPSC and MPSC** scenarios and **a range
of burst sizes**, to characterize where our lean hand-rolled ring wins/loses and
why. Extends the earlier single-number autobench comparison (our SPSC ring
~367.6M vs disruptor ~148M ops/s) into a full burst×pause matrix with reference
implementations.

Reference source studied: `../disruptor-rs/` (its `benches/spsc.rs`,
`benches/mpsc.rs`, builder/producer API). We mirror its benchmark methodology so
our numbers are directly comparable to its published results.

## Scope & form

A **rust-only criterion benchmark study** — not part of the cross-language
result-contract grid (the autobench/bench-infra model produces one
`handoff_throughput` number per cell; criterion's burst×pause sampling with
confidence intervals is the right tool for a fine-grained microbench study, and
matches how disruptor-rs benchmarks itself).

It lives in a **new self-contained crate** so the shipped, AWS-validated,
autobench-optimized `thread-handoff-*` cells stay untouched. (Refactoring the
`thread-handoff-ring` binary into a shared library would move its optimization
surface out of the autobench cell's mutable path — a conflict we avoid.)

Run **locally with thread pinning** (`core_affinity`): every implementation is
measured back-to-back on the same pinned cores, so the *relative* comparison is
sound and pinning + criterion's robust sampling tame the dev-box scheduling
variance that hurt the single-shot autobench runs. No AWS run is part of this
study.

## Crate layout

```
rust/handoff-compare/                 (new crate; NOT in the cross-language grid)
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── spsc.rs      # our optimized SPSC ring, mirrored from thread-handoff/ring,
│   │                #   + a batch claim/fill/publish API
│   └── mpsc.rs      # NEW lock-free multi-producer ring (disruptor-style)
└── benches/
    ├── spsc.rs      # criterion: our-ring | disruptor | crossbeam | std-mpsc
    └── mpsc.rs      # criterion: our-mp-ring | disruptor | crossbeam | std-mpsc
```

`Cargo.toml`:
- `[dependencies]`: `disruptor = "4.3"`.
- `[dev-dependencies]`: `criterion = "0.5"`, `crossbeam = "0.8"`, `core_affinity = "0.8"`.
- `[[bench]] name = "spsc"/"mpsc"`, `harness = false`.
- Added to the rust workspace `members`. The release profile is inherited.

These third-party deps are **bench/comparison-only**; the shipped `thread-handoff`
cells remain std-only. The crate is excluded from the autobench task registry and
the bench-infra matrix.

## `src/spsc.rs` — our SPSC ring (mirrored) + batch

A verbatim copy of the optimized SPSC ring from `rust/thread-handoff/ring/src/spsc.rs`
(the `channel(cap) -> (Producer, Consumer)` design: `head`/`tail` on separate
64-byte cache lines via `#[repr(align(64))]`, producer-cached `head` /
consumer-cached `tail` so the contended atomic is read only when the ring appears
full/empty). A one-line provenance comment notes it is a pinned snapshot for this
study (the shipped cell is the source of truth).

**Added batch API** (needed for burst>1, matching disruptor's `batch_publish`):
- `Producer::batch_publish(n, fill)` — wait until `n` slots are free (cached-head
  backpressure), write them via `fill(iter)`, then publish `tail += n` once
  (single Release). One barrier per burst instead of per element.
- `Consumer::drain(max, f)` — read all currently-available elements (up to `max`),
  calling `f` per element, advancing `head` once at the end. Lets the consumer
  amortize its Release per drained batch.

Single-element `push`/`pop` remain for burst=1.

## `src/mpsc.rs` — lock-free multi-producer ring (NEW)

The disruptor multi-producer algorithm:

- Ring of `cap` slots (power of two), each an `AtomicU64` payload (plus the
  availability buffer below).
- **Claim:** a single `claim: AtomicUsize` cursor. A producer claims a contiguous
  range with `claim.fetch_add(n, Relaxed)` → `[seq, seq+n)`. (Monotonic; index =
  `seq % cap`.)
- **Backpressure:** before writing claimed `seq`, the producer busy-waits until
  `seq + 1 - cap <= consumer_head` — i.e. the slot it will overwrite has been
  consumed. The consumer head is read (Acquire) only when the producer's cached
  copy says it might be full (cached like SPSC).
- **Out-of-order publish (availability buffer):** producers finish in arbitrary
  order, so a single "published" counter cannot simply advance. An
  `available: Box<[AtomicI64]>` of length `cap` records, per slot, the **round
  number** it was last published at: after writing slot `seq`, the producer does
  `available[seq % cap].store(seq / cap, Release)`. Sequence `seq` is published
  iff `available[seq % cap].load(Acquire) == (seq / cap) as i64`.
- **Consumer (single):** holds a `head` cursor; busy-waits until `head` is
  available, then scans the **contiguous available prefix** (`head`, `head+1`, …
  while each is available), reading each slot, and advances `head` (Release) once
  per drain. This naturally batches.
- **Wait strategy:** busy-spin with `std::hint::spin_loop()` (matching
  disruptor-rs `BusySpin`, our SPSC ring, and a fair comparison).

**Correctness invariant:** every claimed sequence is delivered to the consumer
**exactly once, in sequence order**. Items from different producers interleave by
claim order; there is no per-producer global FIFO guarantee (identical to
disruptor's multi-producer semantics). No loss, no duplication, no slot
overwritten before it is consumed.

Public API mirrors the SPSC shape for bench symmetry:
- `mpsc::ring(cap) -> (MpProducer, MpConsumer)`; `MpProducer: Clone` (each clone
  is an independent producer sharing the claim cursor + availability buffer).
- `MpProducer::batch_publish(n, fill)`; `MpConsumer::drain(max, f)`.

## Benches (criterion, mirroring disruptor-rs)

Both benches use `iter_custom` for wall-time, `Throughput::Elements(burst)`, the
matrix **burst ∈ [1, 10, 100] × pause ∈ [0, 1, 10] ms**, and a `base` row
measuring pure measurement overhead (an `AtomicI64` sink), exactly as
disruptor-rs does. Each measured iteration: publish one burst, then busy-wait
until the consumer's `sink: AtomicI64` reaches the expected count. The `pause`
dimension exercises the idle→wakeup path (where busy-spin vs blocking diverges).

**Pinning:** `core_affinity::get_core_ids()` → pin the producer thread(s) and the
consumer thread to distinct physical cores; skip pinning gracefully if affinity
is unavailable (record that in the run notes).

### `benches/spsc.rs` — `cap = 128`, single producer

| label | implementation |
|-------|----------------|
| `our-ring` | our SPSC ring; `batch_publish`/`drain` |
| `disruptor` | `build_single_producer(128, factory, BusySpin).handle_events_with(sink-store)`, `batch_publish` |
| `crossbeam` | `crossbeam::channel::bounded(128)`, `try_send`/`try_recv` loop |
| `std-mpsc` | `std::sync::mpsc::sync_channel(128)` |

### `benches/mpsc.rs` — `cap = 256`, 2 producers

Uses disruptor-rs's persistent **`BurstProducer`** pattern (producer threads
released by a barrier each iteration, to avoid per-sample thread-spawn overhead);
the consumer's `sink` counts total received and the driver waits for
`burst × producers`.

| label | implementation |
|-------|----------------|
| `our-mp-ring` | our new MP ring; cloned `MpProducer` per producer, `batch_publish` |
| `disruptor` | `build_multi_producer(256, factory, BusySpin)`, `producer.clone()` per producer, `batch_publish` |
| `crossbeam` | `crossbeam::channel::bounded(256)`, cloned senders |
| `std-mpsc` | `std::sync::mpsc::channel`, cloned senders |

## Testing

The comparability-critical code is the **ring correctness** (the benches are
validated by running). Unit tests in `handoff-compare`:

- **SPSC**: single-element order+count (every token received once, in order);
  batch `batch_publish`/`drain` order+count, including non-divisible bursts and
  wrap-around (more elements than `cap`).
- **MPSC**: a **stress test** — 2–4 cloned producers each publish many *unique*
  tokens (e.g. producer `p` emits `p*N .. p*N+N`); the consumer collects all and
  asserts the multiset is exactly the union with no loss and no duplication, and
  the total count is exact. Run with a high element count and repeat the test
  several times to shake out races (the lock-free publish/availability path is
  the riskiest code).
- Backpressure: a `cap`-sized ring with far more elements than `cap` must not
  overwrite an unconsumed slot (the stress test at small `cap` covers this).

Keep the crate **clippy- and rustfmt-clean**. A `cargo bench -- --quick` smoke
confirms every cell runs and yields positive throughput before the full run.

## Output

- The full criterion matrix (throughput estimate + confidence interval per cell),
  saved/summarized from the criterion run.
- A written **analysis** (appended to this spec's results section or a sibling
  results doc): our-ring vs disruptor vs crossbeam vs std-mpsc, per scenario ×
  burst × pause, with interpretation — e.g. where batch amortization closes or
  opens gaps, how pause>0 (park/wakeup) changes the ranking for blocking vs
  busy-spin impls, and the cost of multi-producer claim contention vs SPSC.
- No result-contract / journal coupling (rust-only study). Headline numbers can
  optionally be surfaced into the journal later if desired; not part of this work.

## Out of scope (YAGNI)

- MPMC (multiple consumers) — disruptor supports it, but our handoff focus is
  single-consumer.
- The `EventPoller` disruptor API (comparable perf per their README; the managed
  event-handler closure path is the representative one).
- Go/Java disruptor-equivalents (no mature direct analog; this study is rust-only).
- Wiring into bench-infra/autobench or AWS.
- Changing or re-optimizing the shipped `thread-handoff` cells.
