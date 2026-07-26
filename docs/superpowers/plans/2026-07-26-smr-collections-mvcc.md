# smr-collections MVCC Variants + ultima_db Cell — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a chunked copy-on-write (MVCC) variant of the smr-collections limit-order-book in Rust/Go/Java, a "snapshot under live writes" experiment family for all store variants, and an ultima_db competitor cell (Rust), per the approved spec `docs/superpowers/specs/2026-07-26-smr-collections-mvcc-design.md`.

**Architecture:** The existing stop-the-world `Book` stays untouched as the baseline. A new `CowBook` keeps the same flat layout but splits the order pool and ladder into fixed-size chunks behind a chunk table; a snapshot is an O(#chunks) root capture at an op boundary (epoch-based copy decision, language-native reclamation), and a serializer encodes the frozen root to the existing SBE `book_snapshot` format **byte-identically**, so the existing golden file verifies every new store. ultima_db is used through its SMR pattern (SingleWriter, explicit versions, snapshot = read-txn at a version) with two custom-keyed-by-construction tables whose id-ordered iteration reproduces the golden byte order.

**Tech Stack:** Rust 1.96 (edition 2024, existing `booksnap-codec` SBE crate, `crc32c`), Go 1.22 (existing `booksnap` SBE package), Java 21 + Agrona 1.21.0 (existing generated `booksnap` SBE), ultima_db pinned git dep, Ansible matrix in `bench-infra/`.

## Global Constraints

- **Result contract:** stdout is result-contract JSON lines ONLY (one line per metric); logs to stderr. Emit through the per-language bench library (`bench_common::smrcoll` / `internal/bench` / `net.knego.hiperf.common.SmrCollections`), never hand-rolled JSON. Focus area is exactly `smr-collections`; the `experiment` field is exactly the experiment name (`mvcc_insert`, `mvcc_update`, `mvcc_snapshot`, `ultima_insert`, `ultima_update`, `ultima_snapshot`, `live_stw`, `live_mvcc`, `live_ultima`).
- **Byte identity:** every new store's snapshot encoding must be byte-identical to the STW encoder for the same logical state. The pinned artifact is `rust/smr-collections/testdata/golden_snapshot.bin` (93,256 bytes; produced at `cap=4096, levels=64, tick=1, priceMin=0, steady=2000`). Never regenerate it in this plan.
- **Workload:** the existing splitmix64 stream (`SEED = 0x123456789ABCDEF0`) and draw functions are reused verbatim. No cancel/remove op is added.
- **New env vars (identical parsing in all three languages, hard-error on malformed):** `SMRC_CHUNK` default 4096 (must be ≤ `SMRC_CAP`), `SMRC_LIVE_ITERS` default 200000, `SMRC_SNAP_EVERY` default 20000 (must be ≤ `SMRC_LIVE_ITERS`). All strictly positive.
- **Existing cells untouched:** do not modify `Book`'s logic, the STW encoders' output, or the `insert`/`update`/`snapshot` artifacts (visibility-only changes to `book.rs` are allowed).
- **ultima_db dep:** `ultima_db = { git = "https://github.com/PeterKnego/ultima_db.git", rev = "b48295e9ad6ba6e54100a6e8ec9248c8e84e09c3" }`, declared in `[workspace.dependencies]` but referenced ONLY by the ultima artifact crates. Do NOT enable its `persistence` feature.
- **Quality gates:** Rust `cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check` (run from `rust/`); Go `go build ./... && go vet ./... && go test ./...` plus `go test -race ./internal/smrcoll/` for concurrency tests (from `go/`); Java `./gradlew build` via the checked-in wrapper (from `java/`).
- **Trigger rule (all live experiments):** after the untimed steady build and `SMRC_WARMUP` untimed warmup updates, run `SMRC_LIVE_ITERS` timed updates; at each op index `k` where `k % SMRC_SNAP_EVERY == 0` a snapshot is triggered *inside* op `k`'s timed window (defaults → 10 triggers). If the serializer is still busy, no capture happens and `snap_skipped` increments.
- **Live metrics (every live artifact emits exactly):** `writer_p50`/`writer_p99`/`writer_mean` (via the latency emit helper, prefix `writer`), `writer_max` (int, ns), `snapshot_p50`/`snapshot_p99`/`snapshot_mean` (prefix `snapshot`, over completed snapshots), `snap_count` (int, unit `count`, samples=1), `snap_skipped` (int, unit `count`, samples=1), `snapshot_bytes` (int, unit `bytes`, samples=1).

## File Structure

**Rust** (all under `rust/`):
- Modify `bench-common/src/smrcoll.rs` — 3 new `SmrConfig` fields.
- Modify `smr-collections/common/src/book.rs` — make `IdMap` and `PriceLevel::EMPTY` public (visibility only).
- Create `smr-collections/common/src/cowbook.rs` — `CowBook`, `Root`, chunks, capture.
- Create `smr-collections/common/src/cowsnap.rs` — `encode_root`, `restore_cow` + tests (equivalence, golden, capture isolation, concurrent capture).
- Create `smr-collections/{mvcc_insert,mvcc_update,mvcc_snapshot,live_stw,live_mvcc}/` — 5 bin crates.
- Create `smr-collections/ultima-common/` — lib crate `smr-collections-ultima` (records, `UltimaBook`, `encode_at`, `restore_ultima`) + tests.
- Create `smr-collections/{ultima_insert,ultima_update,ultima_snapshot,live_ultima}/` — 4 bin crates.
- Modify `Cargo.toml` — 10 new members + `ultima_db` workspace dep.

**Go** (all under `go/`):
- Modify `internal/bench/smrcoll.go` — 3 new `SmrConfig` fields.
- Create `internal/smrcoll/cowbook.go`, `internal/smrcoll/cowsnapshot.go`, tests `cowbook_test.go`, `cowsnapshot_test.go`.
- Create `cmd/smr-collections-{mvcc_insert,mvcc_update,mvcc_snapshot,live_stw,live_mvcc}/main.go`.

**Java** (all under `java/`):
- Modify `common/src/main/java/net/knego/hiperf/common/SmrConfig.java` — 3 new record components.
- Create `smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/CowBook.java`, `CowRoot.java`, `CowSnapshotter.java` + tests `CowBookTest.java`, `CowSnapshotTest.java`.
- Create `smr-collections-{mvcc_insert,mvcc_update,mvcc_snapshot,live_stw,live_mvcc}/` subprojects.
- Modify `settings.gradle.kts`.

**Infra/docs:** `bench-infra/ansible/group_vars/all.yml`, `bench-infra/ansible/roles/run/tasks/local.yml`, `CLAUDE.md`, `README.md`.

Suggested execution order: R1→R7 (Rust is the reference; the golden/live semantics get pinned here), then G1→G5, J1→J5 (independent of each other, parallelizable), then I1.

---

### Task R1: Rust config — extend `SmrConfig`

**Files:**
- Modify: `rust/bench-common/src/smrcoll.rs`
- Modify: `rust/smr-collections/common/src/book.rs` (test `cfg()` literal)
- Modify: `rust/smr-collections/common/src/snapshot.rs` (test `cfg()` + golden-export literals)

**Interfaces:**
- Produces: `SmrConfig` gains `pub chunk: usize`, `pub live_iters: usize`, `pub snap_every: usize` (parsed from `SMRC_CHUNK`/`SMRC_LIVE_ITERS`/`SMRC_SNAP_EVERY`, defaults 4096/200000/20000). All later Rust tasks construct `SmrConfig` with all 10 fields.

- [ ] **Step 1: Write the failing test** — append to the bottom of `rust/bench-common/src/smrcoll.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Serialize env-var mutation: cargo runs tests in parallel within a binary.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn smrc_new_fields_default() {
        let _g = ENV_LOCK.lock().unwrap();
        for k in ["SMRC_CHUNK", "SMRC_LIVE_ITERS", "SMRC_SNAP_EVERY"] {
            unsafe { std::env::remove_var(k) };
        }
        let c = SmrConfig::from_env().expect("defaults parse");
        assert_eq!(c.chunk, 4096);
        assert_eq!(c.live_iters, 200_000);
        assert_eq!(c.snap_every, 20_000);
    }

    #[test]
    fn smrc_snap_every_must_not_exceed_live_iters() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("SMRC_LIVE_ITERS", "1000");
            std::env::set_var("SMRC_SNAP_EVERY", "2000");
        }
        assert!(SmrConfig::from_env().is_err());
        unsafe {
            std::env::remove_var("SMRC_LIVE_ITERS");
            std::env::remove_var("SMRC_SNAP_EVERY");
        }
    }

    #[test]
    fn smrc_chunk_must_not_exceed_cap() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SMRC_CHUNK", "999999999") };
        assert!(SmrConfig::from_env().is_err());
        unsafe { std::env::remove_var("SMRC_CHUNK") };
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rust && cargo test -p bench-common smrc_`
Expected: COMPILE FAIL (`no field 'chunk' on SmrConfig`).

- [ ] **Step 3: Implement** — in `SmrConfig` add the three fields after `iters`:

```rust
    pub iters: usize,
    /// Orders per CoW chunk (CowBook only).
    pub chunk: usize,
    /// Timed writer ops in the live_* experiments.
    pub live_iters: usize,
    /// Ops between snapshot triggers in the live_* experiments.
    pub snap_every: usize,
```

In `from_env`, after the `iters` parse:

```rust
        let chunk = parse_usize("SMRC_CHUNK", 4_096)?;
        let live_iters = parse_usize("SMRC_LIVE_ITERS", 200_000)?;
        let snap_every = parse_usize("SMRC_SNAP_EVERY", 20_000)?;
```

after the existing `warmup + iters` check:

```rust
        if chunk > cap {
            return Err("SMRC_CHUNK must be <= SMRC_CAP".into());
        }
        if snap_every > live_iters {
            return Err("SMRC_SNAP_EVERY must be <= SMRC_LIVE_ITERS".into());
        }
```

and add `chunk, live_iters, snap_every` to the `Ok(SmrConfig { ... })` literal. Then fix the three struct literals in `smr-collections/common`: the `cfg()` helpers in `book.rs` and `snapshot.rs` tests and the config inside `export_golden_when_requested` each gain `chunk: 4096, live_iters: 200_000, snap_every: 20_000`.

- [ ] **Step 4: Run to verify pass**

Run: `cd rust && cargo test -p bench-common && cargo test -p smr-collections-common`
Expected: PASS (all existing + 3 new tests).

- [ ] **Step 5: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/bench-common rust/smr-collections/common
git commit -m "feat(smr-collections): SMRC_CHUNK/SMRC_LIVE_ITERS/SMRC_SNAP_EVERY config"
```

---

### Task R2: Rust `CowBook` core (chunked CoW store + capture)

**Files:**
- Modify: `rust/smr-collections/common/src/book.rs` (visibility only)
- Create: `rust/smr-collections/common/src/cowbook.rs`
- Modify: `rust/smr-collections/common/src/lib.rs`

**Interfaces:**
- Consumes: `SmrConfig` (R1), `book::{Order, PriceLevel, NIL, IdMap}`.
- Produces: `CowBook::new(&SmrConfig)`, `insert(order_id: i64, price: i64, qty: i64, side: u8)`, `update(order_id: i64, fill_qty: i64)`, `capture(&mut self) -> Root`, read accessors `level(&self, side: u8, t: u32) -> &PriceLevel`, `order(&self, slot: u32) -> &Order`, `get_slot(&self, order_id: i64) -> u32`, `level_qty(&self, side: u8, tick: u32) -> i64`; `Root` with pub scalars (`price_min, tick, n_levels, capacity, hwm, best_bid, best_ask, chunk`) and accessors `Root::level(&self, side: u8, t: u32) -> &PriceLevel`, `Root::order(&self, slot: u32) -> &Order`. `Root` is `Send`. `pub const LEVEL_CHUNK: usize = 256`. Writer-side mutable accessors `order_mut`/`level_mut` are `pub(crate)` (used by `cowsnap::restore_cow` in R3).

- [ ] **Step 1: Visibility tweaks in `book.rs`** — change `type IdMap = ...` to `pub type IdMap = ...`, and in `impl PriceLevel` change `const EMPTY` to `pub const EMPTY`. No logic changes.

- [ ] **Step 2: Write `cowbook.rs` with failing tests** — create the file:

```rust
//! Chunked copy-on-write LOB (`CowBook`): same logical behavior as `Book`,
//! but the order pool and ladder live in fixed-size chunks behind a chunk
//! table. A snapshot is an O(#chunks) `capture()` at an op boundary; the
//! writer copies a chunk before its first write after a capture (epoch
//! check `born < gen`), so a frozen `Root` is never mutated. Reclamation is
//! `Arc`; the copy decision is ALWAYS the epoch, never the refcount.

use crate::book::{IdMap, NIL, Order, PriceLevel};
use bench_common::smrcoll::SmrConfig;
use std::hash::BuildHasherDefault;
use std::sync::Arc;

/// Price levels per ladder chunk (fixed; orders-per-chunk is `SMRC_CHUNK`).
pub const LEVEL_CHUNK: usize = 256;

pub struct OrderChunk {
    born: u64,
    orders: Vec<Order>,
}

pub struct LevelChunk {
    born: u64,
    levels: Vec<PriceLevel>,
}

/// A frozen point-in-time view: chunk refs + scalars. `Send` by construction
/// (all shared state is behind `Arc`, never mutated after capture).
pub struct Root {
    pub price_min: i64,
    pub tick: i64,
    pub n_levels: u32,
    pub capacity: u32,
    pub hwm: u32,
    pub best_bid: i32,
    pub best_ask: i32,
    pub chunk: usize,
    order_chunks: Vec<Arc<OrderChunk>>,
    bid_chunks: Vec<Arc<LevelChunk>>,
    ask_chunks: Vec<Arc<LevelChunk>>,
}

impl Root {
    #[inline]
    pub fn order(&self, slot: u32) -> &Order {
        &self.order_chunks[slot as usize / self.chunk].orders[slot as usize % self.chunk]
    }

    #[inline]
    pub fn level(&self, side: u8, t: u32) -> &PriceLevel {
        let lane = if side == 0 {
            &self.bid_chunks
        } else {
            &self.ask_chunks
        };
        &lane[t as usize / LEVEL_CHUNK].levels[t as usize % LEVEL_CHUNK]
    }
}

pub struct CowBook {
    pub price_min: i64,
    pub tick: i64,
    pub n_levels: u32,
    pub capacity: u32,
    pub chunk: usize,
    /// Bumped on every capture; chunks with `born < gen` are frozen (shared
    /// with some root) and must be copied before the next write.
    gen: u64,
    order_chunks: Vec<Arc<OrderChunk>>,
    bid_chunks: Vec<Arc<LevelChunk>>,
    ask_chunks: Vec<Arc<LevelChunk>>,
    pub hwm: u32,
    pub best_bid: i32,
    pub best_ask: i32,
    pub(crate) idmap: IdMap,
}

impl CowBook {
    pub fn new(cfg: &SmrConfig) -> CowBook {
        let chunk = cfg.chunk;
        let zero = Order {
            order_id: 0,
            price: 0,
            qty: 0,
            filled: 0,
            next: NIL,
            prev: NIL,
            side: 0,
        };
        let order_chunks = (0..cfg.cap.div_ceil(chunk))
            .map(|ci| {
                let len = chunk.min(cfg.cap - ci * chunk);
                Arc::new(OrderChunk {
                    born: 1,
                    orders: vec![zero; len],
                })
            })
            .collect();
        let mk_lane = || {
            (0..(cfg.levels as usize).div_ceil(LEVEL_CHUNK))
                .map(|ci| {
                    let len = LEVEL_CHUNK.min(cfg.levels as usize - ci * LEVEL_CHUNK);
                    Arc::new(LevelChunk {
                        born: 1,
                        levels: vec![PriceLevel::EMPTY; len],
                    })
                })
                .collect()
        };
        CowBook {
            price_min: cfg.price_min,
            tick: cfg.tick,
            n_levels: cfg.levels,
            capacity: cfg.cap as u32,
            chunk,
            gen: 1,
            order_chunks,
            bid_chunks: mk_lane(),
            ask_chunks: mk_lane(),
            hwm: 0,
            best_bid: -1,
            best_ask: -1,
            idmap: IdMap::with_capacity_and_hasher(cfg.cap, BuildHasherDefault::default()),
        }
    }

    #[inline]
    fn tick_of(&self, price: i64) -> u32 {
        ((price - self.price_min) / self.tick) as u32
    }

    #[inline]
    pub fn order(&self, slot: u32) -> &Order {
        &self.order_chunks[slot as usize / self.chunk].orders[slot as usize % self.chunk]
    }

    #[inline]
    pub fn level(&self, side: u8, t: u32) -> &PriceLevel {
        let lane = if side == 0 {
            &self.bid_chunks
        } else {
            &self.ask_chunks
        };
        &lane[t as usize / LEVEL_CHUNK].levels[t as usize % LEVEL_CHUNK]
    }

    #[inline]
    pub(crate) fn order_mut(&mut self, slot: u32) -> &mut Order {
        let ci = slot as usize / self.chunk;
        if self.order_chunks[ci].born < self.gen {
            self.order_chunks[ci] = Arc::new(OrderChunk {
                born: self.gen,
                orders: self.order_chunks[ci].orders.clone(),
            });
        }
        let off = slot as usize % self.chunk;
        &mut Arc::get_mut(&mut self.order_chunks[ci])
            .expect("current-gen chunk is unshared")
            .orders[off]
    }

    #[inline]
    pub(crate) fn level_mut(&mut self, side: u8, t: u32) -> &mut PriceLevel {
        let gen = self.gen;
        let lane = if side == 0 {
            &mut self.bid_chunks
        } else {
            &mut self.ask_chunks
        };
        let ci = t as usize / LEVEL_CHUNK;
        if lane[ci].born < gen {
            lane[ci] = Arc::new(LevelChunk {
                born: gen,
                levels: lane[ci].levels.clone(),
            });
        }
        &mut Arc::get_mut(&mut lane[ci])
            .expect("current-gen chunk is unshared")
            .levels[t as usize % LEVEL_CHUNK]
    }

    /// Same op semantics as `Book::insert` (keep in lockstep).
    pub fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        let t = self.tick_of(price);
        let slot = self.hwm;
        self.hwm += 1;
        let prev_tail = self.level(side, t).tail;
        *self.order_mut(slot) = Order {
            order_id,
            price,
            qty,
            filled: 0,
            next: NIL,
            prev: prev_tail,
            side,
        };
        {
            let lvl = self.level_mut(side, t);
            if lvl.tail == NIL {
                lvl.head = slot;
            }
            lvl.tail = slot;
            lvl.qty_total += qty;
            lvl.count += 1;
        }
        if prev_tail != NIL {
            self.order_mut(prev_tail).next = slot;
        }
        self.idmap.insert(order_id, slot);
        if side == 0 && (self.best_bid < 0 || t as i32 > self.best_bid) {
            self.best_bid = t as i32;
        }
        if side == 1 && (self.best_ask < 0 || (t as i32) < self.best_ask) {
            self.best_ask = t as i32;
        }
    }

    /// Same op semantics as `Book::update` (keep in lockstep).
    pub fn update(&mut self, order_id: i64, fill_qty: i64) {
        let slot = self.idmap[&order_id];
        let (side, price, add) = {
            let o = self.order_mut(slot);
            let add = fill_qty.min(o.qty - o.filled);
            o.filled += add;
            (o.side, o.price, add)
        };
        let t = self.tick_of(price);
        self.level_mut(side, t).qty_total -= add;
    }

    /// Freeze the current state: clone the chunk-ref tables (O(#chunks)) and
    /// bump the generation so the writer copies-on-write from here on.
    pub fn capture(&mut self) -> Root {
        let root = Root {
            price_min: self.price_min,
            tick: self.tick,
            n_levels: self.n_levels,
            capacity: self.capacity,
            hwm: self.hwm,
            best_bid: self.best_bid,
            best_ask: self.best_ask,
            chunk: self.chunk,
            order_chunks: self.order_chunks.clone(),
            bid_chunks: self.bid_chunks.clone(),
            ask_chunks: self.ask_chunks.clone(),
        };
        self.gen += 1;
        root
    }

    pub fn get_slot(&self, order_id: i64) -> u32 {
        self.idmap[&order_id]
    }

    pub fn level_qty(&self, side: u8, tick: u32) -> i64 {
        self.level(side, tick).qty_total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::book::workload::{next_insert, next_update};
    use crate::rng::{SEED, SplitMix};

    fn cfg() -> SmrConfig {
        SmrConfig {
            cap: 1024,
            levels: 300, // > LEVEL_CHUNK so the ladder spans 2 chunks
            tick: 1,
            price_min: 0,
            steady: 500,
            warmup: 0,
            iters: 0,
            chunk: 64, // small so the pool spans many chunks
            live_iters: 200_000,
            snap_every: 20_000,
        }
    }

    /// Drive Book and CowBook with the identical op stream; queries must agree.
    #[test]
    fn cowbook_matches_book_queries_after_mixed_ops() {
        let c = cfg();
        let mut b = Book::new(&c);
        let mut cb = CowBook::new(&c);
        let mut r1 = SplitMix::new(SEED);
        let mut r2 = SplitMix::new(SEED);
        for i in 0..c.steady {
            let a = next_insert(&mut r1, i, c.levels, c.tick, c.price_min);
            let x = next_insert(&mut r2, i, c.levels, c.tick, c.price_min);
            b.insert(a.order_id, a.price, a.qty, a.side);
            cb.insert(x.order_id, x.price, x.qty, x.side);
        }
        for _ in 0..1000 {
            let a = next_update(&mut r1, c.steady);
            let x = next_update(&mut r2, c.steady);
            b.update(a.order_id, a.fill_qty);
            cb.update(x.order_id, x.fill_qty);
        }
        assert_eq!(cb.hwm, b.hwm());
        assert_eq!(cb.best_bid, b.best_bid());
        assert_eq!(cb.best_ask, b.best_ask());
        for id in 1..=c.steady as i64 {
            assert_eq!(cb.get_slot(id), b.get_slot(id));
        }
        for t in 0..c.levels {
            assert_eq!(cb.level_qty(0, t), b.level_qty(0, t));
            assert_eq!(cb.level_qty(1, t), b.level_qty(1, t));
        }
        for slot in 0..cb.hwm {
            let (co, bo) = (cb.order(slot), &b.pool[slot as usize]);
            assert_eq!(
                (co.order_id, co.price, co.qty, co.filled, co.next, co.prev, co.side),
                (bo.order_id, bo.price, bo.qty, bo.filled, bo.next, bo.prev, bo.side)
            );
        }
    }

    /// A captured root must not see writes made after the capture.
    #[test]
    fn capture_isolates_root_from_later_writes() {
        let c = cfg();
        let mut cb = CowBook::new(&c);
        for i in 0..c.steady {
            // deterministic direct inserts: order i+1 at tick i%levels
            cb.insert(i as i64 + 1, (i % c.levels as usize) as i64, 10, (i % 2) as u8);
        }
        let root = cb.capture();
        let before_filled = root.order(5).filled;
        let before_qty = root.level(root.order(5).side, 5 % c.levels).qty_total;
        // Mutate the live book: fill order 6 (slot 5) heavily.
        cb.update(6, 7);
        assert_eq!(root.order(5).filled, before_filled, "root frozen");
        assert_eq!(cb.order(5).filled, before_filled + 7, "writer advanced");
        let t = ((cb.order(5).price - c.price_min) / c.tick) as u32;
        assert_eq!(root.level(cb.order(5).side, t).qty_total, before_qty);
    }

    /// Two captures in a row: second root sees writes between the captures.
    #[test]
    fn successive_captures_see_successive_states() {
        let c = cfg();
        let mut cb = CowBook::new(&c);
        cb.insert(1, 5, 10, 0);
        let r1 = cb.capture();
        cb.update(1, 4);
        let r2 = cb.capture();
        assert_eq!(r1.order(0).filled, 0);
        assert_eq!(r2.order(0).filled, 4);
    }
}
```

Register the module in `lib.rs`:

```rust
pub mod book;
pub mod cowbook;
pub mod rng;
pub mod snapshot;
```

- [ ] **Step 3: Run to verify the new tests pass** (the implementation is written with the tests; the failure gate here is compile+behavior)

Run: `cd rust && cargo test -p smr-collections-common cowbook`
Expected: PASS (3 tests). If `capture_isolates_root_from_later_writes` fails, the CoW epoch check is wrong — fix `order_mut`/`level_mut`, do not touch the test.

- [ ] **Step 4: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/smr-collections/common
git commit -m "feat(smr-collections): CowBook chunked copy-on-write store (Rust)"
```

---

### Task R3: Rust CoW snapshot codec (`encode_root` / `restore_cow`) + golden

**Files:**
- Create: `rust/smr-collections/common/src/cowsnap.rs`
- Modify: `rust/smr-collections/common/src/lib.rs`

**Interfaces:**
- Consumes: `cowbook::{CowBook, Root, LEVEL_CHUNK}` (R2), the `booksnap` codec crate exactly as `snapshot.rs` uses it.
- Produces: `pub fn encode_root(root: &Root, buf: &mut [u8]) -> usize` (SBE bytes + crc32c trailer, byte-identical to `snapshot::encode` for the same logical state) and `pub fn restore_cow(bytes: &[u8], cfg: &SmrConfig) -> Result<CowBook, String>`.

- [ ] **Step 1: Create `cowsnap.rs`** — the encoder mirrors `snapshot::encode` field-for-field (kept as a separate function so the measured STW path is untouched); the decoder mirrors `snapshot::restore` but writes through the CowBook chunk accessors:

```rust
//! SBE snapshot codec over a frozen `Root` (CoW variant). Byte-identical to
//! `snapshot::encode` for the same logical state — enforced by the golden and
//! equivalence tests below. Kept separate from `snapshot.rs` so the measured
//! STW encode path is untouched.

use crate::book::NIL;
use crate::cowbook::{CowBook, Root};
use bench_common::smrcoll::SmrConfig;
use booksnap::book_snapshot_codec::encoder::{LevelsEncoder, OrdersEncoder};
use booksnap::book_snapshot_codec::{BookSnapshotDecoder, BookSnapshotEncoder};
use booksnap::message_header_codec::MessageHeaderDecoder;
use booksnap::side::Side;
use booksnap::{Encoder, ReadBuf, WriteBuf};

const HEADER_LEN: usize = 8;

fn side_enum(side: u8) -> Side {
    if side == 0 { Side::BID } else { Side::ASK }
}
fn side_u8(s: Side) -> u8 {
    match s {
        Side::ASK => 1,
        _ => 0,
    }
}

/// Encode a frozen root into `buf`; returns SBE length + 4 (crc32c trailer).
pub fn encode_root(root: &Root, buf: &mut [u8]) -> usize {
    let mut level_count = 0u16;
    for side in 0..2u8 {
        for t in 0..root.n_levels {
            if root.level(side, t).head != NIL {
                level_count += 1;
            }
        }
    }
    let sbe_len = {
        let enc = BookSnapshotEncoder::default().wrap(WriteBuf::new(buf), HEADER_LEN);
        let mut header = enc.header(0);
        let mut enc = header.parent().expect("header parent");
        enc.price_min(root.price_min);
        enc.tick_size(root.tick);
        // NOTE: `nl_evels` is the SBE codegen's odd snake_casing of `nLevels`
        // (same as snapshot.rs).
        enc.nl_evels(root.n_levels);
        enc.capacity(root.capacity);
        enc.hwm(root.hwm);
        enc.best_bid(root.best_bid);
        enc.best_ask(root.best_ask);

        let mut lg = enc.levels_encoder(level_count, LevelsEncoder::default());
        for side in 0..2u8 {
            for t in 0..root.n_levels {
                let lvl = root.level(side, t);
                if lvl.head == NIL {
                    continue;
                }
                lg.advance().expect("levels advance");
                lg.side(side_enum(side));
                lg.level_tick(t);
                lg.qty_total(lvl.qty_total);
                lg.order_count(lvl.count);
                lg.head(lvl.head);
                lg.tail(lvl.tail);
            }
        }
        let enc = lg.parent().expect("levels parent");

        let mut og = enc.orders_encoder(root.hwm as u16, OrdersEncoder::default());
        for slot in 0..root.hwm {
            let o = root.order(slot);
            og.advance().expect("orders advance");
            og.slot(slot);
            og.order_id(o.order_id);
            og.price(o.price);
            og.qty(o.qty);
            og.filled(o.filled);
            og.side(side_enum(o.side));
            og.next_slot(o.next);
            og.prev(o.prev);
        }
        og.get_limit()
    };
    let crc = crc32c::crc32c(&buf[..sbe_len]);
    buf[sbe_len..sbe_len + 4].copy_from_slice(&crc.to_le_bytes());
    sbe_len + 4
}

/// Restore a fresh CowBook from an encoded image; verifies the crc32c trailer.
pub fn restore_cow(bytes: &[u8], cfg: &SmrConfig) -> Result<CowBook, String> {
    if bytes.len() < 4 {
        return Err("snapshot too short".into());
    }
    let sbe_len = bytes.len() - 4;
    let want = u32::from_le_bytes(bytes[sbe_len..].try_into().unwrap());
    if crc32c::crc32c(&bytes[..sbe_len]) != want {
        return Err("crc32c mismatch".into());
    }
    let mut book = CowBook::new(cfg);
    let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(&bytes[..sbe_len]), 0);
    let dec = BookSnapshotDecoder::default().header(header, 0);
    book.price_min = dec.price_min();
    book.tick = dec.tick_size();
    book.n_levels = dec.nl_evels();
    book.hwm = dec.hwm();
    book.best_bid = dec.best_bid();
    book.best_ask = dec.best_ask();

    let mut lg = dec.levels_decoder();
    let lc = lg.count();
    for _ in 0..lc {
        lg.advance().expect("advance").expect("level present");
        let side = side_u8(lg.side());
        let t = lg.level_tick();
        let (head, tail, qty_total, count) = (lg.head(), lg.tail(), lg.qty_total(), lg.order_count());
        let lvl = book.level_mut(side, t);
        lvl.head = head;
        lvl.tail = tail;
        lvl.qty_total = qty_total;
        lvl.count = count;
    }
    let dec = lg.parent().expect("levels parent");

    let mut og = dec.orders_decoder();
    let oc = og.count();
    for _ in 0..oc {
        og.advance().expect("advance").expect("order present");
        let slot = og.slot();
        let (order_id, price, qty, filled) = (og.order_id(), og.price(), og.qty(), og.filled());
        let (side, next, prev) = (side_u8(og.side()), og.next_slot(), og.prev());
        let o = book.order_mut(slot);
        o.order_id = order_id;
        o.price = price;
        o.qty = qty;
        o.filled = filled;
        o.side = side;
        o.next = next;
        o.prev = prev;
        book.idmap.insert(order_id, slot);
    }
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::book::workload::{next_insert, next_update};
    use crate::rng::{SEED, SplitMix};
    use crate::snapshot;

    fn golden_cfg() -> SmrConfig {
        SmrConfig {
            cap: 4096,
            levels: 64,
            tick: 1,
            price_min: 0,
            steady: 2000,
            warmup: 0,
            iters: 0,
            chunk: 512, // several chunks even at golden scale
            live_iters: 200_000,
            snap_every: 20_000,
        }
    }

    fn build_cow(c: &SmrConfig, n: usize) -> CowBook {
        let mut b = CowBook::new(c);
        let mut rng = SplitMix::new(SEED);
        for i in 0..n {
            let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
            b.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        b
    }

    /// CowBook must reproduce the pinned cross-language golden bytes exactly.
    #[test]
    fn cowbook_matches_golden_bytes() {
        let c = golden_cfg();
        let mut cb = build_cow(&c, c.steady);
        let root = cb.capture();
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode_root(&root, &mut buf);
        let golden = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testdata/golden_snapshot.bin"
        ))
        .expect("golden file");
        assert_eq!(&buf[..n], &golden[..], "CowBook bytes == golden bytes");
    }

    /// Byte-equivalence with the STW encoder after a mixed insert+update run.
    #[test]
    fn cow_encode_equals_stw_encode_after_mixed_ops() {
        let c = golden_cfg();
        let mut b = Book::new(&c);
        let mut cb = CowBook::new(&c);
        let mut r1 = SplitMix::new(SEED);
        let mut r2 = SplitMix::new(SEED);
        for i in 0..c.steady {
            let a = next_insert(&mut r1, i, c.levels, c.tick, c.price_min);
            let x = next_insert(&mut r2, i, c.levels, c.tick, c.price_min);
            b.insert(a.order_id, a.price, a.qty, a.side);
            cb.insert(x.order_id, x.price, x.qty, x.side);
        }
        for _ in 0..500 {
            let a = next_update(&mut r1, c.steady);
            let x = next_update(&mut r2, c.steady);
            b.update(a.order_id, a.fill_qty);
            cb.update(x.order_id, x.fill_qty);
        }
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n1 = snapshot::encode(&b, &mut buf1);
        let root = cb.capture();
        let n2 = encode_root(&root, &mut buf2);
        assert_eq!(&buf1[..n1], &buf2[..n2]);
    }

    /// Restore round-trips: restore_cow(bytes) re-encodes to identical bytes,
    /// and a corrupted image is rejected.
    #[test]
    fn restore_cow_round_trips_and_rejects_corruption() {
        let c = golden_cfg();
        let mut cb = build_cow(&c, c.steady);
        let root = cb.capture();
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode_root(&root, &mut buf);
        let mut r = restore_cow(&buf[..n], &c).expect("restore");
        let root2 = r.capture();
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n2 = encode_root(&root2, &mut buf2);
        assert_eq!(&buf[..n], &buf2[..n2]);
        let mut bad = buf[..n].to_vec();
        bad[0] ^= 0xFF;
        assert!(restore_cow(&bad, &c).is_err());
    }

    /// The concurrency correctness test: capture at op k while a serializer
    /// encodes concurrently with ongoing writes; bytes must equal a
    /// single-threaded STW encode of a Book replayed to exactly op k.
    #[test]
    fn concurrent_capture_equals_stw_replay_at_same_position() {
        let c = golden_cfg();
        let total_updates = 2_000usize;
        let capture_at = 700usize; // snapshot after this many updates

        // Reference: Book replayed to exactly `capture_at` updates.
        let mut reference = Book::new(&c);
        let mut rr = SplitMix::new(SEED);
        for i in 0..c.steady {
            let ins = next_insert(&mut rr, i, c.levels, c.tick, c.price_min);
            reference.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        for _ in 0..capture_at {
            let up = next_update(&mut rr, c.steady);
            reference.update(up.order_id, up.fill_qty);
        }
        let mut want = vec![0u8; 4 * 1024 * 1024];
        let wn = snapshot::encode(&reference, &mut want);

        // Live: CowBook with a serializer thread encoding the captured root
        // while the writer keeps applying updates.
        let mut cb = build_cow(&c, c.steady);
        let mut rng = SplitMix::new(SEED);
        // skip the insert draws already consumed by build_cow
        for _ in 0..c.steady {
            let _ = next_insert(&mut rng, 0, c.levels, c.tick, c.price_min);
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<crate::cowbook::Root>(1);
        let ser = std::thread::spawn(move || {
            let root = rx.recv().expect("root");
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            let n = encode_root(&root, &mut buf);
            buf.truncate(n);
            buf
        });
        for k in 0..total_updates {
            if k == capture_at {
                tx.send(cb.capture()).expect("send root");
            }
            let up = next_update(&mut rng, c.steady);
            cb.update(up.order_id, up.fill_qty);
        }
        let got = ser.join().expect("serializer");
        assert_eq!(&want[..wn], &got[..], "concurrent capture == STW replay");
    }
}
```

Register in `lib.rs` (`pub mod cowsnap;`).

Note for the rng skip in `concurrent_capture_...`: `build_cow` consumed `2 * steady` draws (two per insert). The skip loop above calls `next_insert` `steady` times, which also draws twice per call — the streams line up. Do not "optimize" it to raw `rng.next()` calls without keeping the draw count at `2 * steady`.

- [ ] **Step 2: Run to verify pass**

Run: `cd rust && cargo test -p smr-collections-common cowsnap`
Expected: PASS (4 tests). The golden test failing means the encoder walk order diverges from `snapshot::encode` — fix `encode_root`, never the golden file.

- [ ] **Step 3: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/smr-collections/common
git commit -m "feat(smr-collections): CoW snapshot codec, golden + concurrent-capture tests (Rust)"
```

---

### Task R4: Rust `mvcc_insert` / `mvcc_update` / `mvcc_snapshot` artifacts

**Files:**
- Create: `rust/smr-collections/mvcc_insert/{Cargo.toml,src/main.rs}`
- Create: `rust/smr-collections/mvcc_update/{Cargo.toml,src/main.rs}`
- Create: `rust/smr-collections/mvcc_snapshot/{Cargo.toml,src/main.rs}`
- Modify: `rust/Cargo.toml` (3 new workspace members)

**Interfaces:**
- Consumes: `CowBook` (R2), `encode_root`/`restore_cow` (R3), `bench_common::smrcoll` helpers.
- Produces: binaries `smr-collections-mvcc_insert` etc., emitting the same metric names as the STW `insert`/`update`/`snapshot` cells with `experiment` = `mvcc_insert`/`mvcc_update`/`mvcc_snapshot`.

- [ ] **Step 1: Create the three crates.** Each `Cargo.toml` follows the existing pattern (shown for `mvcc_insert`; the other two substitute the name):

```toml
[package]
name = "smr-collections-mvcc_insert"
version.workspace = true
edition.workspace = true

[[bin]]
name = "smr-collections-mvcc_insert"
path = "src/main.rs"

[dependencies]
smr-collections-common = { path = "../common" }
bench-common = { path = "../../bench-common" }
```

`mvcc_insert/src/main.rs` (identical to `insert/src/main.rs` except `Book` → `CowBook` and the experiment name):

```rust
//! smr-collections **mvcc_insert** — insert cost on the chunked-CoW book
//! (steady-state MVCC overhead; no snapshots fire here).

use bench_common::smrcoll::{SmrConfig, emit_latency, measure};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::cowbook::CowBook;
use smr_collections_common::rng::{SEED, SplitMix};

const EXPERIMENT: &str = "mvcc_insert";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    let mut i = 0usize;
    let samples = measure(cfg.warmup, cfg.iters, || {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
        i += 1;
    });
    emit_latency(EXPERIMENT, "insert", &samples);
}
```

`mvcc_update/src/main.rs`:

```rust
//! smr-collections **mvcc_update** — partial-fill cost on the chunked-CoW book.

use bench_common::smrcoll::{SmrConfig, emit_latency, measure};
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::cowbook::CowBook;
use smr_collections_common::rng::{SEED, SplitMix};

const EXPERIMENT: &str = "mvcc_update";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;
    let samples = measure(cfg.warmup, cfg.iters, || {
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
    });
    emit_latency(EXPERIMENT, "update", &samples);
}
```

`mvcc_snapshot/src/main.rs` (capture is timed as part of "snapshot" — it is part of the MVCC snapshot cost):

```rust
//! smr-collections **mvcc_snapshot** — capture+serialize and restore cost of
//! the chunked-CoW book (single-threaded; the concurrent axis is live_mvcc).

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::cowbook::CowBook;
use smr_collections_common::cowsnap::{encode_root, restore_cow};
use smr_collections_common::rng::{SEED, SplitMix};
use std::time::Instant;

const EXPERIMENT: &str = "mvcc_snapshot";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];

    let mut snap_ns = vec![0u64; cfg.iters];
    let mut rest_ns = vec![0u64; cfg.iters];
    let mut snap_len = 0usize;
    for _ in 0..cfg.warmup {
        let root = book.capture();
        snap_len = encode_root(&root, &mut buf);
        let _ = restore_cow(&buf[..snap_len], &cfg).expect("restore");
    }
    for k in 0..cfg.iters {
        let t0 = Instant::now();
        let root = book.capture();
        snap_len = encode_root(&root, &mut buf);
        snap_ns[k] = t0.elapsed().as_nanos() as u64;
        let t1 = Instant::now();
        let r = restore_cow(&buf[..snap_len], &cfg).expect("restore");
        rest_ns[k] = t1.elapsed().as_nanos() as u64;
        std::hint::black_box(&r);
    }
    emit_latency(EXPERIMENT, "snapshot", &snap_ns);
    emit_latency(EXPERIMENT, "restore", &rest_ns);
    emit_int(EXPERIMENT, "snapshot_bytes", snap_len as u64, "bytes", 1);
    let mean_ns = bench_common::stats::mean(&snap_ns);
    emit_float(
        EXPERIMENT,
        "snapshot_throughput",
        (snap_len as f64) / (mean_ns / 1e9),
        "bytes_per_sec",
        cfg.iters,
    );
}
```

Add to `rust/Cargo.toml` members (after `"smr-collections/snapshot",`):

```toml
    "smr-collections/mvcc_insert",
    "smr-collections/mvcc_update",
    "smr-collections/mvcc_snapshot",
```

- [ ] **Step 2: Build and smoke-run** (small params so it finishes in seconds; smoke output is a fitness check only, never journaled)

Run:
```bash
cd rust && cargo build --release -p smr-collections-mvcc_insert -p smr-collections-mvcc_update -p smr-collections-mvcc_snapshot
SMRC_CAP=8192 SMRC_LEVELS=64 SMRC_STEADY=2000 SMRC_WARMUP=100 SMRC_ITERS=1000 cargo run --release -q -p smr-collections-mvcc_snapshot
```
Expected: 8 JSON lines with `"focus_area":"smr-collections","experiment":"mvcc_snapshot"` and metrics `snapshot_p50/p99/mean`, `restore_p50/p99/mean`, `snapshot_bytes`, `snapshot_throughput`. Also run the other two binaries the same way (3 lines each: `insert_*` / `update_*`).

- [ ] **Step 3: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/smr-collections/mvcc_insert rust/smr-collections/mvcc_update rust/smr-collections/mvcc_snapshot rust/Cargo.toml
git commit -m "feat(smr-collections): Rust mvcc_insert/mvcc_update/mvcc_snapshot cells"
```

---

### Task R5: Rust live experiments (`live_stw`, `live_mvcc`) + shared live emit

**Files:**
- Modify: `rust/bench-common/src/smrcoll.rs` (add `emit_live`)
- Create: `rust/smr-collections/live_stw/{Cargo.toml,src/main.rs}`
- Create: `rust/smr-collections/live_mvcc/{Cargo.toml,src/main.rs}`
- Modify: `rust/Cargo.toml` (2 members)

**Interfaces:**
- Consumes: `Book` + `snapshot::encode` (existing), `CowBook`/`Root` (R2), `encode_root` (R3), `SmrConfig.live_iters/snap_every` (R1).
- Produces: `bench_common::smrcoll::emit_live(experiment: &str, writer_ns: &[u64], snap_ns: &[u64], skipped: u64, snap_len: usize)` emitting the Global-Constraints live metric set; binaries `smr-collections-live_stw`, `smr-collections-live_mvcc`.

- [ ] **Step 1: Add `emit_live` to `bench-common/src/smrcoll.rs`:**

```rust
/// Emit the live-experiment metric set: writer latency (p50/p99/mean + max),
/// snapshot latency over completed snapshots, counts, and image size.
pub fn emit_live(experiment: &str, writer_ns: &[u64], snap_ns: &[u64], skipped: u64, snap_len: usize) {
    emit_latency(experiment, "writer", writer_ns);
    emit_int(
        experiment,
        "writer_max",
        writer_ns.iter().copied().max().unwrap_or(0),
        "ns",
        writer_ns.len(),
    );
    emit_latency(experiment, "snapshot", snap_ns);
    emit_int(experiment, "snap_count", snap_ns.len() as u64, "count", 1);
    emit_int(experiment, "snap_skipped", skipped, "count", 1);
    emit_int(experiment, "snapshot_bytes", snap_len as u64, "bytes", 1);
}
```

- [ ] **Step 2: Create `live_stw`.** `Cargo.toml` as in R4 (name `smr-collections-live_stw`). `src/main.rs`:

```rust
//! smr-collections **live_stw** — writer-observed latency while stop-the-world
//! snapshots run inline at a fixed op cadence. The op that triggers a snapshot
//! pays the whole serialize in its own latency (writer_max is the stall).

use bench_common::smrcoll::{SmrConfig, emit_live};
use smr_collections_common::book::Book;
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_common::snapshot::encode;
use std::time::Instant;

const EXPERIMENT: &str = "live_stw";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = Book::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;
    for _ in 0..cfg.warmup {
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
    }
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];
    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut snap_ns: Vec<u64> = Vec::new();
    let mut snap_len = 0usize;
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            snap_len = encode(&book, &mut buf);
            snap_ns.push(t0.elapsed().as_nanos() as u64);
        }
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
        *w = t0.elapsed().as_nanos() as u64;
    }
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, 0, snap_len);
}
```

- [ ] **Step 3: Create `live_mvcc`.** `Cargo.toml` as in R4 (name `smr-collections-live_mvcc`). `src/main.rs`:

```rust
//! smr-collections **live_mvcc** — writer-observed latency while a serializer
//! thread encodes captured CoW roots concurrently. The writer pays only the
//! O(#chunks) capture plus CoW chunk copies as it re-dirties state.

use bench_common::smrcoll::{SmrConfig, emit_live};
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::cowbook::{CowBook, Root};
use smr_collections_common::cowsnap::encode_root;
use smr_collections_common::rng::{SEED, SplitMix};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

const EXPERIMENT: &str = "live_mvcc";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;
    for _ in 0..cfg.warmup {
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
    }

    let busy = Arc::new(AtomicBool::new(false));
    let busy_ser = Arc::clone(&busy);
    let buf_len = 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32;
    let (tx, rx) = mpsc::sync_channel::<(Root, Instant)>(1);
    let ser = std::thread::spawn(move || {
        let mut buf = vec![0u8; buf_len];
        let mut durations: Vec<u64> = Vec::new();
        let mut len = 0usize;
        for (root, t0) in rx {
            len = encode_root(&root, &mut buf);
            durations.push(t0.elapsed().as_nanos() as u64);
            busy_ser.store(false, Ordering::Release);
        }
        (durations, len)
    });

    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut skipped = 0u64;
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            if busy.load(Ordering::Acquire) {
                skipped += 1;
            } else {
                busy.store(true, Ordering::Relaxed);
                tx.send((book.capture(), t0)).expect("serializer alive");
            }
        }
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
        *w = t0.elapsed().as_nanos() as u64;
    }
    drop(tx);
    let (snap_ns, snap_len) = ser.join().expect("serializer join");
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, skipped, snap_len);
}
```

Add both to `rust/Cargo.toml` members:

```toml
    "smr-collections/live_stw",
    "smr-collections/live_mvcc",
```

- [ ] **Step 4: Build and smoke-run both**

Run:
```bash
cd rust && cargo build --release -p smr-collections-live_stw -p smr-collections-live_mvcc
SMRC_CAP=131072 SMRC_STEADY=60000 SMRC_WARMUP=1000 SMRC_LIVE_ITERS=50000 SMRC_SNAP_EVERY=10000 cargo run --release -q -p smr-collections-live_stw
SMRC_CAP=131072 SMRC_STEADY=60000 SMRC_WARMUP=1000 SMRC_LIVE_ITERS=50000 SMRC_SNAP_EVERY=10000 cargo run --release -q -p smr-collections-live_mvcc
```
Expected: 10 JSON lines each (`writer_p50/p99/mean`, `writer_max`, `snapshot_p50/p99/mean`, `snap_count`=5, `snap_skipped`, `snapshot_bytes`). Sanity: `live_stw` `writer_max` should be ≫ `writer_p99` (it contains a full serialize); `live_mvcc` `writer_max` should be far below `live_stw`'s; `snapshot_bytes` identical across the two.

- [ ] **Step 5: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/bench-common rust/smr-collections/live_stw rust/smr-collections/live_mvcc rust/Cargo.toml
git commit -m "feat(smr-collections): Rust live_stw/live_mvcc snapshot-under-writes cells"
```

---

### Task R6: ultima_db adapter crate (`smr-collections-ultima`)

**Files:**
- Modify: `rust/Cargo.toml` (workspace dep + member `smr-collections/ultima-common`)
- Create: `rust/smr-collections/ultima-common/{Cargo.toml,src/lib.rs}`

**Interfaces:**
- Consumes: ultima_db `Store`/`StoreConfig`/`WriterMode` (SingleWriter, explicit versions, `num_snapshots_retained(1024)`), `booksnap` codec, `book::NIL`, `SmrConfig`.
- Produces: `UltimaBook { pub store: Arc<Store>, ... }` with `new(&SmrConfig)`, `insert(order_id, price, qty, side)`, `update(order_id, fill_qty)`, `current_version(&self) -> u64`; free fns `encode_at(store: &Store, version: u64, buf: &mut [u8]) -> usize` (callable from any thread — it does `begin_read(Some(version))` itself, since `ReadTx` is `!Send`) and `restore_ultima(bytes: &[u8], cfg: &SmrConfig) -> Result<UltimaBook, String>`.

- [ ] **Step 1: Wire the dependency.** In `rust/Cargo.toml` `[workspace.dependencies]` add:

```toml
ultima_db = { git = "https://github.com/PeterKnego/ultima_db.git", rev = "b48295e9ad6ba6e54100a6e8ec9248c8e84e09c3" }
```

and add member `"smr-collections/ultima-common",`. Create `rust/smr-collections/ultima-common/Cargo.toml`:

```toml
[package]
name = "smr-collections-ultima"
version.workspace = true
edition.workspace = true

[lib]
name = "smr_collections_ultima"
path = "src/lib.rs"

[dependencies]
bench-common = { path = "../../bench-common" }
smr-collections-common = { path = "../common" }
booksnap = { package = "booksnap-codec", path = "../booksnap-sbe/generated/booksnap" }
crc32c = { workspace = true }
ultima_db = { workspace = true }
```

- [ ] **Step 2: Write `src/lib.rs`.** All state lives in three tables whose id-ordered iteration reproduces the golden byte order: `orders` (id = orderId = slot+1, sequential by construction), `levels` (id = side·nLevels + tick + 1; ALL levels pre-created at init in side-major tick order, so ids sort bids-then-asks ascending-tick), `meta` (single record, id 1, carries the scalars). Every mutation is one write-txn committed at an explicit version = op index (the SMR pattern).

```rust
//! ultima_db competitor cell: the LOB mapped onto an MVCC store used in its
//! SMR pattern — SingleWriter, explicit commit versions (= log position),
//! snapshot = read-txn at a version, encoded to the shared booksnap SBE image
//! (byte-identical to the STW encoder; enforced by the golden test).

use bench_common::smrcoll::SmrConfig;
use booksnap::book_snapshot_codec::encoder::{LevelsEncoder, OrdersEncoder};
use booksnap::book_snapshot_codec::{BookSnapshotDecoder, BookSnapshotEncoder};
use booksnap::message_header_codec::MessageHeaderDecoder;
use booksnap::side::Side;
use booksnap::{Encoder, ReadBuf, WriteBuf};
use smr_collections_common::book::NIL;
use std::sync::Arc;
use ultima_db::{Store, StoreConfig, WriterMode};

const HEADER_LEN: usize = 8;

#[derive(Clone, Debug)]
pub struct OrderRec {
    pub slot: u32,
    pub price: i64,
    pub qty: i64,
    pub filled: i64,
    pub side: u8,
    pub next: u32,
    pub prev: u32,
}

#[derive(Clone, Debug)]
pub struct LevelRec {
    pub side: u8,
    pub tick: u32,
    pub qty_total: i64,
    pub count: u32,
    pub head: u32,
    pub tail: u32,
}

/// Single meta record (table id 1): every scalar the snapshot needs, so
/// `encode_at` is self-contained given only the store + version.
#[derive(Clone, Debug)]
pub struct MetaRec {
    pub price_min: i64,
    pub tick: i64,
    pub n_levels: u32,
    pub capacity: u32,
    pub hwm: u32,
    pub best_bid: i32,
    pub best_ask: i32,
}

pub struct UltimaBook {
    pub store: Arc<Store>,
    version: u64,
    price_min: i64,
    tick: i64,
    n_levels: u32,
}

impl UltimaBook {
    pub fn new(cfg: &SmrConfig) -> UltimaBook {
        let store = Store::new(
            StoreConfig::builder()
                .writer_mode(WriterMode::SingleWriter)
                .require_explicit_version(true)
                // Retention safety for live_ultima: the serializer re-opens the
                // captured version by number while the writer keeps committing.
                .num_snapshots_retained(1024)
                .build(),
        )
        .expect("store");
        let mut ub = UltimaBook {
            store: Arc::new(store),
            version: 0,
            price_min: cfg.price_min,
            tick: cfg.tick,
            n_levels: cfg.levels,
        };
        ub.version += 1;
        let mut wtx = ub.store.begin_write(Some(ub.version)).expect("wtx");
        {
            let mut levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            for side in 0..2u8 {
                for t in 0..cfg.levels {
                    levels
                        .insert(LevelRec {
                            side,
                            tick: t,
                            qty_total: 0,
                            count: 0,
                            head: NIL,
                            tail: NIL,
                        })
                        .expect("level insert");
                }
            }
        }
        {
            let mut meta = wtx.open_table::<MetaRec>("meta").expect("meta");
            meta.insert(MetaRec {
                price_min: cfg.price_min,
                tick: cfg.tick,
                n_levels: cfg.levels,
                capacity: cfg.cap as u32,
                hwm: 0,
                best_bid: -1,
                best_ask: -1,
            })
            .expect("meta insert");
        }
        wtx.commit().expect("commit");
        ub
    }

    #[inline]
    fn level_id(&self, side: u8, t: u32) -> u64 {
        side as u64 * self.n_levels as u64 + t as u64 + 1
    }

    #[inline]
    fn tick_of(&self, price: i64) -> u32 {
        ((price - self.price_min) / self.tick) as u32
    }

    pub fn current_version(&self) -> u64 {
        self.version
    }

    pub fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        let t = self.tick_of(price);
        let lid = self.level_id(side, t);
        let slot = (order_id - 1) as u32; // ids are assigned sequentially from 1
        let mut lvl = {
            let levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            levels.get(lid).expect("level").clone()
        };
        let prev_tail = lvl.tail;
        {
            let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
            let id = orders
                .insert(OrderRec {
                    slot,
                    price,
                    qty,
                    filled: 0,
                    side,
                    next: NIL,
                    prev: prev_tail,
                })
                .expect("order insert");
            assert_eq!(id, order_id as u64, "table id must equal orderId");
            if prev_tail != NIL {
                let pid = prev_tail as u64 + 1; // orderId of the previous tail
                let mut p = orders.get(pid).expect("prev order").clone();
                p.next = slot;
                orders.update(pid, p).expect("prev update");
            }
        }
        if lvl.tail == NIL {
            lvl.head = slot;
        }
        lvl.tail = slot;
        lvl.qty_total += qty;
        lvl.count += 1;
        {
            let mut levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            levels.update(lid, lvl).expect("level update");
        }
        {
            let mut meta = wtx.open_table::<MetaRec>("meta").expect("meta");
            let mut m = meta.get(1).expect("meta rec").clone();
            m.hwm = slot + 1;
            if side == 0 && (m.best_bid < 0 || t as i32 > m.best_bid) {
                m.best_bid = t as i32;
            }
            if side == 1 && (m.best_ask < 0 || (t as i32) < m.best_ask) {
                m.best_ask = t as i32;
            }
            meta.update(1, m).expect("meta update");
        }
        wtx.commit().expect("commit");
    }

    pub fn update(&mut self, order_id: i64, fill_qty: i64) {
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        let (lid, add) = {
            let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
            let mut o = orders.get(order_id as u64).expect("order").clone();
            let add = fill_qty.min(o.qty - o.filled);
            o.filled += add;
            let t = self.tick_of(o.price);
            let lid = self.level_id(o.side, t);
            orders.update(order_id as u64, o).expect("order update");
            (lid, add)
        };
        {
            let mut levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            let mut lvl = levels.get(lid).expect("level").clone();
            lvl.qty_total -= add;
            levels.update(lid, lvl).expect("level update");
        }
        wtx.commit().expect("commit");
    }
}

fn side_enum(side: u8) -> Side {
    if side == 0 { Side::BID } else { Side::ASK }
}

/// Encode the state at `version` into `buf`. Opens its own read-txn, so it is
/// callable from a serializer thread (`ReadTx` is not `Send` by design —
/// ultima_db documents "begin_read on the thread that needs the view").
pub fn encode_at(store: &Store, version: u64, buf: &mut [u8]) -> usize {
    let rtx = store.begin_read(Some(version)).expect("read txn");
    let levels = rtx.open_table::<LevelRec>("levels").expect("levels");
    let orders = rtx.open_table::<OrderRec>("orders").expect("orders");
    let meta_t = rtx.open_table::<MetaRec>("meta").expect("meta");
    let m = meta_t.get(1).expect("meta rec");
    let level_count = levels.iter().filter(|(_, l)| l.head != NIL).count() as u16;
    let sbe_len = {
        let enc = BookSnapshotEncoder::default().wrap(WriteBuf::new(buf), HEADER_LEN);
        let mut header = enc.header(0);
        let mut enc = header.parent().expect("header parent");
        enc.price_min(m.price_min);
        enc.tick_size(m.tick);
        enc.nl_evels(m.n_levels);
        enc.capacity(m.capacity);
        enc.hwm(m.hwm);
        enc.best_bid(m.best_bid);
        enc.best_ask(m.best_ask);

        let mut lg = enc.levels_encoder(level_count, LevelsEncoder::default());
        // Level ids are side*nLevels + tick + 1: id order IS bids-then-asks,
        // ascending tick — the STW encoder's lane order.
        for (_, l) in levels.iter() {
            if l.head == NIL {
                continue;
            }
            lg.advance().expect("levels advance");
            lg.side(side_enum(l.side));
            lg.level_tick(l.tick);
            lg.qty_total(l.qty_total);
            lg.order_count(l.count);
            lg.head(l.head);
            lg.tail(l.tail);
        }
        let enc = lg.parent().expect("levels parent");

        let mut og = enc.orders_encoder(m.hwm as u16, OrdersEncoder::default());
        // Order ids are sequential from 1 in insertion order: id order IS slot
        // order 0..hwm.
        for (_, o) in orders.iter() {
            og.advance().expect("orders advance");
            og.slot(o.slot);
            og.order_id(o.slot as i64 + 1);
            og.price(o.price);
            og.qty(o.qty);
            og.filled(o.filled);
            og.side(side_enum(o.side));
            og.next_slot(o.next);
            og.prev(o.prev);
        }
        og.get_limit()
    };
    let crc = crc32c::crc32c(&buf[..sbe_len]);
    buf[sbe_len..sbe_len + 4].copy_from_slice(&crc.to_le_bytes());
    sbe_len + 4
}

/// Restore a fresh UltimaBook from an encoded image (crc-verified): build the
/// empty store (v1), then apply the decoded state as one bulk commit (v2).
pub fn restore_ultima(bytes: &[u8], cfg: &SmrConfig) -> Result<UltimaBook, String> {
    if bytes.len() < 4 {
        return Err("snapshot too short".into());
    }
    let sbe_len = bytes.len() - 4;
    let want = u32::from_le_bytes(bytes[sbe_len..].try_into().unwrap());
    if crc32c::crc32c(&bytes[..sbe_len]) != want {
        return Err("crc32c mismatch".into());
    }
    let mut ub = UltimaBook::new(cfg);
    let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(&bytes[..sbe_len]), 0);
    let dec = BookSnapshotDecoder::default().header(header, 0);
    let (price_min, tick, n_levels) = (dec.price_min(), dec.tick_size(), dec.nl_evels());
    let (capacity, hwm) = (dec.capacity(), dec.hwm());
    let (best_bid, best_ask) = (dec.best_bid(), dec.best_ask());
    if n_levels != cfg.levels {
        return Err("nLevels mismatch vs config".into());
    }

    ub.version += 1;
    let mut wtx = ub.store.begin_write(Some(ub.version)).expect("wtx");

    let mut lg = dec.levels_decoder();
    let lc = lg.count();
    {
        let mut levels = wtx.open_table::<LevelRec>("levels").expect("levels");
        for _ in 0..lc {
            lg.advance().expect("advance").expect("level present");
            let side = if lg.side() == Side::ASK { 1u8 } else { 0u8 };
            let t = lg.level_tick();
            let lid = side as u64 * n_levels as u64 + t as u64 + 1;
            levels
                .update(
                    lid,
                    LevelRec {
                        side,
                        tick: t,
                        qty_total: lg.qty_total(),
                        count: lg.order_count(),
                        head: lg.head(),
                        tail: lg.tail(),
                    },
                )
                .expect("level update");
        }
    }
    let dec = lg.parent().expect("levels parent");

    let mut og = dec.orders_decoder();
    let oc = og.count();
    {
        let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
        for _ in 0..oc {
            og.advance().expect("advance").expect("order present");
            let slot = og.slot();
            let id = orders
                .insert(OrderRec {
                    slot,
                    price: og.price(),
                    qty: og.qty(),
                    filled: og.filled(),
                    side: if og.side() == Side::ASK { 1 } else { 0 },
                    next: og.next_slot(),
                    prev: og.prev(),
                })
                .expect("order insert");
            if id != slot as u64 + 1 {
                return Err("orders group not in slot order".into());
            }
        }
    }
    {
        let mut meta = wtx.open_table::<MetaRec>("meta").expect("meta");
        meta.update(
            1,
            MetaRec {
                price_min,
                tick,
                n_levels,
                capacity,
                hwm,
                best_bid,
                best_ask,
            },
        )
        .expect("meta update");
    }
    wtx.commit().expect("commit");
    Ok(ub)
}

#[cfg(test)]
mod tests {
    use super::*;
    use smr_collections_common::book::Book;
    use smr_collections_common::book::workload::{next_insert, next_update};
    use smr_collections_common::rng::{SEED, SplitMix};
    use smr_collections_common::snapshot;

    fn cfg() -> SmrConfig {
        SmrConfig {
            cap: 4096,
            levels: 64,
            tick: 1,
            price_min: 0,
            steady: 2000,
            warmup: 0,
            iters: 0,
            chunk: 4096,
            live_iters: 200_000,
            snap_every: 20_000,
        }
    }

    #[test]
    fn ultima_matches_golden_bytes() {
        let c = cfg();
        let mut ub = UltimaBook::new(&c);
        let mut rng = SplitMix::new(SEED);
        for i in 0..c.steady {
            let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
            ub.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode_at(&ub.store, ub.current_version(), &mut buf);
        let golden = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testdata/golden_snapshot.bin"
        ))
        .expect("golden file");
        assert_eq!(&buf[..n], &golden[..], "ultima bytes == golden bytes");
    }

    #[test]
    fn ultima_encode_equals_stw_encode_after_mixed_ops() {
        let c = cfg();
        let mut b = Book::new(&c);
        let mut ub = UltimaBook::new(&c);
        let mut r1 = SplitMix::new(SEED);
        let mut r2 = SplitMix::new(SEED);
        for i in 0..c.steady {
            let a = next_insert(&mut r1, i, c.levels, c.tick, c.price_min);
            let x = next_insert(&mut r2, i, c.levels, c.tick, c.price_min);
            b.insert(a.order_id, a.price, a.qty, a.side);
            ub.insert(x.order_id, x.price, x.qty, x.side);
        }
        for _ in 0..300 {
            let a = next_update(&mut r1, c.steady);
            let x = next_update(&mut r2, c.steady);
            b.update(a.order_id, a.fill_qty);
            ub.update(x.order_id, x.fill_qty);
        }
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n1 = snapshot::encode(&b, &mut buf1);
        let n2 = encode_at(&ub.store, ub.current_version(), &mut buf2);
        assert_eq!(&buf1[..n1], &buf2[..n2]);
    }

    #[test]
    fn old_version_is_a_frozen_snapshot() {
        let c = cfg();
        let mut ub = UltimaBook::new(&c);
        let mut rng = SplitMix::new(SEED);
        for i in 0..c.steady {
            let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
            ub.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        let v = ub.current_version();
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let n1 = encode_at(&ub.store, v, &mut buf1);
        for _ in 0..200 {
            let up = next_update(&mut rng, c.steady);
            ub.update(up.order_id, up.fill_qty);
        }
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n2 = encode_at(&ub.store, v, &mut buf2); // same old version
        assert_eq!(&buf1[..n1], &buf2[..n2], "old version frozen");
        let mut buf3 = vec![0u8; 4 * 1024 * 1024];
        let n3 = encode_at(&ub.store, ub.current_version(), &mut buf3);
        assert_ne!(&buf1[..n1], &buf3[..n3], "latest version advanced");
    }

    #[test]
    fn restore_round_trips_and_rejects_corruption() {
        let c = cfg();
        let mut ub = UltimaBook::new(&c);
        let mut rng = SplitMix::new(SEED);
        for i in 0..c.steady {
            let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
            ub.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode_at(&ub.store, ub.current_version(), &mut buf);
        let r = restore_ultima(&buf[..n], &c).expect("restore");
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n2 = encode_at(&r.store, r.current_version(), &mut buf2);
        assert_eq!(&buf[..n], &buf2[..n2]);
        let mut bad = buf[..n].to_vec();
        bad[100] ^= 0xFF;
        assert!(restore_ultima(&bad, &c).is_err());
    }

    /// The concurrency correctness test for the ultima adapter: a serializer
    /// thread encodes a captured version while the writer keeps committing;
    /// bytes must equal a single-threaded STW encode of a Book replayed to
    /// exactly the capture position.
    #[test]
    fn concurrent_encode_at_equals_stw_replay() {
        let c = cfg();
        let total_updates = 500usize;
        let capture_at = 200usize;

        let mut reference = Book::new(&c);
        let mut rr = SplitMix::new(SEED);
        for i in 0..c.steady {
            let ins = next_insert(&mut rr, i, c.levels, c.tick, c.price_min);
            reference.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        for _ in 0..capture_at {
            let up = next_update(&mut rr, c.steady);
            reference.update(up.order_id, up.fill_qty);
        }
        let mut want = vec![0u8; 4 * 1024 * 1024];
        let wn = snapshot::encode(&reference, &mut want);

        let mut ub = UltimaBook::new(&c);
        let mut rng = SplitMix::new(SEED);
        for i in 0..c.steady {
            let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
            ub.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        let store = std::sync::Arc::clone(&ub.store);
        let (tx, rx) = std::sync::mpsc::sync_channel::<u64>(1);
        let ser = std::thread::spawn(move || {
            let version = rx.recv().expect("version");
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            let n = encode_at(&store, version, &mut buf);
            buf.truncate(n);
            buf
        });
        for k in 0..total_updates {
            if k == capture_at {
                tx.send(ub.current_version()).expect("send version");
            }
            let up = next_update(&mut rng, c.steady);
            ub.update(up.order_id, up.fill_qty);
        }
        let got = ser.join().expect("serializer");
        assert_eq!(&want[..wn], &got[..], "concurrent encode_at == STW replay");
    }
}
```

API note for the implementer: the exact re-export paths (`ultima_db::{Store, StoreConfig, WriterMode}`) and minor method-signature details should be verified against the pinned rev's docs (`cargo doc -p ultima_db` or the source at `~/ultima/ultima_db`). If `require_explicit_version` conflicts with the two internal bookkeeping commits (`new` at v1, restore at v2), those are fine — they also pass explicit versions. Adjust mechanically; do NOT change the table/id scheme or the emitted bytes. One assumption to verify on first run: `Table::insert` assigns sequential ids starting at **1** — the `assert_eq!(id, order_id as u64)` in `insert` and the golden test both fail immediately if ids start at 0; in that case shift `level_id` and the slot↔id arithmetic by one consistently.

- [ ] **Step 3: Run to verify pass** (first build fetches the git dep)

Run: `cd rust && cargo test -p smr-collections-ultima`
Expected: PASS (5 tests). The golden test is the gate: if it fails, iteration order or field mapping is wrong — fix the adapter, never the golden file.

- [ ] **Step 4: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/smr-collections/ultima-common rust/Cargo.toml rust/Cargo.lock
git commit -m "feat(smr-collections): ultima_db adapter (SMR pattern, golden-identical bytes)"
```

---

### Task R7: ultima artifacts (`ultima_insert`/`ultima_update`/`ultima_snapshot`/`live_ultima`)

**Files:**
- Create: `rust/smr-collections/{ultima_insert,ultima_update,ultima_snapshot,live_ultima}/{Cargo.toml,src/main.rs}`
- Modify: `rust/Cargo.toml` (4 members)

**Interfaces:**
- Consumes: `UltimaBook`, `encode_at`, `restore_ultima` (R6), `emit_live` (R5), workload fns.
- Produces: binaries `smr-collections-ultima_insert` etc.

- [ ] **Step 1: Create the four crates.** `Cargo.toml` pattern (substitute the name for each):

```toml
[package]
name = "smr-collections-ultima_insert"
version.workspace = true
edition.workspace = true

[[bin]]
name = "smr-collections-ultima_insert"
path = "src/main.rs"

[dependencies]
smr-collections-common = { path = "../common" }
smr-collections-ultima = { path = "../ultima-common" }
bench-common = { path = "../../bench-common" }
```

`ultima_insert/src/main.rs` — same shape as `mvcc_insert` with `CowBook` → `UltimaBook`, `EXPERIMENT = "ultima_insert"`:

```rust
//! smr-collections **ultima_insert** — insert cost through ultima_db (one
//! explicit-version write-txn per op: order insert + level + meta updates).

use bench_common::smrcoll::{SmrConfig, emit_latency, measure};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::UltimaBook;

const EXPERIMENT: &str = "ultima_insert";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    let mut i = 0usize;
    let samples = measure(cfg.warmup, cfg.iters, || {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
        i += 1;
    });
    emit_latency(EXPERIMENT, "insert", &samples);
}
```

`ultima_update/src/main.rs` — same shape as `mvcc_update` (`UltimaBook`, `EXPERIMENT = "ultima_update"`; steady build via `book.insert`, timed loop via `book.update`).

`ultima_snapshot/src/main.rs` — same shape as `mvcc_snapshot`, with the snapshot phase `encode_at(&book.store, book.current_version(), &mut buf)` and the restore phase `restore_ultima(&buf[..snap_len], &cfg)`, `EXPERIMENT = "ultima_snapshot"`. Full main:

```rust
//! smr-collections **ultima_snapshot** — read-txn serialize + rebuild-store
//! restore cost through ultima_db.

use bench_common::smrcoll::{SmrConfig, emit_float, emit_int, emit_latency};
use smr_collections_common::book::workload::next_insert;
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::{UltimaBook, encode_at, restore_ultima};
use std::time::Instant;

const EXPERIMENT: &str = "ultima_snapshot";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];

    let mut snap_ns = vec![0u64; cfg.iters];
    let mut rest_ns = vec![0u64; cfg.iters];
    let mut snap_len = 0usize;
    for _ in 0..cfg.warmup {
        snap_len = encode_at(&book.store, book.current_version(), &mut buf);
        let _ = restore_ultima(&buf[..snap_len], &cfg).expect("restore");
    }
    for k in 0..cfg.iters {
        let t0 = Instant::now();
        snap_len = encode_at(&book.store, book.current_version(), &mut buf);
        snap_ns[k] = t0.elapsed().as_nanos() as u64;
        let t1 = Instant::now();
        let r = restore_ultima(&buf[..snap_len], &cfg).expect("restore");
        rest_ns[k] = t1.elapsed().as_nanos() as u64;
        std::hint::black_box(&r);
    }
    emit_latency(EXPERIMENT, "snapshot", &snap_ns);
    emit_latency(EXPERIMENT, "restore", &rest_ns);
    emit_int(EXPERIMENT, "snapshot_bytes", snap_len as u64, "bytes", 1);
    let mean_ns = bench_common::stats::mean(&snap_ns);
    emit_float(
        EXPERIMENT,
        "snapshot_throughput",
        (snap_len as f64) / (mean_ns / 1e9),
        "bytes_per_sec",
        cfg.iters,
    );
}
```

`live_ultima/src/main.rs` — the live loop with a serializer thread that receives `(version, t0)` and calls `encode_at` (which opens its own read-txn on that thread):

```rust
//! smr-collections **live_ultima** — writer-observed latency while a
//! serializer thread encodes a pinned old version concurrently. Capture is
//! O(1): the writer just hands over its last committed version number.

use bench_common::smrcoll::{SmrConfig, emit_live};
use smr_collections_common::book::workload::{next_insert, next_update};
use smr_collections_common::rng::{SEED, SplitMix};
use smr_collections_ultima::{UltimaBook, encode_at};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

const EXPERIMENT: &str = "live_ultima";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = UltimaBook::new(&cfg);
    let mut rng = SplitMix::new(SEED);
    for i in 0..cfg.steady {
        let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
        book.insert(ins.order_id, ins.price, ins.qty, ins.side);
    }
    let n = cfg.steady;
    for _ in 0..cfg.warmup {
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
    }

    let busy = Arc::new(AtomicBool::new(false));
    let busy_ser = Arc::clone(&busy);
    let store = Arc::clone(&book.store);
    let buf_len = 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32;
    let (tx, rx) = mpsc::sync_channel::<(u64, Instant)>(1);
    let ser = std::thread::spawn(move || {
        let mut buf = vec![0u8; buf_len];
        let mut durations: Vec<u64> = Vec::new();
        let mut len = 0usize;
        for (version, t0) in rx {
            len = encode_at(&store, version, &mut buf);
            durations.push(t0.elapsed().as_nanos() as u64);
            busy_ser.store(false, Ordering::Release);
        }
        (durations, len)
    });

    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut skipped = 0u64;
    for (k, w) in writer_ns.iter_mut().enumerate() {
        let t0 = Instant::now();
        if k % cfg.snap_every == 0 {
            if busy.load(Ordering::Acquire) {
                skipped += 1;
            } else {
                busy.store(true, Ordering::Relaxed);
                tx.send((book.current_version(), t0)).expect("serializer alive");
            }
        }
        let up = next_update(&mut rng, n);
        book.update(up.order_id, up.fill_qty);
        *w = t0.elapsed().as_nanos() as u64;
    }
    drop(tx);
    let (snap_ns, snap_len) = ser.join().expect("serializer join");
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, skipped, snap_len);
}
```

Add the four members to `rust/Cargo.toml`:

```toml
    "smr-collections/ultima_insert",
    "smr-collections/ultima_update",
    "smr-collections/ultima_snapshot",
    "smr-collections/live_ultima",
```

- [ ] **Step 2: Build and smoke-run** (small iters — ultima ops are µs-scale, 100k default would take minutes locally; note `SMRC_WARMUP + SMRC_ITERS <= SMRC_CAP` still applies)

Run:
```bash
cd rust && cargo build --release -p smr-collections-ultima_insert -p smr-collections-ultima_update -p smr-collections-ultima_snapshot -p smr-collections-live_ultima
SMRC_CAP=16384 SMRC_STEADY=2000 SMRC_WARMUP=200 SMRC_ITERS=2000 cargo run --release -q -p smr-collections-ultima_insert
SMRC_CAP=16384 SMRC_STEADY=2000 SMRC_WARMUP=200 SMRC_ITERS=50 cargo run --release -q -p smr-collections-ultima_snapshot
SMRC_CAP=16384 SMRC_STEADY=2000 SMRC_WARMUP=200 SMRC_LIVE_ITERS=10000 SMRC_SNAP_EVERY=2000 cargo run --release -q -p smr-collections-live_ultima
```
Expected: contract lines with the right experiment names; `live_ultima` `snap_count + snap_skipped == 5`.

- [ ] **Step 3: Gates + commit**

```bash
cd rust && cargo clippy --all-targets && cargo fmt
git add -A rust/smr-collections rust/Cargo.toml rust/Cargo.lock
git commit -m "feat(smr-collections): Rust ultima_* and live_ultima cells"
```

---

### Task G1: Go config — extend `SmrConfig`

**Files:**
- Modify: `go/internal/bench/smrcoll.go`
- Create: `go/internal/bench/smrcoll_test.go`

**Interfaces:**
- Produces: `SmrConfig` gains `Chunk, LiveIters, SnapEvery int` (env `SMRC_CHUNK`/`SMRC_LIVE_ITERS`/`SMRC_SNAP_EVERY`, defaults 4096/200000/20000, validated `Chunk <= Cap`, `SnapEvery <= LiveIters`).

- [ ] **Step 1: Write the failing test** — create `go/internal/bench/smrcoll_test.go`:

```go
package bench

import "testing"

func TestSmrConfigNewFieldDefaults(t *testing.T) {
	c, err := LoadSmrConfig()
	if err != nil {
		t.Fatal(err)
	}
	if c.Chunk != 4096 || c.LiveIters != 200000 || c.SnapEvery != 20000 {
		t.Fatalf("defaults wrong: %+v", c)
	}
}

func TestSmrConfigSnapEveryBound(t *testing.T) {
	t.Setenv("SMRC_LIVE_ITERS", "1000")
	t.Setenv("SMRC_SNAP_EVERY", "2000")
	if _, err := LoadSmrConfig(); err == nil {
		t.Fatal("want error: SNAP_EVERY > LIVE_ITERS")
	}
}

func TestSmrConfigChunkBound(t *testing.T) {
	t.Setenv("SMRC_CHUNK", "999999999")
	if _, err := LoadSmrConfig(); err == nil {
		t.Fatal("want error: CHUNK > CAP")
	}
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd go && go test ./internal/bench/`
Expected: COMPILE FAIL (`c.Chunk undefined`).

- [ ] **Step 3: Implement** — in `SmrConfig` add fields `Chunk, LiveIters, SnapEvery int` after `Iters`. In `LoadSmrConfig` add after the `iters` parse:

```go
	chunk, err := positiveEnv("SMRC_CHUNK", 4096)
	if err != nil {
		return SmrConfig{}, err
	}
	liveIters, err := positiveEnv("SMRC_LIVE_ITERS", 200000)
	if err != nil {
		return SmrConfig{}, err
	}
	snapEvery, err := positiveEnv("SMRC_SNAP_EVERY", 20000)
	if err != nil {
		return SmrConfig{}, err
	}
```

add `Chunk: chunk, LiveIters: liveIters, SnapEvery: snapEvery` to the struct literal, and after the existing `warmup+iters` check:

```go
	if chunk > cap_ {
		return SmrConfig{}, fmt.Errorf("SMRC_CHUNK must be <= SMRC_CAP")
	}
	if snapEvery > liveIters {
		return SmrConfig{}, fmt.Errorf("SMRC_SNAP_EVERY must be <= SMRC_LIVE_ITERS")
	}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd go && go vet ./... && go test ./internal/bench/ ./internal/smrcoll/`
Expected: PASS (existing smrcoll tests unaffected — struct literals in that package don't need the new fields since Go zero-values them; the golden test uses explicit fields only).

- [ ] **Step 5: Commit**

```bash
cd go && gofmt -l . && git add internal/bench && git commit -m "feat(smr-collections): Go SMRC_CHUNK/SMRC_LIVE_ITERS/SMRC_SNAP_EVERY config"
```

---

### Task G2: Go `CowBook` core

**Files:**
- Create: `go/internal/smrcoll/cowbook.go`
- Create: `go/internal/smrcoll/cowbook_test.go`

**Interfaces:**
- Consumes: `Order`, `Level`, `NIL`, `idMap`, `bench.SmrConfig` (G1), workload fns.
- Produces: `NewCowBook(cfg bench.SmrConfig) *CowBook`, methods `Insert(orderID, price, qty int64, side uint8)`, `Update(orderID, fillQty int64)`, `Capture() *CowRoot`, `GetSlot(orderID int64) uint32`, `LevelQty(side uint8, tick uint32) int64`, `OrderAt(slot uint32) *Order`, `LevelAt(side uint8, t uint32) *Level`; `CowRoot` with exported scalars (`PriceMin, Tick int64; NLevels, Capacity, Hwm uint32; BestBid, BestAsk int32`) and methods `OrderAt(slot uint32) *Order`, `LevelAt(side uint8, t uint32) *Level`. Package const `levelChunkLen = 256` (unexported). `rebuildIDs()` on CowBook for restore (G3).

- [ ] **Step 1: Create `cowbook.go`:**

```go
package smrcoll

// Chunked copy-on-write LOB: same logical behavior as Book, but the pool and
// ladder live in fixed-size chunks behind chunk tables. Capture() clones the
// chunk-ref slices (O(#chunks)) and bumps the generation; the writer copies a
// chunk before its first write after a capture (born < gen), so a frozen
// CowRoot is never mutated. GC reclaims dropped chunks. The copy decision is
// ALWAYS the epoch, never pointer comparison.

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

// levelChunkLen is the fixed ladder chunk size (orders-per-chunk is SMRC_CHUNK).
const levelChunkLen = 256

type orderChunk struct {
	born   uint64
	orders []Order
}

type lvlChunk struct {
	born   uint64
	levels []Level
}

// CowRoot is a frozen point-in-time view: chunk refs + scalars. Safe to hand
// to another goroutine (channel handoff gives the happens-before edge).
type CowRoot struct {
	PriceMin, Tick   int64
	NLevels          uint32
	Capacity, Hwm    uint32
	BestBid, BestAsk int32
	chunk            int
	orderChunks      []*orderChunk
	bidChunks        []*lvlChunk
	askChunks        []*lvlChunk
}

func (r *CowRoot) OrderAt(slot uint32) *Order {
	return &r.orderChunks[int(slot)/r.chunk].orders[int(slot)%r.chunk]
}

func (r *CowRoot) LevelAt(side uint8, t uint32) *Level {
	lane := r.bidChunks
	if side == 1 {
		lane = r.askChunks
	}
	return &lane[int(t)/levelChunkLen].levels[int(t)%levelChunkLen]
}

type CowBook struct {
	PriceMin, Tick   int64
	NLevels          uint32
	Chunk            int
	capacity         int
	gen              uint64
	orderChunks      []*orderChunk
	bidChunks        []*lvlChunk
	askChunks        []*lvlChunk
	Hwm              uint32
	BestBid, BestAsk int32
	ids              *idMap
}

func NewCowBook(cfg bench.SmrConfig) *CowBook {
	nOC := (cfg.Cap + cfg.Chunk - 1) / cfg.Chunk
	ocs := make([]*orderChunk, nOC)
	for ci := range ocs {
		n := cfg.Chunk
		if rem := cfg.Cap - ci*cfg.Chunk; rem < n {
			n = rem
		}
		ocs[ci] = &orderChunk{born: 1, orders: make([]Order, n)}
	}
	mkLane := func() []*lvlChunk {
		nLC := (int(cfg.Levels) + levelChunkLen - 1) / levelChunkLen
		lcs := make([]*lvlChunk, nLC)
		for ci := range lcs {
			n := levelChunkLen
			if rem := int(cfg.Levels) - ci*levelChunkLen; rem < n {
				n = rem
			}
			ls := make([]Level, n)
			for i := range ls {
				ls[i] = Level{Head: NIL, Tail: NIL}
			}
			lcs[ci] = &lvlChunk{born: 1, levels: ls}
		}
		return lcs
	}
	return &CowBook{
		PriceMin: cfg.PriceMin, Tick: cfg.Tick, NLevels: cfg.Levels,
		Chunk: cfg.Chunk, capacity: cfg.Cap, gen: 1,
		orderChunks: ocs, bidChunks: mkLane(), askChunks: mkLane(),
		BestBid: -1, BestAsk: -1, ids: newIDMap(cfg.Cap),
	}
}

func (b *CowBook) tickOf(price int64) uint32 { return uint32((price - b.PriceMin) / b.Tick) }

func (b *CowBook) laneChunks(side uint8) []*lvlChunk {
	if side == 0 {
		return b.bidChunks
	}
	return b.askChunks
}

func (b *CowBook) OrderAt(slot uint32) *Order {
	return &b.orderChunks[int(slot)/b.Chunk].orders[int(slot)%b.Chunk]
}

func (b *CowBook) LevelAt(side uint8, t uint32) *Level {
	lane := b.laneChunks(side)
	return &lane[int(t)/levelChunkLen].levels[int(t)%levelChunkLen]
}

func (b *CowBook) orderMut(slot uint32) *Order {
	ci := int(slot) / b.Chunk
	c := b.orderChunks[ci]
	if c.born < b.gen {
		cp := &orderChunk{born: b.gen, orders: make([]Order, len(c.orders))}
		copy(cp.orders, c.orders)
		b.orderChunks[ci] = cp
		c = cp
	}
	return &c.orders[int(slot)%b.Chunk]
}

func (b *CowBook) levelMut(side uint8, t uint32) *Level {
	lane := b.laneChunks(side)
	ci := int(t) / levelChunkLen
	c := lane[ci]
	if c.born < b.gen {
		cp := &lvlChunk{born: b.gen, levels: make([]Level, len(c.levels))}
		copy(cp.levels, c.levels)
		lane[ci] = cp
		c = cp
	}
	return &c.levels[int(t)%levelChunkLen]
}

// Insert mirrors Book.Insert (keep in lockstep).
func (b *CowBook) Insert(orderID, price, qty int64, side uint8) {
	t := b.tickOf(price)
	slot := b.Hwm
	b.Hwm++
	prevTail := b.LevelAt(side, t).Tail
	*b.orderMut(slot) = Order{OrderID: orderID, Price: price, Qty: qty, Filled: 0, Next: NIL, Prev: prevTail, Side: side}
	lvl := b.levelMut(side, t)
	if prevTail != NIL {
		b.orderMut(prevTail).Next = slot
	} else {
		lvl.Head = slot
	}
	lvl.Tail = slot
	lvl.QtyTotal += qty
	lvl.Count++
	b.ids.put(orderID, slot)
	if side == 0 && (b.BestBid < 0 || int32(t) > b.BestBid) {
		b.BestBid = int32(t)
	}
	if side == 1 && (b.BestAsk < 0 || int32(t) < b.BestAsk) {
		b.BestAsk = int32(t)
	}
}

// Update mirrors Book.Update (keep in lockstep).
func (b *CowBook) Update(orderID, fillQty int64) {
	slot := b.ids.get(orderID)
	o := b.orderMut(slot)
	add := fillQty
	if rem := o.Qty - o.Filled; add > rem {
		add = rem
	}
	o.Filled += add
	t := b.tickOf(o.Price)
	b.levelMut(o.Side, t).QtyTotal -= add
}

// Capture freezes the current state (O(#chunks)) and bumps the generation.
func (b *CowBook) Capture() *CowRoot {
	root := &CowRoot{
		PriceMin: b.PriceMin, Tick: b.Tick, NLevels: b.NLevels,
		Capacity: uint32(b.capacity), Hwm: b.Hwm,
		BestBid: b.BestBid, BestAsk: b.BestAsk, chunk: b.Chunk,
		orderChunks: append([]*orderChunk(nil), b.orderChunks...),
		bidChunks:   append([]*lvlChunk(nil), b.bidChunks...),
		askChunks:   append([]*lvlChunk(nil), b.askChunks...),
	}
	b.gen++
	return root
}

func (b *CowBook) GetSlot(orderID int64) uint32 { return b.ids.get(orderID) }

func (b *CowBook) LevelQty(side uint8, tick uint32) int64 { return b.LevelAt(side, tick).QtyTotal }

// rebuildIDs re-indexes the id-map from the pool (used after restore).
func (b *CowBook) rebuildIDs() {
	b.ids = newIDMap(b.capacity)
	for slot := uint32(0); slot < b.Hwm; slot++ {
		b.ids.put(b.OrderAt(slot).OrderID, slot)
	}
}
```

**Ordering caveat baked into `Insert` above:** the `lvl := b.levelMut(...)` pointer is taken BEFORE the `b.orderMut(prevTail)` call. That is safe only because `levelMut` and `orderMut` operate on disjoint chunk tables (level chunks vs order chunks) — an `orderMut` can never invalidate a `*Level`. Do not reorder into taking an `*Order` pointer across a `levelMut` call or vice versa within the same table kind.

- [ ] **Step 2: Create `cowbook_test.go`** (mirrors the Rust R2 tests):

```go
package smrcoll

import (
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func cowCfg() bench.SmrConfig {
	return bench.SmrConfig{
		Cap: 1024, Levels: 300, Tick: 1, PriceMin: 0,
		Steady: 500, Warmup: 0, Iters: 0,
		Chunk: 64, LiveIters: 200000, SnapEvery: 20000,
	}
}

func TestCowBookMatchesBookQueries(t *testing.T) {
	c := cowCfg()
	b := NewBook(c)
	cb := NewCowBook(c)
	r1 := NewSplitMix(SmrSeed)
	r2 := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		a := NextInsert(r1, i, c.Levels, c.Tick, c.PriceMin)
		x := NextInsert(r2, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(a.OrderID, a.Price, a.Qty, a.Side)
		cb.Insert(x.OrderID, x.Price, x.Qty, x.Side)
	}
	for i := 0; i < 1000; i++ {
		a := NextUpdate(r1, c.Steady)
		x := NextUpdate(r2, c.Steady)
		b.Update(a.OrderID, a.FillQty)
		cb.Update(x.OrderID, x.FillQty)
	}
	if cb.Hwm != b.Hwm || cb.BestBid != b.BestBid || cb.BestAsk != b.BestAsk {
		t.Fatalf("scalars diverge")
	}
	for id := int64(1); id <= int64(c.Steady); id++ {
		if cb.GetSlot(id) != b.GetSlot(id) {
			t.Fatalf("slot diverges for id %d", id)
		}
	}
	for tick := uint32(0); tick < c.Levels; tick++ {
		if cb.LevelQty(0, tick) != b.LevelQty(0, tick) || cb.LevelQty(1, tick) != b.LevelQty(1, tick) {
			t.Fatalf("level qty diverges at tick %d", tick)
		}
	}
	for slot := uint32(0); slot < cb.Hwm; slot++ {
		if *cb.OrderAt(slot) != b.Pool[slot] {
			t.Fatalf("order diverges at slot %d", slot)
		}
	}
}

func TestCaptureIsolatesRootFromLaterWrites(t *testing.T) {
	c := cowCfg()
	cb := NewCowBook(c)
	for i := 0; i < c.Steady; i++ {
		cb.Insert(int64(i)+1, int64(i%int(c.Levels)), 10, uint8(i%2))
	}
	root := cb.Capture()
	before := root.OrderAt(5).Filled
	cb.Update(6, 7) // order 6 lives in slot 5
	if root.OrderAt(5).Filled != before {
		t.Fatal("root saw a post-capture write")
	}
	if cb.OrderAt(5).Filled != before+7 {
		t.Fatal("writer did not advance")
	}
}

func TestSuccessiveCaptures(t *testing.T) {
	c := cowCfg()
	cb := NewCowBook(c)
	cb.Insert(1, 5, 10, 0)
	r1 := cb.Capture()
	cb.Update(1, 4)
	r2 := cb.Capture()
	if r1.OrderAt(0).Filled != 0 || r2.OrderAt(0).Filled != 4 {
		t.Fatalf("capture generations wrong: %d %d", r1.OrderAt(0).Filled, r2.OrderAt(0).Filled)
	}
}
```

- [ ] **Step 3: Run to verify pass**

Run: `cd go && go vet ./... && go test ./internal/smrcoll/ -run 'TestCow|TestCapture|TestSuccessive'`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
cd go && gofmt -l . && git add internal/smrcoll && git commit -m "feat(smr-collections): CowBook chunked copy-on-write store (Go)"
```

---

### Task G3: Go CoW snapshot codec + golden + concurrent test

**Files:**
- Create: `go/internal/smrcoll/cowsnapshot.go`
- Create: `go/internal/smrcoll/cowsnapshot_test.go`

**Interfaces:**
- Consumes: `CowRoot`/`CowBook` (G2), the `booksnap` SBE package exactly as `snapshot.go` uses it.
- Produces: `(*Snapshotter) EncodeRoot(r *CowRoot) []byte` (byte-identical to `Encode` for the same logical state) and `RestoreCow(data []byte, cfg bench.SmrConfig) (*CowBook, error)`.

- [ ] **Step 1: Create `cowsnapshot.go`** — mirrors `snapshot.go` field-for-field, reading through the root accessors (kept separate so the measured STW path is untouched):

```go
package smrcoll

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"hash/crc32"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll/booksnap"
)

// EncodeRoot serializes a frozen CowRoot (header + body + crc32c) into the
// reused buffer; byte-identical to Encode for the same logical state.
func (s *Snapshotter) EncodeRoot(r *CowRoot) []byte {
	s.buf.Reset()
	msg := &s.msg
	msg.PriceMin = r.PriceMin
	msg.TickSize = r.Tick
	msg.NLevels = r.NLevels
	msg.Capacity = r.Capacity
	msg.Hwm = r.Hwm
	msg.BestBid = r.BestBid
	msg.BestAsk = r.BestAsk

	msg.Levels = msg.Levels[:0]
	for side := uint8(0); side < 2; side++ {
		for t := uint32(0); t < r.NLevels; t++ {
			lvl := r.LevelAt(side, t)
			if lvl.Head == NIL {
				continue
			}
			msg.Levels = append(msg.Levels, booksnap.BookSnapshotLevels{
				Side: sideEnum(side), LevelTick: t,
				QtyTotal: lvl.QtyTotal, OrderCount: lvl.Count, Head: lvl.Head, Tail: lvl.Tail,
			})
		}
	}
	msg.Orders = msg.Orders[:0]
	for slot := uint32(0); slot < r.Hwm; slot++ {
		o := r.OrderAt(slot)
		msg.Orders = append(msg.Orders, booksnap.BookSnapshotOrders{
			Slot: slot, OrderId: o.OrderID, Price: o.Price, Qty: o.Qty, Filled: o.Filled,
			Side: sideEnum(o.Side), NextSlot: o.Next, Prev: o.Prev,
		})
	}

	hdr := booksnap.MessageHeader{
		BlockLength: msg.SbeBlockLength(), TemplateId: msg.SbeTemplateId(),
		SchemaId: msg.SbeSchemaId(), Version: msg.SbeSchemaVersion(),
	}
	_ = hdr.Encode(s.m, s.buf)
	_ = msg.Encode(s.m, s.buf, false)

	crc := crc32.Checksum(s.buf.Bytes(), crc32cTable)
	var tmp [4]byte
	binary.LittleEndian.PutUint32(tmp[:], crc)
	s.buf.Write(tmp[:])
	return s.buf.Bytes()
}

// RestoreCow rebuilds a fresh CowBook from an encoded image, verifying crc32c.
func RestoreCow(data []byte, cfg bench.SmrConfig) (*CowBook, error) {
	if len(data) < 4 {
		return nil, fmt.Errorf("snapshot too short")
	}
	sbeLen := len(data) - 4
	want := binary.LittleEndian.Uint32(data[sbeLen:])
	if crc32.Checksum(data[:sbeLen], crc32cTable) != want {
		return nil, fmt.Errorf("crc32c mismatch")
	}
	rd := bytes.NewReader(data[:sbeLen])
	m := booksnap.NewSbeGoMarshaller()
	var msg booksnap.BookSnapshot
	var hdr booksnap.MessageHeader
	if err := hdr.Decode(m, rd, msg.SbeSchemaVersion()); err != nil {
		return nil, err
	}
	if err := msg.Decode(m, rd, hdr.Version, hdr.BlockLength, false); err != nil {
		return nil, err
	}

	b := NewCowBook(cfg)
	b.PriceMin = msg.PriceMin
	b.Tick = msg.TickSize
	b.NLevels = msg.NLevels
	b.Hwm = msg.Hwm
	b.BestBid = msg.BestBid
	b.BestAsk = msg.BestAsk
	for i := range msg.Levels {
		lv := &msg.Levels[i]
		lvl := b.levelMut(sideU8(lv.Side), lv.LevelTick)
		lvl.Head = lv.Head
		lvl.Tail = lv.Tail
		lvl.QtyTotal = lv.QtyTotal
		lvl.Count = lv.OrderCount
	}
	for i := range msg.Orders {
		o := &msg.Orders[i]
		*b.orderMut(o.Slot) = Order{
			OrderID: o.OrderId, Price: o.Price, Qty: o.Qty, Filled: o.Filled,
			Next: o.NextSlot, Prev: o.Prev, Side: sideU8(o.Side),
		}
	}
	b.rebuildIDs()
	return b, nil
}
```

- [ ] **Step 2: Create `cowsnapshot_test.go`** (golden, equivalence, restore round-trip, concurrent capture — the last one is the `-race` target):

```go
package smrcoll

import (
	"bytes"
	"os"
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func cowGoldenCfg() bench.SmrConfig {
	return bench.SmrConfig{
		Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0,
		Steady: 2000, Warmup: 0, Iters: 0,
		Chunk: 512, LiveIters: 200000, SnapEvery: 20000,
	}
}

func buildCow(c bench.SmrConfig, n int) *CowBook {
	b := NewCowBook(c)
	rng := NewSplitMix(SmrSeed)
	for i := 0; i < n; i++ {
		ins := NextInsert(rng, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	return b
}

func TestCowBookMatchesGoldenBytes(t *testing.T) {
	c := cowGoldenCfg()
	cb := buildCow(c, c.Steady)
	root := cb.Capture()
	got := NewSnapshotter().EncodeRoot(root)
	want, err := os.ReadFile("../../../rust/smr-collections/testdata/golden_snapshot.bin")
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatalf("CowBook bytes differ from golden: got %d bytes, want %d", len(got), len(want))
	}
}

func TestCowEncodeEqualsStwEncodeAfterMixedOps(t *testing.T) {
	c := cowGoldenCfg()
	b := NewBook(c)
	cb := NewCowBook(c)
	r1 := NewSplitMix(SmrSeed)
	r2 := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		a := NextInsert(r1, i, c.Levels, c.Tick, c.PriceMin)
		x := NextInsert(r2, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(a.OrderID, a.Price, a.Qty, a.Side)
		cb.Insert(x.OrderID, x.Price, x.Qty, x.Side)
	}
	for i := 0; i < 500; i++ {
		a := NextUpdate(r1, c.Steady)
		x := NextUpdate(r2, c.Steady)
		b.Update(a.OrderID, a.FillQty)
		cb.Update(x.OrderID, x.FillQty)
	}
	stw := NewSnapshotter().Encode(b)
	cow := NewSnapshotter().EncodeRoot(cb.Capture())
	if !bytes.Equal(stw, cow) {
		t.Fatal("CoW bytes differ from STW bytes for identical state")
	}
}

func TestRestoreCowRoundTripAndCorruption(t *testing.T) {
	c := cowGoldenCfg()
	cb := buildCow(c, c.Steady)
	img := append([]byte(nil), NewSnapshotter().EncodeRoot(cb.Capture())...)
	r, err := RestoreCow(img, c)
	if err != nil {
		t.Fatal(err)
	}
	again := NewSnapshotter().EncodeRoot(r.Capture())
	if !bytes.Equal(img, again) {
		t.Fatal("restore does not round-trip")
	}
	bad := append([]byte(nil), img...)
	bad[0] ^= 0xFF
	if _, err := RestoreCow(bad, c); err == nil {
		t.Fatal("corrupt image accepted")
	}
}

// The concurrency correctness test (run under -race): capture at update k
// while the writer keeps going; the concurrently-encoded bytes must equal a
// single-threaded STW encode of a Book replayed to exactly k updates.
func TestConcurrentCaptureEqualsStwReplay(t *testing.T) {
	c := cowGoldenCfg()
	const totalUpdates, captureAt = 2000, 700

	ref := NewBook(c)
	rr := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		ins := NextInsert(rr, i, c.Levels, c.Tick, c.PriceMin)
		ref.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	for i := 0; i < captureAt; i++ {
		up := NextUpdate(rr, c.Steady)
		ref.Update(up.OrderID, up.FillQty)
	}
	want := append([]byte(nil), NewSnapshotter().Encode(ref)...)

	cb := buildCow(c, c.Steady)
	rng := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		_ = NextInsert(rng, i, c.Levels, c.Tick, c.PriceMin) // skip consumed draws
	}
	rootCh := make(chan *CowRoot, 1)
	gotCh := make(chan []byte, 1)
	go func() {
		root := <-rootCh
		gotCh <- append([]byte(nil), NewSnapshotter().EncodeRoot(root)...)
	}()
	for k := 0; k < totalUpdates; k++ {
		if k == captureAt {
			rootCh <- cb.Capture()
		}
		up := NextUpdate(rng, c.Steady)
		cb.Update(up.OrderID, up.FillQty)
	}
	got := <-gotCh
	if !bytes.Equal(want, got) {
		t.Fatal("concurrent capture differs from STW replay at the same position")
	}
}
```

- [ ] **Step 3: Run to verify pass (including race detector)**

Run: `cd go && go vet ./... && go test ./internal/smrcoll/ && go test -race ./internal/smrcoll/ -run TestConcurrentCapture`
Expected: PASS, no data-race reports.

- [ ] **Step 4: Commit**

```bash
cd go && gofmt -l . && git add internal/smrcoll && git commit -m "feat(smr-collections): Go CoW snapshot codec, golden + concurrent-capture tests"
```

---

### Task G4: Go `mvcc_*` artifacts

**Files:**
- Create: `go/cmd/smr-collections-mvcc_insert/main.go`
- Create: `go/cmd/smr-collections-mvcc_update/main.go`
- Create: `go/cmd/smr-collections-mvcc_snapshot/main.go`

**Interfaces:**
- Consumes: `CowBook` (G2), `EncodeRoot`/`RestoreCow` (G3), `bench` emit helpers.

- [ ] **Step 1: Create the three mains.** `mvcc_insert` and `mvcc_update` are the existing STW mains with `NewBook` → `NewCowBook` and the experiment renamed (`experiment = "mvcc_insert"` / `"mvcc_update"`); copy them from `cmd/smr-collections-insert/main.go` / `cmd/smr-collections-update/main.go` verbatim apart from those two changes (types line up: `CowBook` has the same `Insert`/`Update` signatures). `mvcc_snapshot/main.go` in full:

```go
// smr-collections-mvcc_snapshot (Go): capture+serialize and restore cost of
// the chunked-CoW book (single-threaded; the concurrent axis is live_mvcc).
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "mvcc_snapshot"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewCowBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	for i := 0; i < cfg.Steady; i++ {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	s := smrcoll.NewSnapshotter()
	snap := make([]int64, cfg.Iters)
	rest := make([]int64, cfg.Iters)
	var snapLen int
	for i := 0; i < cfg.Warmup; i++ {
		img := s.EncodeRoot(book.Capture())
		if _, err := smrcoll.RestoreCow(img, cfg); err != nil {
			bench.Fatalf("smr-collections-"+experiment, "%v", err)
		}
	}
	for k := 0; k < cfg.Iters; k++ {
		t0 := time.Now()
		img := s.EncodeRoot(book.Capture())
		snap[k] = time.Since(t0).Nanoseconds()
		snapLen = len(img)
		t1 := time.Now()
		if _, err := smrcoll.RestoreCow(img, cfg); err != nil {
			bench.Fatalf("smr-collections-"+experiment, "%v", err)
		}
		rest[k] = time.Since(t1).Nanoseconds()
	}
	bench.EmitSmrLatency(experiment, "snapshot", snap)
	bench.EmitSmrLatency(experiment, "restore", rest)
	bench.EmitSmrInt(experiment, "snapshot_bytes", int64(snapLen), "bytes", 1)
	mean := bench.Mean(snap)
	bench.EmitSmrFloat(experiment, "snapshot_throughput", float64(snapLen)/(mean/1e9), "bytes_per_sec", int64(cfg.Iters))
}
```

(`bench.EmitSmrLatency` sorts its input in place; that is harmless here because `bench.Mean` is order-insensitive. The call order above matches the existing STW `smr-collections-snapshot` main — keep it.)

- [ ] **Step 2: Build and smoke-run**

Run:
```bash
cd go && go build ./... && go vet ./...
SMRC_CAP=8192 SMRC_LEVELS=64 SMRC_STEADY=2000 SMRC_WARMUP=100 SMRC_ITERS=1000 go run ./cmd/smr-collections-mvcc_snapshot
SMRC_CAP=8192 SMRC_LEVELS=64 SMRC_STEADY=2000 SMRC_WARMUP=100 SMRC_ITERS=1000 go run ./cmd/smr-collections-mvcc_insert
SMRC_CAP=8192 SMRC_LEVELS=64 SMRC_STEADY=2000 SMRC_WARMUP=100 SMRC_ITERS=1000 go run ./cmd/smr-collections-mvcc_update
```
Expected: contract lines with experiments `mvcc_snapshot` (8 lines) / `mvcc_insert` / `mvcc_update` (3 lines each).

- [ ] **Step 3: Commit**

```bash
cd go && gofmt -l . && git add cmd && git commit -m "feat(smr-collections): Go mvcc_insert/mvcc_update/mvcc_snapshot cells"
```

---

### Task G5: Go live experiments (`live_stw`, `live_mvcc`)

**Files:**
- Modify: `go/internal/bench/smrcoll.go` (add `EmitSmrLive`)
- Create: `go/cmd/smr-collections-live_stw/main.go`
- Create: `go/cmd/smr-collections-live_mvcc/main.go`

**Interfaces:**
- Consumes: `Book`+`Snapshotter.Encode` (existing), `CowBook`/`CowRoot`/`EncodeRoot` (G2/G3).
- Produces: `bench.EmitSmrLive(experiment string, writerNs, snapNs []int64, skipped, snapLen int64)` emitting the Global-Constraints live metric set.

- [ ] **Step 1: Add `EmitSmrLive` to `go/internal/bench/smrcoll.go`:**

```go
// EmitSmrLive emits the live-experiment metric set: writer latency
// (p50/p99/mean + max), snapshot latency over completed snapshots, counts,
// and image size. Sorts writerNs/snapNs in place (like EmitSmrLatency).
func EmitSmrLive(experiment string, writerNs, snapNs []int64, skipped, snapLen int64) {
	var max int64
	for _, v := range writerNs {
		if v > max {
			max = v
		}
	}
	EmitSmrLatency(experiment, "writer", writerNs)
	EmitSmrInt(experiment, "writer_max", max, "ns", int64(len(writerNs)))
	EmitSmrLatency(experiment, "snapshot", snapNs)
	EmitSmrInt(experiment, "snap_count", int64(len(snapNs)), "count", 1)
	EmitSmrInt(experiment, "snap_skipped", skipped, "count", 1)
	EmitSmrInt(experiment, "snapshot_bytes", snapLen, "bytes", 1)
}
```

- [ ] **Step 2: Create `live_stw/main.go`:**

```go
// smr-collections-live_stw (Go): writer-observed latency while stop-the-world
// snapshots run inline at a fixed op cadence (the trigger op pays the whole
// serialize; writer_max is the stall).
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_stw"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	for i := 0; i < cfg.Steady; i++ {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	n := cfg.Steady
	for i := 0; i < cfg.Warmup; i++ {
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
	}
	s := smrcoll.NewSnapshotter()
	writerNs := make([]int64, cfg.LiveIters)
	snapNs := make([]int64, 0, cfg.LiveIters/cfg.SnapEvery+1)
	var snapLen int
	for k := 0; k < cfg.LiveIters; k++ {
		t0 := time.Now()
		if k%cfg.SnapEvery == 0 {
			img := s.Encode(book)
			snapLen = len(img)
			snapNs = append(snapNs, time.Since(t0).Nanoseconds())
		}
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
		writerNs[k] = time.Since(t0).Nanoseconds()
	}
	bench.EmitSmrLive(experiment, writerNs, snapNs, 0, int64(snapLen))
}
```

- [ ] **Step 3: Create `live_mvcc/main.go`:**

```go
// smr-collections-live_mvcc (Go): writer-observed latency while a serializer
// goroutine encodes captured CoW roots concurrently.
package main

import (
	"sync/atomic"
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_mvcc"

type capMsg struct {
	root *smrcoll.CowRoot
	t0   time.Time
}

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewCowBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	for i := 0; i < cfg.Steady; i++ {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	n := cfg.Steady
	for i := 0; i < cfg.Warmup; i++ {
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
	}

	var busy atomic.Bool
	ch := make(chan capMsg, 1)
	done := make(chan struct{})
	var snapNs []int64
	var snapLen int64
	go func() {
		s := smrcoll.NewSnapshotter()
		for m := range ch {
			img := s.EncodeRoot(m.root)
			snapLen = int64(len(img))
			snapNs = append(snapNs, time.Since(m.t0).Nanoseconds())
			busy.Store(false)
		}
		close(done)
	}()

	writerNs := make([]int64, cfg.LiveIters)
	var skipped int64
	for k := 0; k < cfg.LiveIters; k++ {
		t0 := time.Now()
		if k%cfg.SnapEvery == 0 {
			if busy.Load() {
				skipped++
			} else {
				busy.Store(true)
				ch <- capMsg{root: book.Capture(), t0: t0}
			}
		}
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
		writerNs[k] = time.Since(t0).Nanoseconds()
	}
	close(ch)
	<-done
	bench.EmitSmrLive(experiment, writerNs, snapNs, skipped, snapLen)
}
```

(`snapNs`/`snapLen` are written by the serializer goroutine and read by main only after `<-done` — the channel close/receive gives the happens-before edge; `go test -race` in G3 covers the shared machinery.)

- [ ] **Step 4: Build and smoke-run**

Run:
```bash
cd go && go build ./... && go vet ./...
SMRC_CAP=131072 SMRC_STEADY=60000 SMRC_WARMUP=1000 SMRC_LIVE_ITERS=50000 SMRC_SNAP_EVERY=10000 go run ./cmd/smr-collections-live_stw
SMRC_CAP=131072 SMRC_STEADY=60000 SMRC_WARMUP=1000 SMRC_LIVE_ITERS=50000 SMRC_SNAP_EVERY=10000 go run ./cmd/smr-collections-live_mvcc
```
Expected: 10 lines each; `live_stw` `writer_max` ≫ `writer_p99`; `snapshot_bytes` equal across the two.

- [ ] **Step 5: Commit**

```bash
cd go && gofmt -l . && git add internal/bench cmd && git commit -m "feat(smr-collections): Go live_stw/live_mvcc snapshot-under-writes cells"
```

---

### Task J1: Java config — extend `SmrConfig`

**Files:**
- Modify: `java/common/src/main/java/net/knego/hiperf/common/SmrConfig.java`
- Modify: every `new SmrConfig(...)` call site (grep `new SmrConfig(` under `java/` — the smr-collections test fixtures)

**Interfaces:**
- Produces: `SmrConfig` record gains components `int chunk, int liveIters, int snapEvery` (env `SMRC_CHUNK`/`SMRC_LIVE_ITERS`/`SMRC_SNAP_EVERY`, defaults 4096/200000/20000, validated `chunk <= cap`, `snapEvery <= liveIters`).

- [ ] **Step 1: Change the record and `fromEnv`:**

```java
public record SmrConfig(
        int cap, int levels, long tick, long priceMin, int steady, int warmup, int iters,
        int chunk, int liveIters, int snapEvery) {

    public static SmrConfig fromEnv() {
        int cap = Env.readPositiveInt("SMRC_CAP", 262144);
        int levels = Env.readPositiveInt("SMRC_LEVELS", 1024);
        long tick = Env.readPositiveInt("SMRC_TICK", 1);
        int steady = Env.readPositiveInt("SMRC_STEADY", 60000);
        int warmup = Env.readPositiveInt("SMRC_WARMUP", 10000);
        int iters = Env.readPositiveInt("SMRC_ITERS", 100000);
        long priceMin = readSignedLong("SMRC_PRICE_MIN", 0);
        int chunk = Env.readPositiveInt("SMRC_CHUNK", 4096);
        int liveIters = Env.readPositiveInt("SMRC_LIVE_ITERS", 200000);
        int snapEvery = Env.readPositiveInt("SMRC_SNAP_EVERY", 20000);
        if (levels > 65535) {
            throw new IllegalArgumentException("SMRC_LEVELS must be <= 65535");
        }
        if (steady > cap || steady > 65535) {
            throw new IllegalArgumentException("SMRC_STEADY must be <= SMRC_CAP and <= 65535");
        }
        if ((long) warmup + iters > cap) {
            throw new IllegalArgumentException("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP");
        }
        if (chunk > cap) {
            throw new IllegalArgumentException("SMRC_CHUNK must be <= SMRC_CAP");
        }
        if (snapEvery > liveIters) {
            throw new IllegalArgumentException("SMRC_SNAP_EVERY must be <= SMRC_LIVE_ITERS");
        }
        return new SmrConfig(cap, levels, tick, priceMin, steady, warmup, iters, chunk, liveIters, snapEvery);
    }
    // readSignedLong unchanged
}
```

- [ ] **Step 2: Fix call sites** — every existing `new SmrConfig(cap, levels, tick, priceMin, steady, warmup, iters)` in tests gains `, 4096, 200000, 20000` (grep: `grep -rn "new SmrConfig(" java/`).

- [ ] **Step 3: Run to verify pass**

Run: `cd java && ./gradlew build -q`
Expected: BUILD SUCCESSFUL (all existing tests green).

- [ ] **Step 4: Commit**

```bash
git add -A java && git commit -m "feat(smr-collections): Java SMRC_CHUNK/SMRC_LIVE_ITERS/SMRC_SNAP_EVERY config"
```

---

### Task J2: Java `CowBook` core (SoA chunks)

**Files:**
- Create: `java/smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/CowBook.java`
- Create: `java/smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/CowRoot.java`
- Create: `java/smr-collections-common/src/test/java/net/knego/hiperf/smrcollections/CowBookTest.java`

**Interfaces:**
- Consumes: `SmrConfig` (J1), `Book.NIL`, Agrona `Long2LongHashMap`.
- Produces: `CowBook(SmrConfig)` with `insert(long orderId, long price, long qty, byte side)`, `update(long orderId, long fillQty)`, `capture() -> CowRoot`, `getSlot(long) -> int`, `levelQty(byte side, int tick) -> long`, `orderFilled(int slot) -> long`, package-visible `rebuildIds()`, public scalar fields (`priceMin`, `tick`, `nLevels`, `chunk`, `capacity`, `hwm`, `bestBid`, `bestAsk`), `public static final int LEVEL_CHUNK = 256`; nested package-visible `OrderChunk`/`LvlChunk` (SoA primitive arrays; a chunk copy is `System.arraycopy`, never per-object cloning — Java's equivalent of the POD chunk); `CowRoot` with public scalars, package-visible chunk refs, helpers `lvl(byte side, int t)`, `ord(int slot)`, and `public long orderFilled(int slot)`.

- [ ] **Step 1: Create `CowBook.java`:**

```java
package net.knego.hiperf.smrcollections;

import java.util.Arrays;
import net.knego.hiperf.common.SmrConfig;
import org.agrona.collections.Long2LongHashMap;

/**
 * Chunked copy-on-write LOB: same logical behavior as {@link Book}, but the
 * pool and ladder live in fixed-size structure-of-arrays chunks (parallel
 * primitive arrays, so a chunk copy is arraycopy — never per-object cloning).
 * {@link #capture()} clones the chunk-ref arrays (O(#chunks)) and bumps the
 * generation; the writer copies a chunk before its first write after a capture
 * ({@code born < gen}), so a frozen {@link CowRoot} is never mutated. GC
 * reclaims dropped chunks. The copy decision is ALWAYS the epoch.
 */
public final class CowBook {
    public static final int LEVEL_CHUNK = 256;

    static final class OrderChunk {
        long born;
        final long[] orderId, price, qty, filled;
        final int[] next, prev;
        final byte[] side;

        OrderChunk(long born, int n) {
            this.born = born;
            orderId = new long[n];
            price = new long[n];
            qty = new long[n];
            filled = new long[n];
            next = new int[n];
            prev = new int[n];
            side = new byte[n];
        }

        OrderChunk copyFor(long gen) {
            int n = orderId.length;
            OrderChunk c = new OrderChunk(gen, n);
            System.arraycopy(orderId, 0, c.orderId, 0, n);
            System.arraycopy(price, 0, c.price, 0, n);
            System.arraycopy(qty, 0, c.qty, 0, n);
            System.arraycopy(filled, 0, c.filled, 0, n);
            System.arraycopy(next, 0, c.next, 0, n);
            System.arraycopy(prev, 0, c.prev, 0, n);
            System.arraycopy(side, 0, c.side, 0, n);
            return c;
        }
    }

    static final class LvlChunk {
        long born;
        final long[] qtyTotal;
        final int[] head, tail, count;

        LvlChunk(long born, int n) {
            this.born = born;
            qtyTotal = new long[n];
            head = new int[n];
            tail = new int[n];
            count = new int[n];
            Arrays.fill(head, Book.NIL);
            Arrays.fill(tail, Book.NIL);
        }

        LvlChunk copyFor(long gen) {
            int n = qtyTotal.length;
            LvlChunk c = new LvlChunk(gen, n);
            System.arraycopy(qtyTotal, 0, c.qtyTotal, 0, n);
            System.arraycopy(head, 0, c.head, 0, n);
            System.arraycopy(tail, 0, c.tail, 0, n);
            System.arraycopy(count, 0, c.count, 0, n);
            return c;
        }
    }

    public final long priceMin;
    public final long tick;
    public final int nLevels;
    public final int chunk;
    public final int capacity;
    private long gen = 1;
    final OrderChunk[] orderChunks;
    final LvlChunk[] bidChunks;
    final LvlChunk[] askChunks;
    public int hwm;
    public int bestBid = -1;
    public int bestAsk = -1;
    private final Long2LongHashMap ids = new Long2LongHashMap(Book.NIL);

    public CowBook(SmrConfig cfg) {
        this.priceMin = cfg.priceMin();
        this.tick = cfg.tick();
        this.nLevels = cfg.levels();
        this.chunk = cfg.chunk();
        this.capacity = cfg.cap();
        int nOC = (capacity + chunk - 1) / chunk;
        orderChunks = new OrderChunk[nOC];
        for (int ci = 0; ci < nOC; ci++) {
            orderChunks[ci] = new OrderChunk(1, Math.min(chunk, capacity - ci * chunk));
        }
        int nLC = (nLevels + LEVEL_CHUNK - 1) / LEVEL_CHUNK;
        bidChunks = new LvlChunk[nLC];
        askChunks = new LvlChunk[nLC];
        for (int ci = 0; ci < nLC; ci++) {
            int n = Math.min(LEVEL_CHUNK, nLevels - ci * LEVEL_CHUNK);
            bidChunks[ci] = new LvlChunk(1, n);
            askChunks[ci] = new LvlChunk(1, n);
        }
    }

    private int tickOf(long price) {
        return (int) ((price - priceMin) / tick);
    }

    private LvlChunk[] lane(byte side) {
        return side == 0 ? bidChunks : askChunks;
    }

    private OrderChunk orderChunkForWrite(int slot) {
        int ci = slot / chunk;
        OrderChunk c = orderChunks[ci];
        if (c.born < gen) {
            c = c.copyFor(gen);
            orderChunks[ci] = c;
        }
        return c;
    }

    private LvlChunk lvlChunkForWrite(byte side, int t) {
        LvlChunk[] lane = lane(side);
        int ci = t / LEVEL_CHUNK;
        LvlChunk c = lane[ci];
        if (c.born < gen) {
            c = c.copyFor(gen);
            lane[ci] = c;
        }
        return c;
    }

    /** Same op semantics as {@link Book#insert} (keep in lockstep). */
    public void insert(long orderId, long price, long qty, byte side) {
        int t = tickOf(price);
        int slot = hwm++;
        LvlChunk lc = lvlChunkForWrite(side, t);
        int lo = t % LEVEL_CHUNK;
        int prevTail = lc.tail[lo];
        OrderChunk oc = orderChunkForWrite(slot);
        int oo = slot % chunk;
        oc.orderId[oo] = orderId;
        oc.price[oo] = price;
        oc.qty[oo] = qty;
        oc.filled[oo] = 0;
        oc.side[oo] = side;
        oc.next[oo] = Book.NIL;
        oc.prev[oo] = prevTail;
        if (prevTail != Book.NIL) {
            OrderChunk pc = orderChunkForWrite(prevTail);
            pc.next[prevTail % chunk] = slot;
        } else {
            lc.head[lo] = slot;
        }
        lc.tail[lo] = slot;
        lc.qtyTotal[lo] += qty;
        lc.count[lo]++;
        ids.put(orderId, slot);
        if (side == 0 && (bestBid < 0 || t > bestBid)) {
            bestBid = t;
        }
        if (side == 1 && (bestAsk < 0 || t < bestAsk)) {
            bestAsk = t;
        }
    }

    /** Same op semantics as {@link Book#update} (keep in lockstep). */
    public void update(long orderId, long fillQty) {
        int slot = (int) ids.get(orderId);
        OrderChunk oc = orderChunkForWrite(slot);
        int oo = slot % chunk;
        long add = Math.min(fillQty, oc.qty[oo] - oc.filled[oo]);
        oc.filled[oo] += add;
        int t = tickOf(oc.price[oo]);
        LvlChunk lc = lvlChunkForWrite(oc.side[oo], t);
        lc.qtyTotal[t % LEVEL_CHUNK] -= add;
    }

    /** Freeze the current state (O(#chunks)) and bump the generation. */
    public CowRoot capture() {
        CowRoot r = new CowRoot(priceMin, tick, nLevels, capacity, hwm, bestBid, bestAsk, chunk,
                orderChunks.clone(), bidChunks.clone(), askChunks.clone());
        gen++;
        return r;
    }

    public int getSlot(long orderId) {
        return (int) ids.get(orderId);
    }

    public long levelQty(byte side, int t) {
        return lane(side)[t / LEVEL_CHUNK].qtyTotal[t % LEVEL_CHUNK];
    }

    public long orderFilled(int slot) {
        return orderChunks[slot / chunk].filled[slot % chunk];
    }

    /** Re-index the id-map from the pool (used after restore). */
    void rebuildIds() {
        ids.clear();
        for (int slot = 0; slot < hwm; slot++) {
            OrderChunk oc = orderChunks[slot / chunk];
            ids.put(oc.orderId[slot % chunk], slot);
        }
    }
}
```

**Caveat baked into `insert`:** the `lc` reference is taken before the `orderChunkForWrite(prevTail)` call — safe only because level chunks and order chunks are disjoint tables. Never hold an `OrderChunk` reference across another `orderChunkForWrite` call (it may replace the chunk), and likewise for levels.

- [ ] **Step 2: Create `CowRoot.java`:**

```java
package net.knego.hiperf.smrcollections;

/** A frozen point-in-time view: chunk refs + scalars; never mutated after capture. */
public final class CowRoot {
    public final long priceMin;
    public final long tick;
    public final int nLevels;
    public final int capacity;
    public final int hwm;
    public final int bestBid;
    public final int bestAsk;
    final int chunk;
    final CowBook.OrderChunk[] orderChunks;
    final CowBook.LvlChunk[] bidChunks;
    final CowBook.LvlChunk[] askChunks;

    CowRoot(long priceMin, long tick, int nLevels, int capacity, int hwm, int bestBid, int bestAsk,
            int chunk, CowBook.OrderChunk[] orderChunks, CowBook.LvlChunk[] bidChunks, CowBook.LvlChunk[] askChunks) {
        this.priceMin = priceMin;
        this.tick = tick;
        this.nLevels = nLevels;
        this.capacity = capacity;
        this.hwm = hwm;
        this.bestBid = bestBid;
        this.bestAsk = bestAsk;
        this.chunk = chunk;
        this.orderChunks = orderChunks;
        this.bidChunks = bidChunks;
        this.askChunks = askChunks;
    }

    CowBook.LvlChunk lvl(byte side, int t) {
        return (side == 0 ? bidChunks : askChunks)[t / CowBook.LEVEL_CHUNK];
    }

    CowBook.OrderChunk ord(int slot) {
        return orderChunks[slot / chunk];
    }

    public long orderFilled(int slot) {
        return ord(slot).filled[slot % chunk];
    }
}
```

- [ ] **Step 3: Create `CowBookTest.java`** (mirrors the Rust/Go equivalence + isolation tests):

```java
package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertEquals;

import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class CowBookTest {

    private static SmrConfig cfg() {
        return new SmrConfig(1024, 300, 1, 0, 500, 0, 0, 64, 200000, 20000);
    }

    @Test
    void cowBookMatchesBookQueriesAfterMixedOps() {
        SmrConfig c = cfg();
        Book b = new Book(c);
        CowBook cb = new CowBook(c);
        Workload.SplitMix r1 = new Workload.SplitMix(Workload.SEED);
        Workload.SplitMix r2 = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ia = new Workload.Insert();
        Workload.Insert ix = new Workload.Insert();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(r1, i, c.levels(), c.tick(), c.priceMin(), ia);
            Workload.nextInsert(r2, i, c.levels(), c.tick(), c.priceMin(), ix);
            b.insert(ia.orderId, ia.price, ia.qty, ia.side);
            cb.insert(ix.orderId, ix.price, ix.qty, ix.side);
        }
        Workload.Update ua = new Workload.Update();
        Workload.Update ux = new Workload.Update();
        for (int i = 0; i < 1000; i++) {
            Workload.nextUpdate(r1, c.steady(), ua);
            Workload.nextUpdate(r2, c.steady(), ux);
            b.update(ua.orderId, ua.fillQty);
            cb.update(ux.orderId, ux.fillQty);
        }
        assertEquals(b.hwm(), cb.hwm);
        assertEquals(b.bestBid(), cb.bestBid);
        assertEquals(b.bestAsk(), cb.bestAsk);
        for (long id = 1; id <= c.steady(); id++) {
            assertEquals(b.getSlot(id), cb.getSlot(id));
        }
        for (int t = 0; t < c.levels(); t++) {
            assertEquals(b.levelQty((byte) 0, t), cb.levelQty((byte) 0, t));
            assertEquals(b.levelQty((byte) 1, t), cb.levelQty((byte) 1, t));
        }
        for (int slot = 0; slot < cb.hwm; slot++) {
            assertEquals(b.pool[slot].filled, cb.orderFilled(slot));
        }
    }

    @Test
    void captureIsolatesRootFromLaterWrites() {
        SmrConfig c = cfg();
        CowBook cb = new CowBook(c);
        for (int i = 0; i < c.steady(); i++) {
            cb.insert(i + 1, i % c.levels(), 10, (byte) (i % 2));
        }
        CowRoot root = cb.capture();
        long before = root.orderFilled(5);
        cb.update(6, 7); // order 6 lives in slot 5
        assertEquals(before, root.orderFilled(5), "root must be frozen");
        assertEquals(before + 7, cb.orderFilled(5), "writer must advance");
    }

    @Test
    void successiveCapturesSeeSuccessiveStates() {
        SmrConfig c = cfg();
        CowBook cb = new CowBook(c);
        cb.insert(1, 5, 10, (byte) 0);
        CowRoot r1 = cb.capture();
        cb.update(1, 4);
        CowRoot r2 = cb.capture();
        assertEquals(0, r1.orderFilled(0));
        assertEquals(4, r2.orderFilled(0));
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd java && ./gradlew :smr-collections-common:test -q`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 5: Commit**

```bash
git add -A java/smr-collections-common
git commit -m "feat(smr-collections): CowBook chunked copy-on-write store (Java, SoA chunks)"
```

---

### Task J3: Java CoW snapshot codec + golden + concurrent test

**Files:**
- Create: `java/smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/CowSnapshotter.java`
- Create: `java/smr-collections-common/src/test/java/net/knego/hiperf/smrcollections/CowSnapshotTest.java`

**Interfaces:**
- Consumes: `CowRoot`/`CowBook` (J2), the generated `booksnap` SBE classes exactly as `Snapshotter` uses them.
- Produces: `CowSnapshotter(int maxBytes)` with `int encodeRoot(CowRoot r)` (byte-identical to `Snapshotter.encode` for the same logical state), `byte[] backing()`, `int lastLen()`, and `static CowBook restoreCow(byte[] data, int len, SmrConfig cfg)`.

- [ ] **Step 1: Create `CowSnapshotter.java`** — mirrors `Snapshotter` field-for-field, reading through the root chunks (kept separate so the measured STW path is untouched):

```java
package net.knego.hiperf.smrcollections;

import booksnap.BookSnapshotDecoder;
import booksnap.BookSnapshotEncoder;
import booksnap.MessageHeaderDecoder;
import booksnap.MessageHeaderEncoder;
import booksnap.Side;
import java.nio.ByteOrder;
import java.util.zip.CRC32C;
import net.knego.hiperf.common.SmrConfig;
import org.agrona.concurrent.UnsafeBuffer;

/** SBE codec over a frozen CowRoot; byte-identical to {@link Snapshotter}. */
public final class CowSnapshotter {

    private final byte[] backing;
    private final UnsafeBuffer buffer;
    private final MessageHeaderEncoder headerEnc = new MessageHeaderEncoder();
    private final BookSnapshotEncoder enc = new BookSnapshotEncoder();
    private int lastLen;

    public CowSnapshotter(int maxBytes) {
        this.backing = new byte[maxBytes];
        this.buffer = new UnsafeBuffer(backing);
    }

    private static long u32(int v) {
        return v & 0xFFFFFFFFL;
    }

    /** Encode the frozen root; returns total length (SBE + crc32c). */
    public int encodeRoot(CowRoot r) {
        enc.wrapAndApplyHeader(buffer, 0, headerEnc);
        enc.priceMin(r.priceMin);
        enc.tickSize(r.tick);
        enc.nLevels(u32(r.nLevels));
        enc.capacity(u32(r.capacity));
        enc.hwm(u32(r.hwm));
        enc.bestBid(r.bestBid);
        enc.bestAsk(r.bestAsk);

        int levelCount = 0;
        for (byte side = 0; side < 2; side++) {
            for (int t = 0; t < r.nLevels; t++) {
                if (r.lvl(side, t).head[t % CowBook.LEVEL_CHUNK] != Book.NIL) {
                    levelCount++;
                }
            }
        }
        BookSnapshotEncoder.LevelsEncoder lg = enc.levelsCount(levelCount);
        for (byte side = 0; side < 2; side++) {
            for (int t = 0; t < r.nLevels; t++) {
                CowBook.LvlChunk c = r.lvl(side, t);
                int lo = t % CowBook.LEVEL_CHUNK;
                if (c.head[lo] == Book.NIL) {
                    continue;
                }
                lg.next();
                lg.side(side == 0 ? Side.BID : Side.ASK);
                lg.levelTick(u32(t));
                lg.qtyTotal(c.qtyTotal[lo]);
                lg.orderCount(u32(c.count[lo]));
                lg.head(u32(c.head[lo]));
                lg.tail(u32(c.tail[lo]));
            }
        }

        BookSnapshotEncoder.OrdersEncoder og = enc.ordersCount(r.hwm);
        for (int slot = 0; slot < r.hwm; slot++) {
            CowBook.OrderChunk c = r.ord(slot);
            int oo = slot % r.chunk;
            og.next();
            og.slot(u32(slot));
            og.orderId(c.orderId[oo]);
            og.price(c.price[oo]);
            og.qty(c.qty[oo]);
            og.filled(c.filled[oo]);
            og.side(c.side[oo] == 0 ? Side.BID : Side.ASK);
            og.nextSlot(u32(c.next[oo]));
            og.prev(u32(c.prev[oo]));
        }

        int sbeLen = enc.limit();
        CRC32C crc = new CRC32C();
        crc.update(backing, 0, sbeLen);
        buffer.putInt(sbeLen, (int) crc.getValue(), ByteOrder.LITTLE_ENDIAN);
        lastLen = sbeLen + 4;
        return lastLen;
    }

    public byte[] backing() {
        return backing;
    }

    public int lastLen() {
        return lastLen;
    }

    /** Restore a fresh CowBook, verifying the crc32c trailer. */
    public static CowBook restoreCow(byte[] data, int len, SmrConfig cfg) {
        if (len < 4) {
            throw new IllegalArgumentException("snapshot too short");
        }
        int sbeLen = len - 4;
        UnsafeBuffer buf = new UnsafeBuffer(data, 0, len);
        CRC32C crc = new CRC32C();
        crc.update(data, 0, sbeLen);
        int want = buf.getInt(sbeLen, ByteOrder.LITTLE_ENDIAN);
        if ((int) crc.getValue() != want) {
            throw new IllegalArgumentException("crc32c mismatch");
        }
        MessageHeaderDecoder header = new MessageHeaderDecoder();
        header.wrap(buf, 0);
        BookSnapshotDecoder dec = new BookSnapshotDecoder();
        dec.wrap(buf, header.encodedLength(), header.blockLength(), header.version());

        CowBook b = new CowBook(cfg);
        // priceMin/tick/nLevels are final (from cfg); wire values equal cfg by
        // construction, as in Snapshotter.restore.
        b.hwm = (int) dec.hwm();
        b.bestBid = dec.bestBid();
        b.bestAsk = dec.bestAsk();

        BookSnapshotDecoder.LevelsDecoder levels = dec.levels();
        while (levels.hasNext()) {
            levels.next();
            byte side = (byte) (levels.side() == Side.ASK ? 1 : 0);
            int t = (int) levels.levelTick();
            CowBook.LvlChunk c = (side == 0 ? b.bidChunks : b.askChunks)[t / CowBook.LEVEL_CHUNK];
            int lo = t % CowBook.LEVEL_CHUNK;
            c.qtyTotal[lo] = levels.qtyTotal();
            c.count[lo] = (int) levels.orderCount();
            c.head[lo] = (int) levels.head();
            c.tail[lo] = (int) levels.tail();
        }
        BookSnapshotDecoder.OrdersDecoder orders = dec.orders();
        while (orders.hasNext()) {
            orders.next();
            int slot = (int) orders.slot();
            CowBook.OrderChunk c = b.orderChunks[slot / b.chunk];
            int oo = slot % b.chunk;
            c.orderId[oo] = orders.orderId();
            c.price[oo] = orders.price();
            c.qty[oo] = orders.qty();
            c.filled[oo] = orders.filled();
            c.side[oo] = (byte) (orders.side() == Side.ASK ? 1 : 0);
            c.next[oo] = (int) orders.nextSlot();
            c.prev[oo] = (int) orders.prev();
        }
        b.rebuildIds();
        return b;
    }
}
```

- [ ] **Step 2: Create `CowSnapshotTest.java`** (golden, STW-equivalence, restore round-trip + corruption, concurrent capture):

```java
package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.concurrent.SynchronousQueue;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class CowSnapshotTest {

    private static SmrConfig goldenCfg() {
        return new SmrConfig(4096, 64, 1, 0, 2000, 0, 0, 512, 200000, 20000);
    }

    private static CowBook buildCow(SmrConfig c, int n) {
        CowBook b = new CowBook(c);
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        for (int i = 0; i < n; i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins);
            b.insert(ins.orderId, ins.price, ins.qty, ins.side);
        }
        return b;
    }

    private static int maxBytes(SmrConfig c) {
        return 64 + c.cap() * 64 + c.levels() * 2 * 32;
    }

    @Test
    void cowBookMatchesGoldenBytes() throws Exception {
        SmrConfig c = goldenCfg();
        CowBook cb = buildCow(c, c.steady());
        CowSnapshotter s = new CowSnapshotter(maxBytes(c));
        int len = s.encodeRoot(cb.capture());
        byte[] got = Arrays.copyOf(s.backing(), len);
        byte[] want = Files.readAllBytes(
                Path.of("../../rust/smr-collections/testdata/golden_snapshot.bin"));
        assertArrayEquals(want, got, "CowBook bytes == golden bytes");
    }

    @Test
    void cowEncodeEqualsStwEncodeAfterMixedOps() {
        SmrConfig c = goldenCfg();
        Book b = new Book(c);
        CowBook cb = new CowBook(c);
        Workload.SplitMix r1 = new Workload.SplitMix(Workload.SEED);
        Workload.SplitMix r2 = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ia = new Workload.Insert();
        Workload.Insert ix = new Workload.Insert();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(r1, i, c.levels(), c.tick(), c.priceMin(), ia);
            Workload.nextInsert(r2, i, c.levels(), c.tick(), c.priceMin(), ix);
            b.insert(ia.orderId, ia.price, ia.qty, ia.side);
            cb.insert(ix.orderId, ix.price, ix.qty, ix.side);
        }
        Workload.Update ua = new Workload.Update();
        Workload.Update ux = new Workload.Update();
        for (int i = 0; i < 500; i++) {
            Workload.nextUpdate(r1, c.steady(), ua);
            Workload.nextUpdate(r2, c.steady(), ux);
            b.update(ua.orderId, ua.fillQty);
            cb.update(ux.orderId, ux.fillQty);
        }
        Snapshotter stw = new Snapshotter(maxBytes(c));
        int n1 = stw.encode(b);
        CowSnapshotter cow = new CowSnapshotter(maxBytes(c));
        int n2 = cow.encodeRoot(cb.capture());
        assertArrayEquals(
                Arrays.copyOf(stw.backing(), n1), Arrays.copyOf(cow.backing(), n2));
    }

    @Test
    void restoreCowRoundTripsAndRejectsCorruption() {
        SmrConfig c = goldenCfg();
        CowBook cb = buildCow(c, c.steady());
        CowSnapshotter s = new CowSnapshotter(maxBytes(c));
        int len = s.encodeRoot(cb.capture());
        byte[] img = Arrays.copyOf(s.backing(), len);
        CowBook r = CowSnapshotter.restoreCow(img, len, c);
        CowSnapshotter s2 = new CowSnapshotter(maxBytes(c));
        int len2 = s2.encodeRoot(r.capture());
        assertArrayEquals(img, Arrays.copyOf(s2.backing(), len2));
        byte[] bad = img.clone();
        bad[0] ^= 0xFF;
        assertThrows(IllegalArgumentException.class, () -> CowSnapshotter.restoreCow(bad, len, c));
    }

    /** Capture at update k under concurrent encoding == STW replay to k. */
    @Test
    void concurrentCaptureEqualsStwReplay() throws Exception {
        SmrConfig c = goldenCfg();
        final int totalUpdates = 2000;
        final int captureAt = 700;

        Book ref = new Book(c);
        Workload.SplitMix rr = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        Workload.Update up = new Workload.Update();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(rr, i, c.levels(), c.tick(), c.priceMin(), ins);
            ref.insert(ins.orderId, ins.price, ins.qty, ins.side);
        }
        for (int i = 0; i < captureAt; i++) {
            Workload.nextUpdate(rr, c.steady(), up);
            ref.update(up.orderId, up.fillQty);
        }
        Snapshotter stw = new Snapshotter(maxBytes(c));
        int wn = stw.encode(ref);
        byte[] want = Arrays.copyOf(stw.backing(), wn);

        CowBook cb = buildCow(c, c.steady());
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins); // skip consumed draws
        }
        SynchronousQueue<CowRoot> rootQ = new SynchronousQueue<>();
        SynchronousQueue<byte[]> gotQ = new SynchronousQueue<>();
        Thread ser = new Thread(() -> {
            try {
                CowRoot root = rootQ.take();
                CowSnapshotter s = new CowSnapshotter(maxBytes(goldenCfg()));
                int n = s.encodeRoot(root);
                gotQ.put(Arrays.copyOf(s.backing(), n));
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        });
        ser.start();
        for (int k = 0; k < totalUpdates; k++) {
            if (k == captureAt) {
                rootQ.put(cb.capture());
            }
            Workload.nextUpdate(rng, c.steady(), up);
            cb.update(up.orderId, up.fillQty);
        }
        byte[] got = gotQ.take();
        ser.join();
        assertArrayEquals(want, got, "concurrent capture == STW replay");
        assertEquals(want.length, got.length);
    }
}
```

(The golden path `../../rust/...` is relative to the `smr-collections-common` subproject dir, matching `GoldenTest.java` — copy the exact relative prefix from that file if it differs.)

- [ ] **Step 3: Run to verify pass**

Run: `cd java && ./gradlew :smr-collections-common:test -q`
Expected: BUILD SUCCESSFUL. The golden test is the byte-identity gate.

- [ ] **Step 4: Commit**

```bash
git add -A java/smr-collections-common
git commit -m "feat(smr-collections): Java CoW snapshot codec, golden + concurrent-capture tests"
```

---

### Task J4: Java `mvcc_*` artifacts

**Files:**
- Create: `java/smr-collections-mvcc_insert/{build.gradle.kts,src/main/java/net/knego/hiperf/smrcollections/mvccinsert/Main.java}`
- Create: `java/smr-collections-mvcc_update/{build.gradle.kts,src/main/java/net/knego/hiperf/smrcollections/mvccupdate/Main.java}`
- Create: `java/smr-collections-mvcc_snapshot/{build.gradle.kts,src/main/java/net/knego/hiperf/smrcollections/mvccsnapshot/Main.java}`
- Modify: `java/settings.gradle.kts`

**Interfaces:**
- Consumes: `CowBook` (J2), `CowSnapshotter` (J3), `SmrCollections` emit helpers.

- [ ] **Step 1: Create the three subprojects.** Each `build.gradle.kts` copies `smr-collections-insert/build.gradle.kts` with the right `mainClass` (e.g. `net.knego.hiperf.smrcollections.mvccinsert.Main`). `mvcc_insert` Main:

```java
package net.knego.hiperf.smrcollections.mvccinsert;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.CowBook;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/mvcc_insert (Java): insert cost on the chunked-CoW book. */
public final class Main {
    private static final String EXPERIMENT = "mvcc_insert";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            int[] i = {0};
            long[] samples = SmrCollections.measure(cfg.warmup(), cfg.iters(), () -> {
                Workload.nextInsert(rng, i[0], cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
                i[0]++;
            });
            SmrCollections.emitLatency(EXPERIMENT, "insert", samples);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

`mvcc_update` Main (package `...mvccupdate`):

```java
package net.knego.hiperf.smrcollections.mvccupdate;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.CowBook;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/mvcc_update (Java): partial-fill cost on the chunked-CoW book. */
public final class Main {
    private static final String EXPERIMENT = "mvcc_update";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            for (int i = 0; i < cfg.steady(); i++) {
                Workload.nextInsert(rng, i, cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
            }
            int n = cfg.steady();
            Workload.Update up = new Workload.Update();
            long[] samples = SmrCollections.measure(cfg.warmup(), cfg.iters(), () -> {
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
            });
            SmrCollections.emitLatency(EXPERIMENT, "update", samples);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

`mvcc_snapshot` Main (package `...mvccsnapshot`) mirrors the STW snapshot Main with `Book`→`CowBook`, `Snapshotter`→`CowSnapshotter`, encode → `s.encodeRoot(book.capture())`, restore → `CowSnapshotter.restoreCow(s.backing(), len, cfg)`, `EXPERIMENT = "mvcc_snapshot"`; keep the identical metric emission block (`snapshot`/`restore` latency, `snapshot_bytes`, `snapshot_throughput`).

Add to `settings.gradle.kts` after `"smr-collections-snapshot",`:

```kotlin
    "smr-collections-mvcc_insert",
    "smr-collections-mvcc_update",
    "smr-collections-mvcc_snapshot",
```

- [ ] **Step 2: Build and smoke-run**

Run:
```bash
cd java && ./gradlew build -q
SMRC_CAP=8192 SMRC_LEVELS=64 SMRC_STEADY=2000 SMRC_WARMUP=100 SMRC_ITERS=1000 ./gradlew :smr-collections-mvcc_snapshot:run -q
```
Expected: 8 contract lines, experiment `mvcc_snapshot`. Run the other two the same way (3 lines each).

- [ ] **Step 3: Commit**

```bash
git add -A java && git commit -m "feat(smr-collections): Java mvcc_insert/mvcc_update/mvcc_snapshot cells"
```

---

### Task J5: Java live experiments (`live_stw`, `live_mvcc`)

**Files:**
- Modify: `java/common/src/main/java/net/knego/hiperf/common/SmrCollections.java` (add `emitLive`)
- Create: `java/smr-collections-live_stw/{build.gradle.kts,src/main/java/net/knego/hiperf/smrcollections/livestw/Main.java}`
- Create: `java/smr-collections-live_mvcc/{build.gradle.kts,src/main/java/net/knego/hiperf/smrcollections/livemvcc/Main.java}`
- Modify: `java/settings.gradle.kts`

**Interfaces:**
- Consumes: `Book`+`Snapshotter` (existing), `CowBook`/`CowRoot`/`CowSnapshotter` (J2/J3).
- Produces: `SmrCollections.emitLive(String experiment, long[] writerNs, long[] snapNs, long skipped, long snapLen)`.

- [ ] **Step 1: Add `emitLive` to `SmrCollections.java`** (compute max BEFORE `emitLatency` — it sorts in place):

```java
    /** Live-experiment metric set: writer latency (+max), snapshot latency, counts, size. */
    public static void emitLive(String experiment, long[] writerNs, long[] snapNs, long skipped, long snapLen) {
        long max = 0;
        for (long v : writerNs) {
            if (v > max) {
                max = v;
            }
        }
        emitLatency(experiment, "writer", writerNs);
        emitInt(experiment, "writer_max", max, "ns", writerNs.length);
        emitLatency(experiment, "snapshot", snapNs);
        emitInt(experiment, "snap_count", snapNs.length, "count", 1);
        emitInt(experiment, "snap_skipped", skipped, "count", 1);
        emitInt(experiment, "snapshot_bytes", snapLen, "bytes", 1);
    }
```

- [ ] **Step 2: Create `live_stw` Main** (package `...livestw`):

```java
package net.knego.hiperf.smrcollections.livestw;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Snapshotter;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/live_stw (Java): writer latency with inline STW snapshots. */
public final class Main {
    private static final String EXPERIMENT = "live_stw";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            Book book = new Book(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            for (int i = 0; i < cfg.steady(); i++) {
                Workload.nextInsert(rng, i, cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
            }
            int n = cfg.steady();
            Workload.Update up = new Workload.Update();
            for (int i = 0; i < cfg.warmup(); i++) {
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
            }
            Snapshotter s = new Snapshotter(64 + cfg.cap() * 64 + cfg.levels() * 2 * 32);
            long[] writerNs = new long[cfg.liveIters()];
            long[] snapNs = new long[cfg.liveIters() / cfg.snapEvery() + 1];
            int snapCount = 0;
            long snapLen = 0;
            for (int k = 0; k < cfg.liveIters(); k++) {
                long t0 = System.nanoTime();
                if (k % cfg.snapEvery() == 0) {
                    snapLen = s.encode(book);
                    snapNs[snapCount++] = System.nanoTime() - t0;
                }
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
                writerNs[k] = System.nanoTime() - t0;
            }
            SmrCollections.emitLive(EXPERIMENT, writerNs, java.util.Arrays.copyOf(snapNs, snapCount), 0, snapLen);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 3: Create `live_mvcc` Main** (package `...livemvcc`; serializer thread + `ArrayBlockingQueue(1)` handoff + busy flag; `thread.join()` gives the final happens-before for `snapDur`/`snapLen`):

```java
package net.knego.hiperf.smrcollections.livemvcc;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.CowBook;
import net.knego.hiperf.smrcollections.CowRoot;
import net.knego.hiperf.smrcollections.CowSnapshotter;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/live_mvcc (Java): writer latency with concurrent CoW serialization. */
public final class Main {
    private static final String EXPERIMENT = "live_mvcc";

    private record CapMsg(CowRoot root, long t0) {}

    public static void main(String[] args) throws InterruptedException {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            for (int i = 0; i < cfg.steady(); i++) {
                Workload.nextInsert(rng, i, cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
            }
            int n = cfg.steady();
            Workload.Update up = new Workload.Update();
            for (int i = 0; i < cfg.warmup(); i++) {
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
            }

            int maxBytes = 64 + cfg.cap() * 64 + cfg.levels() * 2 * 32;
            ArrayBlockingQueue<CapMsg> q = new ArrayBlockingQueue<>(1);
            AtomicBoolean busy = new AtomicBoolean(false);
            List<Long> snapDur = new ArrayList<>();
            long[] snapLenBox = new long[1];
            Thread ser = new Thread(() -> {
                CowSnapshotter s = new CowSnapshotter(maxBytes);
                while (true) {
                    CapMsg m;
                    try {
                        m = q.take();
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        return;
                    }
                    if (m.root() == null) {
                        return; // poison pill
                    }
                    snapLenBox[0] = s.encodeRoot(m.root());
                    snapDur.add(System.nanoTime() - m.t0());
                    busy.set(false);
                }
            });
            ser.start();

            long[] writerNs = new long[cfg.liveIters()];
            long skipped = 0;
            for (int k = 0; k < cfg.liveIters(); k++) {
                long t0 = System.nanoTime();
                if (k % cfg.snapEvery() == 0) {
                    if (busy.get()) {
                        skipped++;
                    } else {
                        busy.set(true);
                        q.put(new CapMsg(book.capture(), t0));
                    }
                }
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
                writerNs[k] = System.nanoTime() - t0;
            }
            q.put(new CapMsg(null, 0));
            ser.join();
            long[] snapNs = snapDur.stream().mapToLong(Long::longValue).toArray();
            SmrCollections.emitLive(EXPERIMENT, writerNs, snapNs, skipped, snapLenBox[0]);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

Both `build.gradle.kts` files copy `smr-collections-insert/build.gradle.kts` with `mainClass` set to the new Mains. Add to `settings.gradle.kts`:

```kotlin
    "smr-collections-live_stw",
    "smr-collections-live_mvcc",
```

- [ ] **Step 4: Build and smoke-run**

Run:
```bash
cd java && ./gradlew build -q
SMRC_CAP=131072 SMRC_STEADY=60000 SMRC_WARMUP=1000 SMRC_LIVE_ITERS=50000 SMRC_SNAP_EVERY=10000 ./gradlew :smr-collections-live_stw:run -q
SMRC_CAP=131072 SMRC_STEADY=60000 SMRC_WARMUP=1000 SMRC_LIVE_ITERS=50000 SMRC_SNAP_EVERY=10000 ./gradlew :smr-collections-live_mvcc:run -q
```
Expected: 10 lines each; `live_stw` `writer_max` ≫ `writer_p99`; `snapshot_bytes` equal across the two.

- [ ] **Step 5: Commit**

```bash
git add -A java && git commit -m "feat(smr-collections): Java live_stw/live_mvcc snapshot-under-writes cells"
```

---

### Task I1: bench-infra matrix + docs

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml`
- Modify: `bench-infra/ansible/roles/run/tasks/local.yml`
- Modify: `CLAUDE.md`
- Modify: `README.md`

- [ ] **Step 1: Matrix rows** — in `group_vars/all.yml`, after the `smr-collections / snapshot` row:

```yaml
  # MVCC variants: mvcc_* = hand-rolled chunked-CoW book (all languages);
  # ultima_* = ultima_db competitor cell (Rust only); live_* = snapshot under
  # live writes per store variant (writer_max is the headline stall metric).
  - { focus_area: smr-collections,  experiment: mvcc_insert,     kind: local }
  - { focus_area: smr-collections,  experiment: mvcc_update,     kind: local }
  - { focus_area: smr-collections,  experiment: mvcc_snapshot,   kind: local }
  - { focus_area: smr-collections,  experiment: ultima_insert,   kind: local, languages: [rust] }
  - { focus_area: smr-collections,  experiment: ultima_update,   kind: local, languages: [rust] }
  - { focus_area: smr-collections,  experiment: ultima_snapshot, kind: local, languages: [rust] }
  - { focus_area: smr-collections,  experiment: live_stw,        kind: local }
  - { focus_area: smr-collections,  experiment: live_mvcc,       kind: local }
  - { focus_area: smr-collections,  experiment: live_ultima,     kind: local, languages: [rust] }
```

In the smr-collections params block add:

```yaml
smrc_chunk: 4096
smrc_live_iters: 200000
smrc_snap_every: 20000
```

- [ ] **Step 2: Env exports** — in `roles/run/tasks/local.yml`, after `export SMRC_ITERS=...`:

```
    export SMRC_CHUNK="{{ smrc_chunk }}"
    export SMRC_LIVE_ITERS="{{ smrc_live_iters }}"
    export SMRC_SNAP_EVERY="{{ smrc_snap_every }}"
```

- [ ] **Step 3: Docs.**
  - `CLAUDE.md` "Build & run" artifact list: extend the smr-collections entry to `smr-collections-{insert,update,snapshot,mvcc_insert,mvcc_update,mvcc_snapshot,live_stw,live_mvcc}` (all languages) `and smr-collections-{ultima_insert,ultima_update,ultima_snapshot,live_ultima} (Rust)`.
  - `CLAUDE.md` "What this is" smr-collections status: after the existing sentence about insert/update/snapshot, add: "A chunked copy-on-write variant (`mvcc_*`, all three languages) snapshots via an O(#chunks) root capture at an op boundary without stopping the writer; `ultima_*` (Rust) runs the same workload through ultima_db (MVCC persistent B-tree, SMR pattern: SingleWriter + explicit versions, pinned git dep); `live_{stw,mvcc,ultima}` measure writer-observed latency while a snapshot is in flight (`writer_max` = stall). All variants emit byte-identical snapshot images, verified against the shared golden file."
  - **Docs fix (spec drive-by):** `CLAUDE.md` and `README.md` claim "Rust/Go use a hand-rolled open-addressing id-map" — correct to: Go hand-rolls open addressing; Rust uses std `HashMap` with an identity (`NoHash`) hasher.
- [ ] **Step 4: Validate + commit** — `cd bench-infra/ansible && ansible-playbook --syntax-check site.yml` (or the playbook name present there; skip if ansible isn't installed locally and note it in the commit message).

```bash
git add bench-infra/ansible/group_vars/all.yml bench-infra/ansible/roles/run/tasks/local.yml CLAUDE.md README.md
git commit -m "feat(smr-collections): bench matrix rows + params for mvcc/ultima/live cells; docs"
```

---

## Verification (whole-plan)

- [ ] Full gates: `cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check`; `cd go && go build ./... && go vet ./... && go test ./... && go test -race ./internal/smrcoll/`; `cd java && ./gradlew build`.
- [ ] Golden byte-identity holds in FIVE encoders (Rust STW — pre-existing, Rust CoW, Rust ultima, Go CoW, Java CoW) against the single unchanged `golden_snapshot.bin`.
- [ ] Local smoke of all 19 binaries produces contract-valid lines (fitness check only — **no journaling**; real numbers come from a user-initiated AWS `bench-infra` run; RESULTS.md gets its smr-collections section only after that run is journaled).

## Deliberate non-goals (from the spec)

No cancel/remove op (version-GC-under-churn unmeasured — documented caveat), no disk IO in the ultima cell (`Persistence` feature off), no incremental snapshots, no journal-CLI changes (`count` is an unknown unit → lower-is-better default, harmless for constant-per-config counters).
