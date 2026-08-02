# CowBook `Arc::get_mut` Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the per-write `Arc::get_mut` atomic refcount check from Rust's `CowBook`, so its chunk access trusts the same epoch invariant Go's does.

**Architecture:** Two functions in one file. `order_mut` and `level_mut` currently call `Arc::get_mut(...).expect(...)` on every mutable access, re-verifying an invariant the epoch check one line above already guarantees. Each is replaced by a documented `unsafe` deref through `Arc::as_ptr`, guarded by a `debug_assert!` on `strong_count`. Correctness is established by the existing byte-identity goldens; soundness by Miri.

**Tech Stack:** Rust edition 2024, stable toolchain (pinned), Miri via an explicitly-invoked nightly.

**Spec:** [`docs/superpowers/specs/2026-08-02-cowbook-arc-getmut-design.md`](../specs/2026-08-02-cowbook-arc-getmut-design.md)

## Global Constraints

- Rust **edition 2024**; `rust-toolchain.toml` pins **stable**. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` must pass.
- **Miri is nightly-only.** Invoke it as `cargo +nightly miri test`, never `cargo miri test` — the toolchain file would otherwise select stable and the command fails with "the 'miri' component ... is not available for the 'stable' toolchain". A nightly toolchain with the `miri` component is already installed.
- **Determinism is the top requirement.** `CowBook` and `Book` must produce byte-identical snapshot images from the same op stream, and both must match the committed cross-language goldens. This change must be strictly behaviour-preserving.
- The golden files under `rust/smr-collections/testdata/` are **read-only**. A mismatch is a real finding — report it, never regenerate.
- The `debug_assert!` must compile out of release builds — the benchmarks run `--release`, and an atomic load reintroduced there would defeat the change.
- **stdout is result-contract JSON lines only** — no `println!` in library code.
- Do NOT run any AWS benchmark, `terraform`, or anything under `bench-infra/`. Local runs are fitness checks and are never journaled.
- Do not touch `go/` or `java/`.

## File Structure

**Modified:**
- `rust/smr-collections/common/src/cowbook.rs` — `order_mut` and `level_mut` only
- `docs/RESULTS.md` — a staleness note on the six affected cells

Nothing is created. No test file changes: the behavioural coverage that matters already exists.

---

### Task 1: Replace `Arc::get_mut` in both chunk accessors

**Files:**
- Modify: `rust/smr-collections/common/src/cowbook.rs` (`order_mut` ~line 150, `level_mut` ~line 165)

**Interfaces:**
- Produces: no signature changes. `order_mut(&mut self, slot: u32) -> &mut Order` and `level_mut(&mut self, side: u8, t: u32) -> &mut PriceLevel` keep their exact signatures and visibility (`pub(crate)`).

This task is behaviour-preserving by construction, so the "failing test first" cycle does not apply — there is no new behaviour to drive out. The equivalent discipline here is: **capture the baseline first, prove the tests pass before and after, and prove soundness with a tool that can see what the tests cannot.**

- [ ] **Step 1: Capture the before-state**

Record the numbers this change is meant to move, on this host, before touching anything:

```sh
cd rust && cargo build --release -q -p smr-collections-churn -p smr-collections-mvcc_churn
for cell in churn mvcc_churn; do
  echo -n "$cell: "
  SMRC_CAP=65536 SMRC_LEVELS=8 SMRC_STEADY=8000 SMRC_WARMUP=2000 SMRC_ITERS=40000 \
    cargo run --release -q -p smr-collections-$cell 2>/dev/null \
    | grep -E '"metric":"(cancel|insert)_mean"'
done
```

`SMRC_LEVELS=8` with `SMRC_STEADY=8000` puts ~1,000 orders on each level, so a level essentially never empties and `repair_best` exits at its guard — isolating the per-access cost from the rescan. Expect roughly `churn` cancel ≈ 54 ns, `mvcc_churn` cancel ≈ 101 ns (a ~1.9× gap). **Record the actual numbers in your report** — Step 6 compares against them.

- [ ] **Step 2: Confirm the suite and Miri are green before the change**

```sh
cd rust && cargo test -p smr-collections-common
cargo +nightly miri test -p smr-collections-common cowbook
```

Expected: both PASS. This establishes that any later failure was introduced by this change rather than pre-existing. If Miri reports anything now, stop and report it — that would be a pre-existing soundness problem and a different investigation.

- [ ] **Step 3: Replace the tail of `order_mut`**

In `rust/smr-collections/common/src/cowbook.rs`, replace exactly this:

```rust
        let off = slot as usize % self.chunk;
        &mut Arc::get_mut(&mut self.order_chunks[ci])
            .expect("current-gen chunk is unshared")
            .orders[off]
```

with:

```rust
        let off = slot as usize % self.chunk;
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
        // deliberately retains mutable provenance (`&raw mut (*ptr).data` in
        // alloc/src/sync.rs) precisely so callers can write through it.
        let chunk = unsafe { &mut *(Arc::as_ptr(arc) as *mut OrderChunk) };
        &mut chunk.orders[off]
```

Leave the epoch-check block above it untouched — that block is what establishes the invariant, and removing or reordering it would make the `unsafe` unsound.

- [ ] **Step 4: Replace the tail of `level_mut`**

Replace exactly this:

```rust
        &mut Arc::get_mut(&mut lane[ci])
            .expect("current-gen chunk is unshared")
            .levels[t as usize % LEVEL_CHUNK]
```

with:

```rust
        let arc = &mut lane[ci];
        debug_assert_eq!(
            Arc::strong_count(arc),
            1,
            "current-gen chunk must be unshared"
        );
        // SAFETY: as in `order_mut` — the epoch check above guarantees
        // `born == gen`, and `capture()` bumps `gen` after cloning, so a
        // current-generation chunk is never held by a Root.
        let chunk = unsafe { &mut *(Arc::as_ptr(arc) as *mut LevelChunk) };
        &mut chunk.levels[t as usize % LEVEL_CHUNK]
```

- [ ] **Step 5: Run the full suite**

```sh
cd rust && cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: PASS. The tests that matter here — and the reason no new test is needed — compare `CowBook` output byte-for-byte against the flat store and the committed goldens:

- `cowbook_matches_golden_bytes`
- `cow_encode_equals_stw_encode_after_mixed_ops`
- `cow_churn_image_matches_flat_churn_image` (10,000-op churn stream)
- `cow_cancel_matches_book_cancel`, `capture_carries_free_head`
- the churn snapshot → restore → replay → re-snapshot byte-identity test

If any of these fail, the change altered behaviour. **Do not adjust a test or a golden** — stop and report which one failed and how the bytes differ.

- [ ] **Step 6: Verify soundness under Miri**

```sh
cd rust && cargo +nightly miri test -p smr-collections-common
```

Expected: PASS with no diagnostics. Miri is the tool that would catch a provenance mistake in the `as_ptr` cast — the one genuinely subtle part of this change, and something the byte-identity tests cannot see.

If Miri is slow on the larger churn tests, scope it (`cargo +nightly miri test -p smr-collections-common cowbook` and `... cowsnap`) and **state explicitly in your report which tests you ran under Miri and which you skipped.** A silently narrowed Miri run is worse than none, because it reads as full coverage.

- [ ] **Step 7: Measure the after-state**

Re-run Step 1's exact command and compare:

```sh
cd rust && cargo build --release -q -p smr-collections-churn -p smr-collections-mvcc_churn
for cell in churn mvcc_churn; do
  echo -n "$cell: "
  SMRC_CAP=65536 SMRC_LEVELS=8 SMRC_STEADY=8000 SMRC_WARMUP=2000 SMRC_ITERS=40000 \
    cargo run --release -q -p smr-collections-$cell 2>/dev/null \
    | grep -E '"metric":"(cancel|insert)_mean"'
done
```

Expected: `churn` unchanged (it does not touch this code); `mvcc_churn` cancel down materially from ~101 ns toward the flat store's ~54 ns. **Report both before and after numbers.**

This is a local fitness check on a shared dev host, so treat it as directional evidence that the change did what it claims — not as a result. It is never journaled. The publishable figure needs a same-host fleet A/B, which is out of scope here.

If `mvcc_churn` does **not** improve, stop and report it: that would mean the diagnosis in the spec was wrong, and no further change should be made on top of it.

- [ ] **Step 8: Commit**

```sh
cd rust && cargo fmt
git add rust/smr-collections/common/src/cowbook.rs
git commit -m "perf(smrcoll): trust the epoch invariant in CowBook's chunk accessors

order_mut/level_mut called Arc::get_mut(...).expect(...) on every mutable
access, running an atomic refcount check to verify an invariant the epoch
check one line above already guarantees: capture() clones the chunk tables
and then bumps gen, so a chunk with born == gen is never held by a Root.
That is why the expect never fired.

Replaced with a documented unsafe deref through Arc::as_ptr (which retains
mutable provenance by design), guarded by a debug_assert on strong_count so
a violated invariant fails loudly in tests rather than becoming UB. Go's
equivalent has always been a plain pointer load trusting the same invariant.

Verified by the existing byte-identity goldens and under Miri."
```

---

### Task 2: Record the measurement consequence

**Files:**
- Modify: `docs/RESULTS.md`

**Interfaces:**
- Consumes: the before/after numbers from Task 1 Steps 1 and 7.

Six already-published Rust cells (`mvcc_insert`, `mvcc_update`, `mvcc_snapshot`, `live_mvcc`, `mvcc_churn`, `live_mvcc_churn`) were measured with the old code. Their numbers are now stale by a known sign — too slow. A reader comparing them against Go's or against a future run needs to be told.

- [ ] **Step 1: Add the staleness note**

`docs/RESULTS.md` already carries a bullet explaining the `Arc::get_mut` finding, beginning **"Chunked CoW's cancel penalty in Rust is `Arc::get_mut`, not copy-on-write."** Append to the end of that same bullet:

```markdown
  **Since fixed** — `order_mut`/`level_mut` now trust the epoch invariant
  directly, matching Go. Every Rust `mvcc_*` figure on this page predates that
  change and is therefore slow by roughly 7–12 ns per mutable access
  (~14 ns/update, ~30 ns/insert, ~47 ns/cancel). Quantifying it properly needs
  a same-host A/B — this grid's ±21–35 % cross-instance band would swamp the
  effect in any cross-run comparison — so the figures here stand until that
  run happens.
```

- [ ] **Step 2: Verify the note reads correctly in place**

```sh
grep -n -A22 "penalty in Rust is" docs/RESULTS.md
```

Check that the appended text sits inside the existing bullet, that the bullet still ends before the next `- **` list item, and that no other bullet was disturbed.

- [ ] **Step 3: Commit**

```sh
git add docs/RESULTS.md
git commit -m "docs(RESULTS): note the published mvcc_* figures predate the Arc fix

Six Rust cells were measured before order_mut/level_mut stopped paying the
per-write atomic check. Their numbers are stale by a known sign -- too slow
by roughly 7-12 ns per mutable access. Quantifying it needs a same-host A/B,
since cross-instance variance on this grid would swamp the effect."
```

---

## Plan Self-Review

**Spec coverage.** The change itself → Task 1 Steps 3–4. The soundness argument → the SAFETY comments, verbatim from the spec. `debug_assert` → Steps 3–4. Existing-tests-as-verification → Step 5, with the test list named. Miri → Step 6, including the nightly invocation the spec did not specify. Measurement consequence → Task 2. The spec's "out of scope" items (Java asymmetry, Go assertion) get no task, correctly.

**One thing the spec got wrong, corrected here.** The spec says to run Miri without noting that it is unavailable on the pinned stable toolchain — `cargo miri test` fails outright. The plan specifies `cargo +nightly miri test` and states why. A nightly toolchain with the component is installed; I verified `cargo +nightly miri test -p smr-collections-common capture_carries_free_head` runs and passes before writing this.

**No TDD cycle, deliberately.** This change introduces no new behaviour, so there is no failing test to write first. The equivalent rigour is Step 2 (prove green before) and Steps 5–6 (prove green after, plus a tool that sees what the tests cannot). Writing a new test asserting `CowBook` still produces the same bytes would duplicate `cow_churn_image_matches_flat_churn_image` exactly.

**Where the risk is.** Two places. The `as_ptr` cast is the only genuinely subtle line, and Miri is the only thing in this plan that can check it — which is why Step 6 demands the report name what was and was not run under it. And Step 7 is a falsification check, not a formality: if `mvcc_churn` does not improve, the spec's diagnosis was wrong and the plan should stop rather than proceed to Task 2.
