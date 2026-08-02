# smr-collections — Remove the per-write `Arc::get_mut` from `CowBook` — Design

**Date:** 2026-08-02
**Status:** Approved — ready for implementation planning

## Purpose

The 20260802T132729Z fleet run measured the chunked copy-on-write store's cancel
at **258 ns in Rust against 125 ns for the flat store** — a 2.1× penalty — while
Go's structurally identical implementation showed parity (133 vs 136 ns). The
run entry originally attributed this to the ladder rescan walking chunk
indirection. **That hypothesis was refuted by experiment.**

`mvcc_churn` never calls `capture()`, so the generation never bumps and **not a
single chunk is ever copied**. There is no copy-on-write in the cell at all. A
local run with `SMRC_LEVELS=8` and ~1,000 orders per level — a configuration in
which a level essentially never empties, so `repair_best` exits at its guard —
reproduces the gap at 1.88×, against the fleet's 2.06×.

The gap instead scales with the number of **mutable accesses per operation**:

| op | `*_mut` calls | flat | CoW | gap | per access |
|---|---|---|---|---|---|
| `update` | 2 | 46.6 ns | 60.7 ns | +14.1 | 7.0 ns |
| `insert` | 3 | 47.9 ns | 78.3 ns | +30.4 | 10.1 ns |
| `cancel` | 4 | 54.1 ns | 101.5 ns | +47.4 | 11.8 ns |

The cause is visible in the two implementations. Rust:

```rust
&mut Arc::get_mut(&mut self.order_chunks[ci])   // atomic uniqueness check
    .expect("current-gen chunk is unshared")     // + Option branch
    .orders[off]
```

Go:

```go
c := b.orderChunks[ci]                           // plain pointer load
return &c.orders[int(slot)%b.Chunk]
```

Rust performs an atomic refcount check on **every write**, to verify an
invariant the epoch check immediately above has already established. Go relies
on the same invariant and simply trusts it. The consequence is that Rust's six
`mvcc_*` cells have carried an avoidable per-write cost in every run to date,
and the flat-vs-CoW delta the grid publishes measures a safety check one
language pays and another does not — rather than copy-on-write.

**Decision (project owner):** make the Rust artifact trust the same invariant
Go trusts, so the delta measures the data structure. `CLAUDE.md` states the
goal is to "choose and optimize the code for each path", and invariant-trusting
hot paths are exactly what Agrona/Aeron-style code does in this domain.

Out of scope: the Java `Book`/`CowBook` asymmetry (different id-map and order
representation), which is a separate design question about whether artifacts
should be idiomatic-per-language or structurally symmetric.

## The change

Two functions in `rust/smr-collections/common/src/cowbook.rs`: `order_mut` and
`level_mut`. Nothing else in the crate changes.

Replace the `Arc::get_mut(...).expect(...)` tail of each with a documented
unsafe deref, guarded in debug builds:

```rust
        let arc = &mut self.order_chunks[ci];
        debug_assert_eq!(
            Arc::strong_count(arc),
            1,
            "current-gen chunk must be unshared"
        );
        // SAFETY: the epoch check above guarantees `born == gen`. `capture()`
        // clones the chunk-ref tables into the Root and *then* bumps `gen`, so
        // every Arc a Root holds necessarily has `born < gen`. A chunk with
        // `born == gen` therefore has `strong_count == 1`, and we hold
        // `&mut self`, so no other reference into it can exist. `Arc::as_ptr`
        // deliberately retains mutable provenance (`&raw mut (*ptr).data`,
        // alloc/src/sync.rs) precisely so callers can write through it.
        let chunk = unsafe { &mut *(Arc::as_ptr(arc) as *mut OrderChunk) };
        &mut chunk.orders[off]
```

`level_mut` takes the identical treatment against `LevelChunk`.

**The `as_ptr` cast is sound on stable, and not by accident.** `Arc::as_ptr` is
implemented as `unsafe { &raw mut (*ptr).data }`, carrying a SAFETY comment
stating it "is required to retain raw/mut provenance such that e.g. `get_mut`
can write through the pointer" (`library/alloc/src/sync.rs`). The returned
pointer therefore carries write provenance by design; casting it back to `*mut`
and writing through it is sound whenever the `Arc` is uniquely owned — which is
exactly what the epoch invariant establishes below.

`Arc::get_mut_unchecked` is the obvious alternative and is **not available**:
`rust-toolchain.toml` pins stable and that method is nightly-only.
`Arc::get_mut(...).unwrap_unchecked()` does not help — the atomic check being
removed lives inside `get_mut`, not in the `Option` handling.

`Rc` is also not available: `Root` is moved to a serializer thread in
`live_mvcc` and `live_mvcc_churn` (`std::thread::spawn(move || ...)`), so the
chunks must be `Send + Sync`.

## Why the invariant holds

`capture()` performs, in order:

1. clone the three chunk-ref tables into a new `Root`
2. `self.gen += 1`

So at the moment a `Root` takes its clones, every chunk it holds has
`born <= gen_old`; immediately afterwards `gen` becomes `gen_old + 1`, making
`born < gen` for all of them. Any chunk created after that point — either by
`CowBook::new` at `gen == 1`, or by the copy branch inside `order_mut` /
`level_mut` — is created with `born == gen` and is held by nothing else.

Therefore **`born == gen` implies `strong_count == 1`**, which is exactly why
today's `.expect("current-gen chunk is unshared")` never fires. The change
replaces a runtime re-verification with a documented invariant plus a debug
assertion.

This holds across repeated captures: each bumps `gen`, so a chunk written at
generation *n* and then captured has `born == n < gen == n+1`, and the next
write to it takes the copy branch.

## Verification

**Behaviour preservation is already covered by existing tests**, and they are
strong ones — every one compares CoW output byte-for-byte against either the
flat store or a committed cross-language golden:

- `cowbook_matches_golden_bytes` — CoW image vs the pinned golden
- `cow_encode_equals_stw_encode_after_mixed_ops` — CoW vs flat after insert+update
- `cow_churn_image_matches_flat_churn_image` — CoW vs flat over a 10,000-op churn stream
- `cow_cancel_matches_book_cancel`, `capture_carries_free_head`
- the churn snapshot → restore → replay → re-snapshot byte-identity test

If this change alters behaviour, those fail. No new behavioural test is needed.

**Two additions:**

1. The `debug_assert_eq!(Arc::strong_count(arc), 1, ...)` shown above. Debug
   builds and the whole test suite run with `debug_assertions` on, so an
   invariant violation fails loudly in CI rather than becoming UB. It compiles
   out of the release builds the benchmarks use.
2. **A Miri run** over `smr-collections-common`'s test suite. This is the first
   `unsafe` in that crate, and the cancel/churn design spec's own requirements
   list already calls for Miri on unsafe layout code. Miri is the tool that
   would catch a provenance mistake in the `as_ptr` cast, which is the one
   genuinely subtle part of this change.

Note Miri cannot run the whole suite usefully — the golden tests read files and
the churn tests run 10,000+ ops, which is slow under Miri but not prohibitive
at the small test configs. Scope the Miri run to the `cowbook` and `cowsnap`
test modules and say so in the report if anything is skipped.

## Measurement consequence

This changes six already-journaled Rust cells: `mvcc_insert`, `mvcc_update`,
`mvcc_snapshot`, `live_mvcc`, `mvcc_churn`, `live_mvcc_churn`.

The expected effect is a reduction of roughly 7–12 ns per mutable access —
about 14 ns on `update`, 30 ns on `insert`, 47 ns on `cancel`, from the table
above. On `mvcc_snapshot` and the `live_*` cells the effect should be
negligible, since those are dominated by the serialize.

**Quantifying it needs a same-host A/B**, not a cross-run comparison: this grid
carries a documented ±21–35 % cross-instance band, which would swamp a ~10 ns
effect entirely. The pattern to follow is the one used for the ultima_db
`#19`/`#20` engine deltas — two runs on the same instances, before and after,
quoting only the delta. Whether that A/B happens as its own fleet run or folded
into the next one is an operational decision, not a design one.

**Until that A/B exists, the six cells' published numbers are stale by a known
sign** (they are too slow). That must be recorded in the run entry for whichever
run first carries this change.

## Open items deliberately deferred

- The Java `Book` / `CowBook` structural asymmetry — `Long2ObjectHashMap<Order>`
  over pooled objects versus `Long2LongHashMap` over primitive `OrderChunk`
  fields. Java's flat-vs-CoW delta measures memory layout rather than
  copy-on-write, and fixing it trades the repo's "idiomatic artifact per
  language" premise for a controlled within-language comparison. Its own
  design question.
- Whether Go's chunk access should gain an equivalent debug-time assertion. Go
  has no refcount to check, so there is no cheap equivalent, and its correctness
  rests on the same epoch invariant with nothing verifying it. Noted, not acted
  on.
