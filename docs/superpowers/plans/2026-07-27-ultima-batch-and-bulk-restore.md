# ultima Batched-Apply Cells + bulk_load Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switch `restore_ultima` to ultima_db's O(N) `bulk_load_batch` path, and add two Rust-only batched-apply cells (`ultima_batch_insert`, `ultima_batch_update`, one write-txn per 64-command batch) to the smr-collections grid.

**Architecture:** Restore builds three sorted vecs from the SBE image and installs them atomically via `Store::bulk_load_batch` into an unseeded store (`UltimaBook::empty`). The batch cells reuse the exact per-command apply logic of the unbatched cells (extracted into `apply_insert`/`apply_update` helpers) inside one txn per batch, so the cells differ only in txn amortization. Spec: `docs/superpowers/specs/2026-07-27-ultima-batch-and-bulk-restore-design.md`.

**Tech Stack:** Rust workspace under `rust/` (bench-common + smr-collections crates), ansible matrix under `bench-infra/ansible/`. ultima_db pinned at `8ac858d` (already, from the VersionPin patch).

## Global Constraints

- Work on branch `bench/ultima-batch-and-bulk-restore` (already created; spec committed). Repo root: `/home/claude/ultima/hi-perf-cmp`.
- This repo gates formatting: `cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check` must ALL pass (unlike ultima_db, `cargo fmt --check` IS enforced here — keep new code rustfmt-clean).
- The golden image is law: `testdata/golden_snapshot.bin` byte-identity tests must keep passing; never regenerate the golden file.
- Batch default is **64** (`SMRC_APPLY_BATCH`); `SMRC_ITERS` keeps meaning *commands*.
- stdout is for result-contract lines only; logs/diagnostics go to stderr (repo-wide bench contract).
- Result emission uses the existing `bench_common` helpers only (`emit_latency`, `emit_float`, `emit_int`) — no hand-rolled JSON.

---

### Task 1: `SmrConfig.apply_batch` (bench-common + all literal sites)

**Files:**
- Modify: `rust/bench-common/src/smrcoll.rs`
- Modify (struct literals gain the new field): `rust/smr-collections/common/src/book.rs:235`, `rust/smr-collections/common/src/cowbook.rs:267`, `rust/smr-collections/common/src/snapshot.rs:165`, `rust/smr-collections/ultima-common/src/lib.rs:429`

**Interfaces:**
- Produces: `SmrConfig.apply_batch: usize` (env `SMRC_APPLY_BATCH`, default 64), validated `1 <= apply_batch <= iters`. Tasks 3–4 consume `cfg.apply_batch`.

- [ ] **Step 1: Add the field and parse**

In `rust/bench-common/src/smrcoll.rs`, after the `pub chunk: usize` field (line ~20), add:

```rust
    /// Commands per write-txn in the ultima batched-apply cells.
    pub apply_batch: usize,
```

In `from_env` after the `chunk` parse (line ~36):

```rust
        let apply_batch = parse_usize("SMRC_APPLY_BATCH", 64)?;
```

After the existing `chunk > cap` validation block (line ~51), add (match the surrounding validation style exactly — read the neighboring blocks first):

```rust
        if apply_batch == 0 || apply_batch > iters {
            return Err(format!(
                "SMRC_APPLY_BATCH must be in 1..={iters} (got {apply_batch})"
            ));
        }
```

(If the surrounding validations build errors differently — e.g. a helper or a different message shape — follow that shape instead; the bound `1..=iters` is the requirement.)

Add `apply_batch,` to the `Ok(SmrConfig { ... })` literal (line ~57).

- [ ] **Step 2: Fix the four test-cfg struct literals**

Each of these builds `SmrConfig { ... }` field-by-field and now fails to compile. Add `apply_batch: 64,` after their `chunk:` field:
- `rust/smr-collections/common/src/book.rs` (~line 235)
- `rust/smr-collections/common/src/cowbook.rs` (~line 267)
- `rust/smr-collections/common/src/snapshot.rs` (~line 165)
- `rust/smr-collections/ultima-common/src/lib.rs` (~line 429)

- [ ] **Step 3: Verify**

Run from `rust/`: `cargo test -p bench-common -p smr-collections-common -p smr-collections-ultima` then `cargo clippy --all-targets -p bench-common` and `cargo fmt --check`
Expected: all green (behavioral change is zero — only a new parsed field).

- [ ] **Step 4: Commit**

```bash
git add rust/bench-common/src/smrcoll.rs rust/smr-collections/common/src/book.rs rust/smr-collections/common/src/cowbook.rs rust/smr-collections/common/src/snapshot.rs rust/smr-collections/ultima-common/src/lib.rs
git commit -m "bench-common: SmrConfig.apply_batch (SMRC_APPLY_BATCH, default 64)"
```

---

### Task 2: `restore_ultima` via `bulk_load_batch`

**Files:**
- Modify: `rust/smr-collections/ultima-common/src/lib.rs` (`UltimaBook::empty`, `restore_ultima`)

**Interfaces:**
- Consumes: ultima_db `8ac858d` API: `Store::bulk_load_batch() -> BulkLoadBatch`, `BulkLoadBatch::add(name, BulkLoadInput::Replace(BulkSource::sorted_vec(vec)), AddOptions::default())`, `BulkLoadBatch::commit(BulkLoadOptions { create_if_missing: true, checkpoint_after: false }) -> Result<u64>` (returns the installed version).
- Produces: `restore_ultima` with identical signature/contract (`Result<UltimaBook, String>`), same CRC and validation errors. The existing tests are the spec.

- [ ] **Step 1: Add `UltimaBook::empty`**

In the `impl UltimaBook` block, directly above `pub fn new`, add a private constructor that is `new` minus the seeding txn (same `StoreConfig::builder()` chain — SingleWriter, `require_explicit_version(true)`, the retention comment stays only in `new`):

```rust
    /// Store + config only — no seeding txn. Used by `restore_ultima`,
    /// which installs the full table set via `bulk_load_batch` instead.
    fn empty(cfg: &SmrConfig) -> UltimaBook {
        let store = Store::new(
            StoreConfig::builder()
                .writer_mode(WriterMode::SingleWriter)
                .require_explicit_version(true)
                .build(),
        )
        .expect("store");
        UltimaBook {
            store: Arc::new(store),
            version: 0,
            price_min: cfg.price_min,
            tick: cfg.tick,
            n_levels: cfg.levels,
        }
    }
```

Then refactor `new` to call `Self::empty(cfg)` and keep only the seeding txn (delete its duplicated store-construction block; keep the retention comment on... the comment currently sits on the builder chain — move it to `empty`'s builder chain so it isn't lost, or keep a one-liner in `new`; either way `cargo fmt --check` clean).

- [ ] **Step 2: Rewrite the install half of `restore_ultima`**

Keep unchanged: length/CRC checks, header decode, `nLevels mismatch` check. Replace everything from `ub.version += 1; let mut wtx = ...` to the final `wtx.commit()` with vec building + one bulk install:

```rust
    let mut ub = UltimaBook::empty(cfg);

    let mut lg = dec.levels_decoder();
    let lc = lg.count();
    let empty_level = |side: u8, t: u32| LevelRec {
        side,
        tick: t,
        qty_total: 0,
        count: 0,
        head: NIL,
        tail: NIL,
    };
    let mut levels: Vec<(u64, LevelRec)> = (0..2u64 * n_levels as u64)
        .map(|i| {
            let side = (i / n_levels as u64) as u8;
            let t = (i % n_levels as u64) as u32;
            (i + 1, empty_level(side, t))
        })
        .collect();
    for _ in 0..lc {
        lg.advance().expect("advance").expect("level present");
        let side = if lg.side() == Side::ASK { 1u8 } else { 0u8 };
        let t = lg.level_tick();
        let lid = side as u64 * n_levels as u64 + t as u64 + 1;
        levels[(lid - 1) as usize] = (
            lid,
            LevelRec {
                side,
                tick: t,
                qty_total: lg.qty_total(),
                count: lg.order_count(),
                head: lg.head(),
                tail: lg.tail(),
            },
        );
    }
    let dec = lg.parent().expect("levels parent");

    let mut og = dec.orders_decoder();
    let oc = og.count();
    let mut orders: Vec<(u64, OrderRec)> = Vec::with_capacity(oc as usize);
    for _ in 0..oc {
        og.advance().expect("advance").expect("order present");
        let slot = og.slot();
        let id = slot as u64 + 1;
        if orders.len() as u64 + 1 != id {
            return Err("orders group not in slot order".into());
        }
        orders.push((
            id,
            OrderRec {
                slot,
                price: og.price(),
                qty: og.qty(),
                filled: og.filled(),
                side: if og.side() == Side::ASK { 1 } else { 0 },
                next: og.next_slot(),
                prev: og.prev(),
            },
        ));
    }

    let meta = vec![(
        1u64,
        MetaRec {
            price_min,
            tick,
            n_levels,
            capacity,
            hwm,
            best_bid,
            best_ask,
        },
    )];

    let mut batch = ub.store.bulk_load_batch();
    let add_opts = AddOptions::default();
    batch
        .add(
            "levels",
            BulkLoadInput::Replace(BulkSource::sorted_vec(levels)),
            add_opts.clone(),
        )
        .map_err(|e| format!("bulk add levels: {e}"))?;
    batch
        .add(
            "orders",
            BulkLoadInput::Replace(BulkSource::sorted_vec(orders)),
            add_opts.clone(),
        )
        .map_err(|e| format!("bulk add orders: {e}"))?;
    batch
        .add(
            "meta",
            BulkLoadInput::Replace(BulkSource::sorted_vec(meta)),
            add_opts,
        )
        .map_err(|e| format!("bulk add meta: {e}"))?;
    ub.version = batch
        .commit(BulkLoadOptions {
            create_if_missing: true,
            checkpoint_after: false,
        })
        .map_err(|e| format!("bulk commit: {e}"))?;
    Ok(ub)
```

Add the imports (extend the existing `use ultima_db::...` line): `AddOptions, BulkLoadInput, BulkLoadOptions, BulkSource`. Check the exact re-export paths with `cargo doc`-level certainty by grepping ultima_db's `src/lib.rs` `pub use bulk_load::{...}` line first; adjust paths to what is actually re-exported (e.g. `ultima_db::bulk_load::...` if not at crate root). If `AddOptions` is not `Clone`, construct it inline per call instead of `.clone()`.

Note: `hwm == 0` yields an empty `orders` vec — `sorted_vec(vec![])` must still `add` so the table exists; the round-trip test on a fresh book covers the normal path, and if empty-vec Replace errors in ultima_db, fall back to skipping the empty add ONLY if a fresh empty-book round-trip test proves `open_table("orders")` still works afterward — otherwise report BLOCKED with the error.

- [ ] **Step 3: Run the adapter tests (the golden round-trip is the gate)**

Run from `rust/`: `cargo test -p smr-collections-ultima`
Expected: all 5 tests pass, in particular `restore_round_trips_and_rejects_corruption` (byte-identity through the new path) and `ultima_matches_golden_bytes`.

- [ ] **Step 4: Quick perf sanity (dev-box, direction-only)**

Run: `cargo run --release -p smr-collections-ultima_snapshot 2>/dev/null | grep restore_mean`
Expected: restore_mean well below the prior ~8–9 ms ballpark (likely ≤ ~2–3 ms). This is a smoke direction check only — real numbers come from the fleet run.

- [ ] **Step 5: fmt/clippy + commit**

Run: `cargo clippy --all-targets -p smr-collections-ultima` and `cargo fmt --check`
```bash
git add rust/smr-collections/ultima-common/src/lib.rs
git commit -m "smr-collections: restore_ultima via bulk_load_batch (atomic O(N) install)"
```

---

### Task 3: apply helpers + `insert_batch_txn` / `update_batch_txn` + equivalence tests

**Files:**
- Modify: `rust/smr-collections/ultima-common/src/lib.rs`

**Interfaces:**
- Consumes: `WriteTx` (from `ultima_db`), existing `insert`/`update` bodies.
- Produces (Task 4 consumes): `pub fn insert_batch_txn(&mut self, cmds: &[(i64, i64, i64, u8)])` (order_id, price, qty, side) and `pub fn update_batch_txn(&mut self, cmds: &[(i64, i64)])` (order_id, fill_qty) — one explicit-version txn per call, one commit. Per-command behavior byte-identical to `insert`/`update`.

- [ ] **Step 1: Extract the helpers**

Move the body of `insert` (everything between `begin_write` and `commit`) into:

```rust
    fn apply_insert(&mut self, wtx: &mut ultima_db::WriteTx, order_id: i64, price: i64, qty: i64, side: u8) {
        // ... exact current body of insert() between begin_write and commit,
        // with `wtx` replacing the local txn binding ...
    }
```

and likewise `fn apply_update(&mut self, wtx: &mut ultima_db::WriteTx, order_id: i64, fill_qty: i64)`. (If `&mut self` + `&mut WriteTx` borrows conflict because the body reads `self.tick_of`/`self.level_id`/`self.n_levels`, take `&self` instead — the body only mutates through `wtx`; version bumping stays in the callers.) Rewrite `insert`/`update` as:

```rust
    pub fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        self.apply_insert(&mut wtx, order_id, price, qty, side);
        wtx.commit().expect("commit");
    }
```

(and the same shape for `update`). Existing tests must pass unchanged after this step — it is a pure extraction.

- [ ] **Step 2: Add the batch methods**

```rust
    /// Apply a batch of insert commands in ONE write txn (the SMR
    /// consensus-batch pattern). Per-command work is identical to
    /// `insert` — the cells differ only in txn amortization.
    pub fn insert_batch_txn(&mut self, cmds: &[(i64, i64, i64, u8)]) {
        if cmds.is_empty() {
            return;
        }
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        for &(order_id, price, qty, side) in cmds {
            self.apply_insert(&mut wtx, order_id, price, qty, side);
        }
        wtx.commit().expect("commit");
    }

    /// Batched analog of `update`; see `insert_batch_txn`.
    pub fn update_batch_txn(&mut self, cmds: &[(i64, i64)]) {
        if cmds.is_empty() {
            return;
        }
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        for &(order_id, fill_qty) in cmds {
            self.apply_update(&mut wtx, order_id, fill_qty);
        }
        wtx.commit().expect("commit");
    }
```

(Adjust `&mut self`/`&self` on the helpers per Step 1's borrow note; if the helper is `&self` these compile as written.)

- [ ] **Step 3: Golden-equivalence tests**

Add to the tests module (uses the existing `cfg()`, RNG, and workload helpers — same imports as `ultima_matches_golden_bytes`):

```rust
    #[test]
    fn batched_insert_matches_golden_bytes() {
        let c = cfg();
        let mut ub = UltimaBook::new(&c);
        let mut rng = SplitMix::new(SEED);
        let cmds: Vec<(i64, i64, i64, u8)> = (0..c.steady)
            .map(|i| {
                let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
                (ins.order_id, ins.price, ins.qty, ins.side)
            })
            .collect();
        for chunk in cmds.chunks(c.apply_batch) {
            ub.insert_batch_txn(chunk);
        }
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode_at(&ub.store, ub.current_version(), &mut buf);
        let golden = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testdata/golden_snapshot.bin"
        ))
        .expect("golden file");
        assert_eq!(&buf[..n], &golden[..], "batched apply == golden bytes");
    }

    #[test]
    fn batched_mixed_ops_match_per_op_apply() {
        let c = cfg();
        let mut a = UltimaBook::new(&c);
        let mut b = UltimaBook::new(&c);
        let mut r1 = SplitMix::new(SEED);
        let mut r2 = SplitMix::new(SEED);
        for i in 0..c.steady {
            let x = next_insert(&mut r1, i, c.levels, c.tick, c.price_min);
            a.insert(x.order_id, x.price, x.qty, x.side);
        }
        let ins: Vec<(i64, i64, i64, u8)> = (0..c.steady)
            .map(|i| {
                let x = next_insert(&mut r2, i, c.levels, c.tick, c.price_min);
                (x.order_id, x.price, x.qty, x.side)
            })
            .collect();
        for chunk in ins.chunks(c.apply_batch) {
            b.insert_batch_txn(chunk);
        }
        let mut u1 = SplitMix::new(SEED ^ 0x9e37);
        let mut u2 = SplitMix::new(SEED ^ 0x9e37);
        for _ in 0..300 {
            let x = next_update(&mut u1, c.steady);
            a.update(x.order_id, x.fill_qty);
        }
        let ups: Vec<(i64, i64)> = (0..300)
            .map(|_| {
                let x = next_update(&mut u2, c.steady);
                (x.order_id, x.fill_qty)
            })
            .collect();
        for chunk in ups.chunks(c.apply_batch) {
            b.update_batch_txn(chunk);
        }
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n1 = encode_at(&a.store, a.current_version(), &mut buf1);
        let n2 = encode_at(&b.store, b.current_version(), &mut buf2);
        assert_eq!(&buf1[..n1], &buf2[..n2], "batched == per-op bytes");
    }
```

(If `SplitMix::new(SEED ^ 0x9e37)` doesn't compile because `SEED`'s type doesn't support `^` directly, use any two identical fresh seeds for u1/u2 — the requirement is only that both books see the same update stream.)

- [ ] **Step 4: Run tests, fmt, clippy; commit**

Run from `rust/`: `cargo test -p smr-collections-ultima`, `cargo clippy --all-targets -p smr-collections-ultima`, `cargo fmt --check`
Expected: 7 tests green (5 existing + 2 new).
```bash
git add rust/smr-collections/ultima-common/src/lib.rs
git commit -m "smr-collections: apply helpers + one-txn-per-batch insert/update (golden-equivalent)"
```

---

### Task 4: cell binaries + matrix wiring + docs

**Files:**
- Create: `rust/smr-collections/ultima_batch_insert/Cargo.toml`, `rust/smr-collections/ultima_batch_insert/src/main.rs`
- Create: `rust/smr-collections/ultima_batch_update/Cargo.toml`, `rust/smr-collections/ultima_batch_update/src/main.rs`
- Modify: `rust/Cargo.toml` (workspace members, after the `live_ultima` entry ~line 38)
- Modify: `bench-infra/ansible/group_vars/all.yml` (two experiment rows after `live_ultima` ~line 46; `smrc_apply_batch: 64` after `smrc_chunk` ~line 98)
- Modify: `bench-infra/ansible/roles/run/tasks/local.yml` (~line 37: `export SMRC_APPLY_BATCH="{{ smrc_apply_batch }}"` next to `SMRC_CHUNK`)
- Modify: root `CLAUDE.md` artifact list (~line 76: extend `smr-collections-{ultima_insert,ultima_update,ultima_snapshot,live_ultima}` with `ultima_batch_insert,ultima_batch_update`)

**Interfaces:**
- Consumes: Task 1's `cfg.apply_batch`, Task 3's `insert_batch_txn`/`update_batch_txn`, `bench_common::smrcoll::{SmrConfig, emit_latency, emit_float, emit_int}` (check `emit_int`/`emit_float` exact signatures in `bench-common/src/smrcoll.rs` — `ultima_snapshot/src/main.rs` shows the call shape: `emit_int(EXPERIMENT, "snapshot_bytes", v, "bytes", 1)`).

- [ ] **Step 1: `ultima_batch_insert` package**

`Cargo.toml` — copy `rust/smr-collections/ultima_insert/Cargo.toml` verbatim, renaming package to `smr-collections-ultima_batch_insert` and the `[[bin]]` (if present) to `smr-collections-ultima_batch_insert` — mirror exactly how `ultima_insert`'s manifest names things.

`src/main.rs`:

```rust
//! smr-collections **ultima_batch_insert** — insert cost through ultima_db
//! with ONE explicit-version write-txn per `apply_batch` commands (the SMR
//! consensus-batch pattern). Compare `per_op_mean` against `ultima_insert`'s
//! `insert_mean`: the difference is pure txn amortization.

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::UltimaBook;
use std::time::Instant;

const EXPERIMENT: &str = "ultima_batch_insert";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let b = cfg.apply_batch;
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    let mut next_cmd = {
        let mut i = 0usize;
        move |rng: &mut SplitMix| {
            let ins = next_insert(rng, i, cfg.levels, cfg.tick, cfg.price_min);
            i += 1;
            (ins.order_id, ins.price, ins.qty, ins.side)
        }
    };

    let warm_batches = cfg.warmup / b;
    for _ in 0..warm_batches {
        let cmds: Vec<_> = (0..b).map(|_| next_cmd(&mut rng)).collect();
        book.insert_batch_txn(&cmds);
    }

    let batches = cfg.iters / b;
    let mut batch_ns = vec![0u64; batches];
    for w in batch_ns.iter_mut() {
        let cmds: Vec<_> = (0..b).map(|_| next_cmd(&mut rng)).collect();
        let t0 = Instant::now();
        book.insert_batch_txn(&cmds);
        *w = t0.elapsed().as_nanos() as u64;
    }

    let ops = (batches * b) as u64;
    let total: u64 = batch_ns.iter().sum();
    emit_latency(EXPERIMENT, "batch", &batch_ns);
    emit_float(
        EXPERIMENT,
        "per_op_mean",
        total as f64 / ops as f64,
        "ns",
        ops as usize,
    );
    emit_int(EXPERIMENT, "batch_size", b as u64, "count", 1);
}
```

(The command-vec build is outside the timed window deliberately — the flat/CoW cells also generate the command outside `measure()`'s closure? Check `ultima_insert/src/main.rs`: its generator runs INSIDE the timed closure. To stay comparable, move `let cmds = ...collect()` INSIDE the timing if and only if `ultima_insert` times generation too — it does (`measure(...)` closure calls `next_insert`). So: build `cmds` inside the `t0` window, i.e. move `let t0 = Instant::now();` above the `let cmds` line. Per-command generation cost is ~ns-scale and identical across cells either way; match `ultima_insert` for strict comparability.)

Apply that correction: final code times `cmds` generation + batch apply, matching the unbatched cell's timed content.

- [ ] **Step 2: `ultima_batch_update` package**

Same manifest pattern. `src/main.rs` mirrors `ultima_update/src/main.rs`'s structure (steady-phase: `cfg.steady` per-op inserts to populate the book — copy that phase from `ultima_update`'s main verbatim), then warmup/timed batched updates:

```rust
//! smr-collections **ultima_batch_update** — update cost through ultima_db
//! with ONE write-txn per `apply_batch` commands; see ultima_batch_insert.

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::UltimaBook;
use std::time::Instant;

const EXPERIMENT: &str = "ultima_batch_update";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let b = cfg.apply_batch;
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;

    let warm_batches = cfg.warmup / b;
    for _ in 0..warm_batches {
        let cmds: Vec<(i64, i64)> = (0..b)
            .map(|_| {
                let u = next_update(&mut rng, n);
                (u.order_id, u.fill_qty)
            })
            .collect();
        book.update_batch_txn(&cmds);
    }

    let batches = cfg.iters / b;
    let mut batch_ns = vec![0u64; batches];
    for w in batch_ns.iter_mut() {
        let t0 = Instant::now();
        let cmds: Vec<(i64, i64)> = (0..b)
            .map(|_| {
                let u = next_update(&mut rng, n);
                (u.order_id, u.fill_qty)
            })
            .collect();
        book.update_batch_txn(&cmds);
        *w = t0.elapsed().as_nanos() as u64;
    }

    let ops = (batches * b) as u64;
    let total: u64 = batch_ns.iter().sum();
    emit_latency(EXPERIMENT, "batch", &batch_ns);
    emit_float(
        EXPERIMENT,
        "per_op_mean",
        total as f64 / ops as f64,
        "ns",
        ops as usize,
    );
    emit_int(EXPERIMENT, "batch_size", b as u64, "count", 1);
}
```

(Same comparability rule as Step 1: generation inside the timed window. Check `ultima_update`'s exact update-workload call shape — if `next_update` returns different field names, mirror what `ultima_update/src/main.rs` does. Note `ultima_batch_insert` uses a closure for indexed `next_insert`; if that closure shape fights the borrow checker, use a plain `i` counter mutated in the loop like `ultima_insert/src/main.rs` does.)

- [ ] **Step 3: Workspace + matrix wiring**

`rust/Cargo.toml` members (after `"smr-collections/live_ultima",`):
```
    "smr-collections/ultima_batch_insert",
    "smr-collections/ultima_batch_update",
```

`bench-infra/ansible/group_vars/all.yml` — after the `live_ultima` row:
```yaml
  - { focus_area: smr-collections,  experiment: ultima_batch_insert, kind: local, languages: [rust] }
  - { focus_area: smr-collections,  experiment: ultima_batch_update, kind: local, languages: [rust] }
```
and after `smrc_chunk: 4096`:
```yaml
smrc_apply_batch: 64
```

`bench-infra/ansible/roles/run/tasks/local.yml` — next to the `SMRC_CHUNK` export:
```
    export SMRC_APPLY_BATCH="{{ smrc_apply_batch }}"
```

Root `CLAUDE.md` (~line 76): change `smr-collections-{ultima_insert,ultima_update,ultima_snapshot,live_ultima}` to `smr-collections-{ultima_insert,ultima_update,ultima_snapshot,live_ultima,ultima_batch_insert,ultima_batch_update}`.

- [ ] **Step 4: Build, run both cells locally, full workspace gates**

From `rust/`:
- `cargo build --release -p smr-collections-ultima_batch_insert -p smr-collections-ultima_batch_update`
- `cargo run --release -p smr-collections-ultima_batch_insert 2>/dev/null` — expect 5 JSON lines (`batch_mean/p50/p99`, `per_op_mean`, `batch_size`), `per_op_mean` well under the unbatched ~6–8 µs dev-box figure.
- Same for `..._batch_update`.
- Full gates: `cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check`

- [ ] **Step 5: Commit**

```bash
git add rust/Cargo.toml rust/Cargo.lock rust/smr-collections/ultima_batch_insert rust/smr-collections/ultima_batch_update bench-infra/ansible/group_vars/all.yml bench-infra/ansible/roles/run/tasks/local.yml CLAUDE.md
git commit -m "smr-collections: ultima_batch_insert/update cells (one txn per 64-command batch)"
```

---

### Task 5 (controller, not a subagent): scoped fleet run + RESULTS.md

Authorized by Peter. Controller runs it directly (billable infra + doc synthesis):
1. Scoped extra-vars: the six ultima cells only (`ultima_insert`, `ultima_update`, `ultima_snapshot`, `live_ultima`, `ultima_batch_insert`, `ultima_batch_update`, all rust-only rows).
2. `make up` → `ansible-playbook bench.yml -e @scoped` → **`make destroy` unconditionally** → `make status`.
3. `journal record` + `journal compare` vs 20260727T134311Z.
4. RESULTS.md: add the batch cells to the smr-collections section (new "batched apply" rows/paragraph bracketing the trade), update the restore figure and the restore-flag bullet, add the run-index row.
5. Commit; report.
