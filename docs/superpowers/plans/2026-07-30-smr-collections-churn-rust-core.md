# smr-collections Churn — Rust Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a cancel op, a free list, and a ~1 % order-to-trade churn workload to the Rust flat (`Book`) and chunked-CoW (`CowBook`) stores, land SBE snapshot schema v2, and ship the four non-ultima Rust cells.

**Architecture:** A shared `Churn` driver in `smr-collections-common` generates a deterministic op stream (even index → insert, odd → cancel/fill) against a `ChurnStore` trait, so every store runs byte-identical ops. Slot recycling uses an intrusive LIFO free list threaded through each freed slot's `next` field, with `free_head` captured in the snapshot so restore reproduces allocation order exactly. Schema v2 adds only `freeHead` — freed slots stay in the pool, so the chain serialises itself. ultima keeps monotone slots (a B-tree has no pool) and is checked against the flat stores by a representation-free canonical digest rather than by bytes.

**Tech Stack:** Rust edition 2024, Cargo workspace, real-logic `sbe-tool` 1.38.1 (committed generated codec), crc32c.

**Spec:** [`docs/superpowers/specs/2026-07-30-smr-collections-cancel-churn-design.md`](../specs/2026-07-30-smr-collections-cancel-churn-design.md)

## Scope

This is **plan 1 of 3**. It covers everything Rust: config, `Book`, `CowBook`, `UltimaBook`, schema v2, the churn driver, the canonical digest, and all seven Rust cells. On completion `cargo test` is green, the goldens are regenerated, and `churn` / `mvcc_churn` / `live_stw_churn` / `live_mvcc_churn` / `ultima_churn` / `ultima_batch_churn` / `live_ultima_churn` all run.

ultima folded back in once the design settled on monotone slots: cancel there is `orders.delete(id)` plus a level update, not the adapter redesign an earlier draft assumed.

Deliberately **not** in this plan:

- **Plan 2 — Go parity.** Four cells, `idMap` backward-shift delete (Go's hand-rolled open addressing has no remove today), snapshot v2, golden verification.
- **Plan 3 — Java parity + infra/docs.** Four cells, plus the ansible matrix rows and `CLAUDE.md` — last, because adding matrix rows before all languages exist breaks a fleet run.

## Global Constraints

- Rust **edition 2024**; workspace members inherit `[workspace.package]` via `field.workspace = true`.
- Workspace must stay **clippy- and rustfmt-clean**: `cargo clippy --all-targets`, `cargo fmt --check`.
- **stdout is result-contract lines only.** All logs, progress, and diagnostics go to stderr.
- Every emitted line carries `focus_area: "smr-collections"` and the cell's `experiment` — always via `bench_common::smrcoll`, never hand-rolled JSON.
- Order IDs start at **1**; `order_id == 0` is the freed-slot marker. `NIL == u32::MAX`.
- Fixed capacity **never grows** — no rehash, no realloc. Exhaustion fails loudly.
- Determinism is the top requirement: same op stream ⇒ identical bytes, on any host, across restore.
- **Do not run `terraform apply` or any AWS benchmark.** Real runs are user-initiated. Local runs in this plan are fitness checks only and are never journaled.

## File Structure

**Modified:**
- `rust/bench-common/src/smrcoll.rs` — `otr_bps` config field, `require_bump_capacity()`, `rss_bytes()`, `emit_churn()`
- `rust/smr-collections/schema/book_snapshot.xml` — schema v2
- `rust/smr-collections/booksnap-sbe/generated/booksnap/**` — regenerated codec (do not hand-edit)
- `rust/smr-collections/common/src/book.rs` — free list, `cancel`, `fill`, best rescan
- `rust/smr-collections/common/src/snapshot.rs` — v2 encode/restore
- `rust/smr-collections/common/src/cowbook.rs` — same for the CoW store, `Root.free_head`
- `rust/smr-collections/common/src/cowsnap.rs` — v2 encode/restore from `Root`
- `rust/smr-collections/common/src/lib.rs` — `pub mod churn;`
- `rust/smr-collections/{insert,mvcc_insert}/src/main.rs` — call `require_bump_capacity()`
- `rust/testdata/golden_snapshot.bin` — regenerated

**Created:**
- `rust/smr-collections/common/src/churn.rs` — `ChurnStore`, `ChurnOp`, `Churn`, `run_churn`
- `rust/smr-collections/{churn,mvcc_churn,live_stw_churn,live_mvcc_churn}/` — four new crates
- `rust/testdata/golden_churn_snapshot.bin` — churn-workload golden

---

### Task 1: Config — `SMRC_OTR_BPS`, capacity-check refactor, RSS helper

**Files:**
- Modify: `rust/bench-common/src/smrcoll.rs`
- Modify: `rust/smr-collections/common/src/{book.rs,snapshot.rs,cowbook.rs,cowsnap.rs}` (test-fixture literals)
- Modify: `rust/smr-collections/ultima-common/src/lib.rs` (test-fixture literal)
- Modify: `rust/smr-collections/{insert,mvcc_insert,ultima_insert,ultima_batch_insert}/src/main.rs`

**Interfaces:**
- Produces: `SmrConfig.otr_bps: u64`; `SmrConfig::require_bump_capacity(&self) -> Result<(), String>`; `bench_common::smrcoll::rss_bytes() -> u64`

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `rust/bench-common/src/smrcoll.rs`. These follow the existing pattern in that file — env vars are process-global, so each test clears what it sets.

```rust
#[test]
fn smrc_otr_bps_defaults_to_100() {
    unsafe { std::env::remove_var("SMRC_OTR_BPS") };
    let c = SmrConfig::from_env().expect("defaults parse");
    assert_eq!(c.otr_bps, 100, "default OTR is 1% = 100 bps");
}

#[test]
fn smrc_otr_bps_rejects_over_10000() {
    unsafe { std::env::set_var("SMRC_OTR_BPS", "10001") };
    let r = SmrConfig::from_env();
    unsafe { std::env::remove_var("SMRC_OTR_BPS") };
    assert!(r.is_err(), "OTR above 100% must be rejected");
}

#[test]
fn churn_sized_run_parses_but_fails_bump_capacity() {
    // warmup + iters > cap is legal for a slot-recycling churn cell and
    // illegal for a bump-allocating insert cell.
    unsafe {
        std::env::set_var("SMRC_CAP", "1024");
        std::env::set_var("SMRC_STEADY", "512");
        std::env::set_var("SMRC_WARMUP", "1000");
        std::env::set_var("SMRC_ITERS", "10000");
    }
    let c = SmrConfig::from_env().expect("churn-sized config must parse");
    let bump = c.require_bump_capacity();
    unsafe {
        for k in ["SMRC_CAP", "SMRC_STEADY", "SMRC_WARMUP", "SMRC_ITERS"] {
            std::env::remove_var(k);
        }
    }
    assert!(bump.is_err(), "bump-allocating cells must reject it");
}

#[test]
fn rss_bytes_is_nonzero_on_linux() {
    assert!(rss_bytes() > 0, "RSS must be readable from /proc/self/statm");
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd rust && cargo test -p bench-common smrc_otr_bps
```

Expected: FAIL — `no field otr_bps on type SmrConfig`, `no method require_bump_capacity`, `cannot find function rss_bytes`.

- [ ] **Step 3: Implement**

In `rust/bench-common/src/smrcoll.rs`, add the field to `pub struct SmrConfig`:

```rust
    /// Order-to-trade ratio in basis points: the share of departures that are
    /// fills rather than cancels. 100 = 1 %, the real-exchange figure.
    pub otr_bps: u64,
```

In `from_env`, parse it next to the other knobs:

```rust
        let otr_bps = parse_usize("SMRC_OTR_BPS", 100)? as u64;
        if otr_bps > 10_000 {
            return Err(format!(
                "SMRC_OTR_BPS must be in 0..=10000 (got {otr_bps})"
            ));
        }
```

**Delete** this block from `from_env` (it is a bump-allocator constraint, not a universal one):

```rust
        if warmup + iters > cap {
            return Err("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP".into());
        }
```

Add `otr_bps,` to the `Ok(SmrConfig { … })` literal, then add the method inside `impl SmrConfig`:

```rust
    /// Cells that bump-allocate (no free list) need a pool slot for every op
    /// they will ever run. Churn cells recycle slots and must NOT call this.
    pub fn require_bump_capacity(&self) -> Result<(), String> {
        if self.warmup + self.iters > self.cap {
            return Err("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP".into());
        }
        Ok(())
    }
```

Add the RSS helper at module level:

```rust
/// Resident set size in bytes, from Linux `/proc/self/statm` field 2
/// (resident pages). Returns 0 where unreadable — the bench hosts are
/// x86-64 Linux with 4 KiB pages, which is the only case that must be right.
pub fn rss_bytes() -> u64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    match s.split_whitespace().nth(1).and_then(|f| f.parse::<u64>().ok()) {
        Some(pages) => pages * 4096,
        None => 0,
    }
}
```

- [ ] **Step 4: Fix the six `SmrConfig` test-fixture literals**

Adding a field breaks every struct literal. Add `otr_bps: 100,` to each:

- `rust/smr-collections/common/src/book.rs:235`
- `rust/smr-collections/common/src/snapshot.rs:165` and `:233`
- `rust/smr-collections/common/src/cowbook.rs:267`
- `rust/smr-collections/common/src/cowsnap.rs:154`
- `rust/smr-collections/ultima-common/src/lib.rs:636`

- [ ] **Step 5: Make the bump-allocating cells call the check**

In each of `rust/smr-collections/{insert,mvcc_insert,ultima_insert,ultima_batch_insert}/src/main.rs`, immediately after the `SmrConfig::from_env()` match block, insert:

```rust
    if let Err(m) = cfg.require_bump_capacity() {
        eprintln!("smr-collections-{EXPERIMENT}: {m}");
        std::process::exit(1);
    }
```

Leave `update`, `snapshot`, and the `live_*` cells alone — they pre-build `steady` orders and then only mutate, so the universal `steady <= cap` check already covers them.

- [ ] **Step 6: Run tests to verify they pass**

```sh
cd rust && cargo test -p bench-common && cargo test -p smr-collections-common
```

Expected: PASS.

- [ ] **Step 7: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/bench-common/src/smrcoll.rs rust/smr-collections
git commit -m "feat(smrcoll): SMRC_OTR_BPS + require_bump_capacity() + rss_bytes()

Moves warmup+iters<=cap out of from_env into an explicit check the
bump-allocating cells call, so slot-recycling churn cells can run longer
than SMRC_CAP."
```

---

### Task 2: `Book` — free list, `cancel`, `fill`, best-price rescan

**Files:**
- Modify: `rust/smr-collections/common/src/book.rs`

**Interfaces:**
- Consumes: `SmrConfig` from Task 1
- Produces: `Book.free_head: u32`; `Book::cancel(&mut self, order_id: i64)`; `Book::fill(&mut self, order_id: i64)`

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `rust/smr-collections/common/src/book.rs`:

```rust
    #[test]
    fn cancel_unlinks_middle_of_level_fifo() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0);
        b.insert(2, 5, 7, 0);
        b.insert(3, 5, 3, 0);
        b.cancel(2);
        assert_eq!(b.level_qty(0, 5), 13, "middle order's qty leaves the level");
        let lvl = &b.bids[5];
        assert_eq!(lvl.count, 2);
        assert_eq!(lvl.head, 0, "head unchanged");
        assert_eq!(lvl.tail, 2, "tail unchanged");
        assert_eq!(b.pool[0].next, 2, "head now links past the cancelled slot");
        assert_eq!(b.pool[2].prev, 0);
    }

    #[test]
    fn cancel_head_and_tail_fix_level_ends() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0);
        b.insert(2, 5, 7, 0);
        b.cancel(1); // head
        assert_eq!(b.bids[5].head, 1, "head advances to the survivor");
        assert_eq!(b.pool[1].prev, NIL);
        b.cancel(2); // tail, level now empty
        assert_eq!(b.bids[5].head, NIL);
        assert_eq!(b.bids[5].tail, NIL);
        assert_eq!(b.bids[5].count, 0);
        assert_eq!(b.level_qty(0, 5), 0);
    }

    #[test]
    fn cancel_emptying_best_level_rescans() {
        let mut b = Book::new(&cfg());
        b.insert(1, 3, 10, 0);
        b.insert(2, 9, 10, 0); // best bid = 9
        b.insert(3, 4, 10, 1);
        b.insert(4, 2, 10, 1); // best ask = 2
        assert_eq!(b.best_bid(), 9);
        assert_eq!(b.best_ask(), 2);
        b.cancel(2);
        assert_eq!(b.best_bid(), 3, "best bid falls back to the next occupied");
        b.cancel(4);
        assert_eq!(b.best_ask(), 4, "best ask rises to the next occupied");
        b.cancel(1);
        assert_eq!(b.best_bid(), -1, "no bids left");
    }

    #[test]
    fn cancelled_slots_are_reused_lifo() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0); // slot 0
        b.insert(2, 5, 10, 0); // slot 1
        b.insert(3, 5, 10, 0); // slot 2
        b.cancel(1); // free: 0
        b.cancel(3); // free: 2 -> 0
        assert_eq!(b.free_head, 2);
        b.insert(4, 5, 10, 0);
        assert_eq!(b.get_slot(4), 2, "LIFO: most recently freed slot first");
        b.insert(5, 5, 10, 0);
        assert_eq!(b.get_slot(5), 0);
        b.insert(6, 5, 10, 0);
        assert_eq!(b.get_slot(6), 3, "free list empty -> bump hwm");
        assert_eq!(b.hwm(), 4);
    }

    #[test]
    fn freed_slot_is_marked_with_zero_order_id() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0);
        b.cancel(1);
        assert_eq!(b.pool[0].order_id, 0, "freed marker for the snapshot walk");
    }

    #[test]
    fn fill_completes_then_frees_the_slot() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0);
        b.update(1, 4); // partial: remaining 6
        assert_eq!(b.level_qty(0, 5), 6);
        b.fill(1);
        assert_eq!(b.level_qty(0, 5), 0, "remaining 6 leaves the level");
        assert_eq!(b.bids[5].count, 0);
        assert_eq!(b.free_head, 0, "slot recycled like a cancel");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd rust && cargo test -p smr-collections-common cancel
```

Expected: FAIL — `no method named cancel found for struct Book`.

- [ ] **Step 3: Implement**

In `rust/smr-collections/common/src/book.rs`, add to `pub struct Book`:

```rust
    /// Head of the intrusive LIFO free list (`NIL` when empty). Freed slots
    /// chain through their own `next` field. This is state a snapshot must
    /// capture — restore reproduces allocation order from it.
    pub free_head: u32,
```

Initialise `free_head: NIL,` in `Book::new`.

Replace the first two lines of `insert`'s body (`let slot = self.hwm; self.hwm += 1;`) with:

```rust
        let slot = self.alloc_slot();
```

Add these private helpers to `impl Book`:

```rust
    #[inline]
    fn alloc_slot(&mut self) -> u32 {
        if self.free_head != NIL {
            let slot = self.free_head;
            self.free_head = self.pool[slot as usize].next;
            slot
        } else {
            if self.hwm as usize == self.pool.len() {
                panic!("order pool exhausted: SMRC_CAP={} reached", self.pool.len());
            }
            let slot = self.hwm;
            self.hwm += 1;
            slot
        }
    }

    #[inline]
    fn free_slot(&mut self, slot: u32) {
        let head = self.free_head;
        let o = &mut self.pool[slot as usize];
        o.order_id = 0; // freed marker: the snapshot walk skips these
        o.next = head;
        o.prev = NIL;
        self.free_head = slot;
    }

    /// Unlink `slot` from its level's intrusive FIFO and debit `rem` from the
    /// level's remaining quantity.
    fn unlink(&mut self, slot: u32, side: u8, t: u32, rem: i64) {
        let (prev, next) = {
            let o = &self.pool[slot as usize];
            (o.prev, o.next)
        };
        if prev != NIL {
            self.pool[prev as usize].next = next;
        }
        if next != NIL {
            self.pool[next as usize].prev = prev;
        }
        let lvl = &mut self.lane(side)[t as usize];
        if lvl.head == slot {
            lvl.head = next;
        }
        if lvl.tail == slot {
            lvl.tail = prev;
        }
        lvl.qty_total -= rem;
        lvl.count -= 1;
    }

    /// After a removal emptied level `t`, restore the cached best for `side`.
    /// O(levels) worst case and deliberately on the timed path — real books
    /// maintain this, and hiding it would hide the worst-case cancel.
    fn repair_best(&mut self, side: u8, t: u32) {
        if side == 0 {
            if self.best_bid != t as i32 || self.bids[t as usize].head != NIL {
                return;
            }
            let mut nb = -1i32;
            for i in (0..=t as usize).rev() {
                if self.bids[i].head != NIL {
                    nb = i as i32;
                    break;
                }
            }
            self.best_bid = nb;
        } else {
            if self.best_ask != t as i32 || self.asks[t as usize].head != NIL {
                return;
            }
            let mut na = -1i32;
            for i in t as usize..self.n_levels as usize {
                if self.asks[i].head != NIL {
                    na = i as i32;
                    break;
                }
            }
            self.best_ask = na;
        }
    }
```

Add the two public ops:

```rust
    /// Remove a resting order. Its remaining quantity leaves the level.
    pub fn cancel(&mut self, order_id: i64) {
        let slot = self
            .idmap
            .remove(&order_id)
            .expect("cancel: unknown order id");
        let (side, price, rem) = {
            let o = &self.pool[slot as usize];
            (o.side, o.price, o.qty - o.filled)
        };
        let t = self.tick_of(price);
        self.unlink(slot, side, t, rem);
        self.free_slot(slot);
        self.repair_best(side, t);
    }

    /// Fill an order to completion, then remove it. Same structural work as
    /// `cancel`; the difference is that the departing quantity is booked as
    /// filled rather than withdrawn.
    pub fn fill(&mut self, order_id: i64) {
        let slot = self.idmap.remove(&order_id).expect("fill: unknown order id");
        let (side, price, rem) = {
            let o = &mut self.pool[slot as usize];
            let rem = o.qty - o.filled;
            o.filled = o.qty;
            (o.side, o.price, rem)
        };
        let t = self.tick_of(price);
        self.unlink(slot, side, t, rem);
        self.free_slot(slot);
        self.repair_best(side, t);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

```sh
cd rust && cargo test -p smr-collections-common
```

Expected: PASS, including the three pre-existing `Book` tests.

- [ ] **Step 5: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/smr-collections/common/src/book.rs
git commit -m "feat(smrcoll): Book cancel/fill with intrusive LIFO free list

Slots recycle through free_head; freed slots are marked order_id=0 and chain
via their own next field. Emptying the best level triggers a ladder rescan,
deliberately on the timed path."
```

---

### Task 3: Schema v2 — `freeHead`

**Files:**
- Modify: `rust/smr-collections/schema/book_snapshot.xml`
- Modify: `rust/smr-collections/booksnap-sbe/generated/booksnap/**` (via `regen.sh`, never by hand)
- Modify: `rust/smr-collections/common/src/snapshot.rs`
- Modify: `rust/testdata/golden_snapshot.bin`

**Interfaces:**
- Consumes: `Book.free_head` from Task 2
- Produces: v2 `encode`/`restore` with the same signatures as today —
  `encode(book: &Book, buf: &mut [u8]) -> usize`, `restore(bytes: &[u8], cfg: &SmrConfig) -> Result<Book, String>`

- [ ] **Step 1: Edit the schema**

In `rust/smr-collections/schema/book_snapshot.xml`, change `version="1"` to `version="2"` on the `<sbe:messageSchema>` element, and add one field after `bestAsk`:

```xml
    <field name="freeHead"  id="8" type="uint32"/>
```

That is the entire schema change. The `orders` group keeps serialising every slot `0..hwm`, freed slots included and marked `order_id == 0` — which is why `freeHead` alone is sufficient: the free chain already rides in the image threaded through those slots' own `nextSlot` fields.

- [ ] **Step 2: Regenerate the codec and check the accessor name**

```sh
cd rust && sh smr-collections/booksnap-sbe/regen.sh
grep -n "fn free_head\|fn nl_evels" \
  smr-collections/booksnap-sbe/generated/booksnap/src/*.rs | head

**Verify the generated names before writing code against them.** The tool's
snake_case split is not always the obvious one — `nLevels` generates as
`nl_evels`, not `n_levels` (there is a comment about this at
`common/src/snapshot.rs:42`). `freeHead` has a multi-letter first word so it
should generate as `free_head`, but confirm from the grep output and use
whatever it actually emitted in the steps below.

- [ ] **Step 3: Write the failing tests**

Append to `mod tests` in `rust/smr-collections/common/src/snapshot.rs`:

```rust
    fn build_with_cancels(c: &SmrConfig, n: usize, cancel_every: usize) -> Book {
        let mut b = Book::new(c);
        let mut rng = SplitMix::new(SEED);
        for i in 0..n {
            let ins = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
            b.insert(ins.order_id, ins.price, ins.qty, ins.side);
            if i % cancel_every == cancel_every - 1 {
                b.cancel(ins.order_id);
            }
        }
        b
    }

    #[test]
    fn round_trip_preserves_free_list_order() {
        let c = cfg();
        let b = build_with_cancels(&c, c.steady, 4);
        assert_ne!(b.free_head, crate::book::NIL, "test needs a non-empty list");
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&b, &mut buf);
        let r = restore(&buf[..n], &c).expect("restore");
        assert_eq!(r.free_head, b.free_head, "free list head survives");
        // Walk both chains and compare slot-for-slot.
        let walk = |bk: &Book| {
            let mut v = Vec::new();
            let mut s = bk.free_head;
            while s != crate::book::NIL {
                v.push(s);
                s = bk.pool[s as usize].next;
            }
            v
        };
        assert_eq!(walk(&r), walk(&b), "free list order survives exactly");
    }

    #[test]
    fn restore_after_cancels_reencodes_identically() {
        let c = cfg();
        let b = build_with_cancels(&c, c.steady, 4);
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let n1 = encode(&b, &mut buf1);
        let r = restore(&buf1[..n1], &c).expect("restore");
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n2 = encode(&r, &mut buf2);
        assert_eq!(&buf1[..n1], &buf2[..n2]);
    }

    #[test]
    fn freed_slots_round_trip_without_polluting_the_id_map() {
        let c = cfg();
        let b = build_with_cancels(&c, c.steady, 4);
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&b, &mut buf);
        let r = restore(&buf[..n], &c).expect("restore");
        for slot in 0..b.hwm {
            let id = b.pool[slot as usize].order_id;
            if id != 0 {
                assert_eq!(r.get_slot(id), slot, "live order {id} keeps its slot");
            } else {
                assert_eq!(
                    r.pool[slot as usize].order_id, 0,
                    "slot {slot} stays marked free"
                );
            }
        }
        // order_id 0 is the freed marker, never a real key.
        assert!(!r.idmap.contains_key(&0), "freed slots must not enter the id-map");
    }

    #[test]
    fn wrong_schema_version_is_rejected() {
        let c = cfg();
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&build(&c, c.steady), &mut buf);
        buf[6] = 1; // messageHeader.version is the 4th u16
        // recompute the crc so version, not corruption, is what fails
        let crc = crc32c::crc32c(&buf[..n - 4]);
        buf[n - 4..n].copy_from_slice(&crc.to_le_bytes());
        let e = restore(&buf[..n], &c).expect_err("v1 image must be rejected");
        assert!(e.contains("version"), "error names the version: {e}");
    }
```

- [ ] **Step 4: Run tests to verify they fail**

```sh
cd rust && cargo test -p smr-collections-common free_list
```

Expected: FAIL — `no field free_head` on the decoder, or an assert on free-list order.

- [ ] **Step 5: Implement the v2 encoder**

In `rust/smr-collections/common/src/snapshot.rs`, inside `encode`, add one scalar next to the other fixed-block writes (after `enc.best_ask(...)`):

```rust
        enc.free_head(book.free_head);
```

**The orders group is unchanged** — it still walks `0..book.hwm` and emits every slot, freed ones included. A freed slot carries `order_id == 0` and its `next` is the free-chain link, so the chain serialises itself. Nothing else in `encode` moves.

- [ ] **Step 6: Implement the v2 restore**

In `restore`, check the schema version before decoding the body. Read it off the header first, since `header` is moved into `.header(header, 0)`:

```rust
    let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(&bytes[..sbe_len]), 0);
    let schema_version = header.version();
    if schema_version != 2 {
        return Err(format!(
            "unsupported snapshot schema version {schema_version} (expected 2)"
        ));
    }
    let dec = BookSnapshotDecoder::default().header(header, 0);
```

Add the capacity compatibility check after the scalars are read:

```rust
    if dec.capacity() as usize != cfg.cap {
        return Err(format!(
            "snapshot capacity {} != SMRC_CAP {}",
            dec.capacity(),
            cfg.cap
        ));
    }
    book.free_head = dec.free_head();
```

In the orders loop, every slot is still written to the pool verbatim (that is what restores the free chain), but freed slots must not enter the id-map. Guard the one line:

```rust
        book.pool[slot] = o;
        if o.order_id != 0 {
            book.idmap.insert(o.order_id, slot as u32);
        }
```

No free-list reconstruction is needed — `free_head` plus the pool's own `next` links are the chain.

- [ ] **Step 7: Run tests to verify they pass**

```sh
cd rust && cargo test -p smr-collections-common
```

Expected: PASS. The pre-existing `snapshot_is_deterministic_and_restore_stable` and `round_trip_preserves_queries` must stay green — an insert-only book has an empty free list and a full-length orders group, so its image is the old one plus `freeHead` and an empty group header.

- [ ] **Step 8: Regenerate the golden and record the size change**

```sh
cd rust && SMRC_WRITE_GOLDEN=1 cargo test -p smr-collections-common export_golden_when_requested
ls -l testdata/golden_snapshot.bin
```

Note the new byte count in the commit message — Go and Java (plans 3 and 4) verify against this exact file, and the fleet's `snapshot_bytes` metric shifts by the same delta.

- [ ] **Step 9: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/smr-collections/schema rust/smr-collections/booksnap-sbe \
        rust/smr-collections/common/src/snapshot.rs rust/testdata/golden_snapshot.bin
git commit -m "feat(smrcoll)!: snapshot schema v2 — freeHead

Freed slots stay in the pool marked order_id=0, so the free chain already
rides in the orders group through their own nextSlot links; capturing
freeHead is all that is needed to reproduce allocation order on restore.

Restore rejects non-v2 images and mismatched capacity, and keeps freed slots
out of the id-map. Golden regenerated; snapshot_bytes +4 for every cell."
```

---

### Task 4: The churn driver

**Files:**
- Create: `rust/smr-collections/common/src/churn.rs`
- Modify: `rust/smr-collections/common/src/lib.rs`

**Interfaces:**
- Consumes: `Book::{insert,cancel,fill}` (Task 2), `encode`/`restore` (Task 3)
- Produces:
  - `trait ChurnStore { fn insert(&mut self, i64, i64, i64, u8); fn cancel(&mut self, i64); fn fill(&mut self, i64); }`
  - `enum ChurnOp { Insert { order_id: i64, price: i64, qty: i64, side: u8 }, Cancel(i64), Fill(i64) }` (derives `Clone, Copy, Debug, PartialEq, Eq`)
  - `struct Churn` with `Churn::new(&SmrConfig) -> Churn`, `next_op(&mut self) -> ChurnOp`, `prebuild<S: ChurnStore>(&mut self, &mut S, usize)`, `apply<S: ChurnStore>(store: &mut S, op: ChurnOp)` (associated fn, no `self`)
  - `struct ChurnSamples { insert_ns: Vec<u64>, cancel_ns: Vec<u64>, fill_ns: Vec<u64> }`
  - `run_churn<S: ChurnStore>(&SmrConfig, &mut S, &mut Churn) -> ChurnSamples`
  - `emit_churn(experiment: &str, &ChurnSamples, rss_growth: u64)`

- [ ] **Step 1: Write the failing tests**

Create `rust/smr-collections/common/src/churn.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::snapshot::{encode, restore};

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
            apply_batch: 64,
            multi_table: false,
            live_iters: 200_000,
            snap_every: 20_000,
            otr_bps: 100,
        }
    }

    #[test]
    fn op_stream_is_deterministic() {
        let c = cfg();
        let (mut a, mut b) = (Churn::new(&c), Churn::new(&c));
        for k in 0..10_000 {
            assert_eq!(a.next_op(), b.next_op(), "op {k} diverged");
        }
    }

    #[test]
    fn stream_alternates_and_honours_otr() {
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut store = Book::new(&c);
        ch.prebuild(&mut store, c.steady);
        let (mut ins, mut can, mut fil) = (0usize, 0usize, 0usize);
        for _ in 0..100_000 {
            match ch.next_op() {
                ChurnOp::Insert { .. } => ins += 1,
                ChurnOp::Cancel(_) => can += 1,
                ChurnOp::Fill(_) => fil += 1,
            }
        }
        assert_eq!(ins, 50_000, "half the ops are inserts");
        assert_eq!(can + fil, 50_000, "the other half depart");
        // 100 bps of 50k departures = 500 fills; allow generous sampling slack.
        assert!((300..800).contains(&fil), "fills out of band: {fil}");
    }

    #[test]
    fn live_set_stays_constant() {
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut store = Book::new(&c);
        ch.prebuild(&mut store, c.steady);
        for _ in 0..20_000 {
            let op = ch.next_op();
            Churn::apply(&mut store, op);
        }
        let live = (0..store.hwm())
            .filter(|&s| store.pool[s as usize].order_id != 0)
            .count();
        assert_eq!(live, c.steady, "alternating stream holds the live set flat");
    }

    #[test]
    fn snapshot_restore_replay_is_bit_identical() {
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut hot = Book::new(&c);
        ch.prebuild(&mut hot, c.steady);
        for _ in 0..5_000 {
            let op = ch.next_op();
            Churn::apply(&mut hot, op);
        }
        // Snapshot at N and restore a cold replica from it.
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&hot, &mut buf);
        let mut cold = restore(&buf[..n], &c).expect("restore");
        // Replay the SAME ops N+1..M into both.
        let ops: Vec<ChurnOp> = (0..5_000).map(|_| ch.next_op()).collect();
        for &op in &ops {
            Churn::apply(&mut hot, op);
            Churn::apply(&mut cold, op);
        }
        let mut hot_buf = vec![0u8; 4 * 1024 * 1024];
        let mut cold_buf = vec![0u8; 4 * 1024 * 1024];
        let hn = encode(&hot, &mut hot_buf);
        let cn = encode(&cold, &mut cold_buf);
        assert_eq!(
            &hot_buf[..hn],
            &cold_buf[..cn],
            "restored replica diverged from the never-restarted one"
        );
    }

    #[test]
    fn export_churn_golden_when_requested() {
        if std::env::var("SMRC_WRITE_GOLDEN").is_err() {
            return;
        }
        let c = cfg();
        let mut ch = Churn::new(&c);
        let mut b = Book::new(&c);
        ch.prebuild(&mut b, c.steady);
        for _ in 0..10_000 {
            let op = ch.next_op();
            Churn::apply(&mut b, op);
        }
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&b, &mut buf);
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../testdata/golden_churn_snapshot.bin"
            ),
            &buf[..n],
        )
        .expect("write churn golden");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd rust && cargo test -p smr-collections-common churn
```

Expected: FAIL to compile — `Churn` and friends do not exist.

- [ ] **Step 3: Implement the driver**

Prepend to `rust/smr-collections/common/src/churn.rs`:

```rust
//! The churn workload: a deterministic insert/cancel/fill stream at a
//! configurable order-to-trade ratio (default 1 %, the real-exchange figure).
//! Op *generation* is deliberately outside the timed region — the driver
//! produces an op, the caller times only the store's application of it, so
//! per-op numbers stay comparable with the insert/update cells.

use crate::book::workload::next_insert;
use crate::rng::{SEED, SplitMix};
use bench_common::smrcoll::{SmrConfig, emit_int, emit_latency};
use std::time::Instant;

/// The three ops a churn stream drives into a store.
pub trait ChurnStore {
    fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8);
    fn cancel(&mut self, order_id: i64);
    fn fill(&mut self, order_id: i64);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChurnOp {
    Insert {
        order_id: i64,
        price: i64,
        qty: i64,
        side: u8,
    },
    Cancel(i64),
    Fill(i64),
}

pub struct Churn {
    rng: SplitMix,
    /// Order IDs currently resting, dense so a victim is one uniform draw.
    live: Vec<i64>,
    /// Global op index: drives both the insert/depart alternation and the
    /// order ID, so IDs are sparse (1, 3, 5, …) but never reused.
    i: usize,
    otr_bps: u64,
    levels: u32,
    tick: i64,
    price_min: i64,
}

impl Churn {
    pub fn new(cfg: &SmrConfig) -> Churn {
        Churn {
            rng: SplitMix::new(SEED),
            live: Vec::with_capacity(cfg.cap),
            i: 0,
            otr_bps: cfg.otr_bps,
            levels: cfg.levels,
            tick: cfg.tick,
            price_min: cfg.price_min,
        }
    }

    fn insert_op(&mut self) -> ChurnOp {
        let ins = next_insert(&mut self.rng, self.i, self.levels, self.tick, self.price_min);
        self.i += 1;
        self.live.push(ins.order_id);
        ChurnOp::Insert {
            order_id: ins.order_id,
            price: ins.price,
            qty: ins.qty,
            side: ins.side,
        }
    }

    /// The next op. Even index inserts, odd index departs; the departure is a
    /// fill with probability `otr_bps / 10_000`, otherwise a cancel.
    pub fn next_op(&mut self) -> ChurnOp {
        if self.i % 2 == 0 || self.live.is_empty() {
            return self.insert_op();
        }
        self.i += 1;
        let v = (self.rng.next() % self.live.len() as u64) as usize;
        let id = self.live[v];
        let is_fill = self.rng.next() % 10_000 < self.otr_bps;
        self.live.swap_remove(v);
        if is_fill {
            ChurnOp::Fill(id)
        } else {
            ChurnOp::Cancel(id)
        }
    }

    /// Bring the store to its steady-state live set with inserts only.
    pub fn prebuild<S: ChurnStore>(&mut self, store: &mut S, steady: usize) {
        for _ in 0..steady {
            let op = self.insert_op();
            Churn::apply(store, op);
        }
    }

    pub fn apply<S: ChurnStore>(store: &mut S, op: ChurnOp) {
        match op {
            ChurnOp::Insert {
                order_id,
                price,
                qty,
                side,
            } => store.insert(order_id, price, qty, side),
            ChurnOp::Cancel(id) => store.cancel(id),
            ChurnOp::Fill(id) => store.fill(id),
        }
    }
}

impl ChurnStore for crate::book::Book {
    fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        crate::book::Book::insert(self, order_id, price, qty, side)
    }
    fn cancel(&mut self, order_id: i64) {
        crate::book::Book::cancel(self, order_id)
    }
    fn fill(&mut self, order_id: i64) {
        crate::book::Book::fill(self, order_id)
    }
}

#[derive(Default)]
pub struct ChurnSamples {
    pub insert_ns: Vec<u64>,
    pub cancel_ns: Vec<u64>,
    pub fill_ns: Vec<u64>,
}

/// Warm up, then time `cfg.iters` ops into per-op-type sample vectors.
/// Only the store call is inside the clock.
pub fn run_churn<S: ChurnStore>(cfg: &SmrConfig, store: &mut S, churn: &mut Churn) -> ChurnSamples {
    for _ in 0..cfg.warmup {
        let op = churn.next_op();
        Churn::apply(store, op);
    }
    let half = cfg.iters / 2 + 1;
    let mut s = ChurnSamples {
        insert_ns: Vec::with_capacity(half),
        cancel_ns: Vec::with_capacity(half),
        fill_ns: Vec::with_capacity(half),
    };
    for _ in 0..cfg.iters {
        let op = churn.next_op();
        let t0 = Instant::now();
        Churn::apply(store, op);
        let ns = t0.elapsed().as_nanos() as u64;
        match op {
            ChurnOp::Insert { .. } => s.insert_ns.push(ns),
            ChurnOp::Cancel(_) => s.cancel_ns.push(ns),
            ChurnOp::Fill(_) => s.fill_ns.push(ns),
        }
    }
    s
}

/// Emit the per-op-type distributions plus RSS growth. A distribution with no
/// samples is skipped rather than emitted as zeros — at `SMRC_OTR_BPS=0` there
/// are no fills, and a fabricated zero would read as a real measurement.
pub fn emit_churn(experiment: &str, s: &ChurnSamples, rss_growth: u64) {
    if !s.insert_ns.is_empty() {
        emit_latency(experiment, "insert", &s.insert_ns);
    }
    if !s.cancel_ns.is_empty() {
        emit_latency(experiment, "cancel", &s.cancel_ns);
    }
    if !s.fill_ns.is_empty() {
        emit_latency(experiment, "fill", &s.fill_ns);
    }
    emit_int(experiment, "rss_growth_bytes", rss_growth, "bytes", 1);
}
```

Register the module in `rust/smr-collections/common/src/lib.rs`:

```rust
pub mod churn;
```

- [ ] **Step 4: Run tests to verify they pass**

```sh
cd rust && cargo test -p smr-collections-common churn
```

Expected: PASS — in particular `snapshot_restore_replay_is_bit_identical`, which is the property the whole schema change exists to support.

- [ ] **Step 5: Write the churn golden**

```sh
cd rust && SMRC_WRITE_GOLDEN=1 cargo test -p smr-collections-common export_churn_golden_when_requested
ls -l testdata/golden_churn_snapshot.bin
```

- [ ] **Step 6: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/smr-collections/common/src/churn.rs rust/smr-collections/common/src/lib.rs \
        rust/testdata/golden_churn_snapshot.bin
git commit -m "feat(smrcoll): churn workload driver at ~1% OTR

Alternating insert/depart stream over a dense live[] with uniform victim
selection, so the live set stays exactly flat and slot reuse is scattered.
Includes the bit-identical resumption test: snapshot, restore, replay the
same ops into both replicas, re-snapshot, compare bytes."
```

---

### Task 5: `CowBook` — free list, cancel, `Root.free_head`, v2 CoW snapshot

**Files:**
- Modify: `rust/smr-collections/common/src/cowbook.rs`
- Modify: `rust/smr-collections/common/src/cowsnap.rs`

**Interfaces:**
- Consumes: `ChurnStore` (Task 4), v2 codec (Task 3)
- Produces: `CowBook.free_head`, `CowBook::{cancel,fill}`, `Root.free_head`, `impl ChurnStore for CowBook`

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `rust/smr-collections/common/src/cowbook.rs`:

```rust
    #[test]
    fn cowbook_cancel_matches_book_cancel() {
        let c = cfg();
        let mut b = Book::new(&c);
        let mut cb = CowBook::new(&c);
        let mut ch = crate::churn::Churn::new(&c);
        ch.prebuild(&mut b, 500);
        let mut ch2 = crate::churn::Churn::new(&c);
        ch2.prebuild(&mut cb, 500);
        for _ in 0..5_000 {
            let op = ch.next_op();
            let op2 = ch2.next_op();
            assert_eq!(op, op2, "the two drivers must stay in lockstep");
            crate::churn::Churn::apply(&mut b, op);
            crate::churn::Churn::apply(&mut cb, op2);
        }
        assert_eq!(cb.free_head, b.free_head, "free heads agree");
        assert_eq!(cb.hwm, b.hwm(), "hwm agrees");
        assert_eq!(cb.best_bid, b.best_bid(), "best bid agrees");
        assert_eq!(cb.best_ask, b.best_ask(), "best ask agrees");
        for t in 0..c.levels {
            assert_eq!(cb.level_qty(0, t), b.level_qty(0, t), "bid level {t}");
            assert_eq!(cb.level_qty(1, t), b.level_qty(1, t), "ask level {t}");
        }
    }

    #[test]
    fn capture_carries_free_head() {
        let c = cfg();
        let mut cb = CowBook::new(&c);
        cb.insert(1, 5, 10, 0);
        cb.insert(2, 5, 10, 0);
        cb.cancel(1);
        let root = cb.capture();
        assert_eq!(root.free_head, cb.free_head);
    }
```

And to `mod tests` in `rust/smr-collections/common/src/cowsnap.rs`:

```rust
    #[test]
    fn cow_churn_image_matches_flat_churn_image() {
        let c = golden_cfg();
        let mut b = crate::book::Book::new(&c);
        let mut cb = CowBook::new(&c);
        let mut cha = crate::churn::Churn::new(&c);
        let mut chb = crate::churn::Churn::new(&c);
        cha.prebuild(&mut b, c.steady);
        chb.prebuild(&mut cb, c.steady);
        for _ in 0..10_000 {
            let op = cha.next_op();
            let op2 = chb.next_op();
            crate::churn::Churn::apply(&mut b, op);
            crate::churn::Churn::apply(&mut cb, op2);
        }
        let mut flat = vec![0u8; 4 * 1024 * 1024];
        let mut cow = vec![0u8; 4 * 1024 * 1024];
        let fl = crate::snapshot::encode(&b, &mut flat);
        let root = cb.capture();
        let cl = encode_root(&root, &mut cow);
        assert_eq!(&flat[..fl], &cow[..cl], "CoW image == flat image");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd rust && cargo test -p smr-collections-common cow
```

Expected: FAIL — `no method named cancel found for struct CowBook`.

- [ ] **Step 3: Implement `CowBook` cancel/fill**

Add `pub free_head: u32,` to both `pub struct Root` and `pub struct CowBook`; initialise `free_head: NIL` in `CowBook::new`, and copy it in `capture`:

```rust
            free_head: self.free_head,
```

Replace `insert`'s `let slot = self.hwm; self.hwm += 1;` with `let slot = self.alloc_slot();` and add, inside `impl CowBook`:

```rust
    #[inline]
    fn alloc_slot(&mut self) -> u32 {
        if self.free_head != NIL {
            let slot = self.free_head;
            self.free_head = self.order(slot).next;
            slot
        } else {
            if self.hwm == self.capacity {
                panic!("order pool exhausted: SMRC_CAP={} reached", self.capacity);
            }
            let slot = self.hwm;
            self.hwm += 1;
            slot
        }
    }

    #[inline]
    fn free_slot(&mut self, slot: u32) {
        let head = self.free_head;
        let o = self.order_mut(slot);
        o.order_id = 0;
        o.next = head;
        o.prev = NIL;
        self.free_head = slot;
    }

    fn unlink(&mut self, slot: u32, side: u8, t: u32, rem: i64) {
        let (prev, next) = {
            let o = self.order(slot);
            (o.prev, o.next)
        };
        if prev != NIL {
            self.order_mut(prev).next = next;
        }
        if next != NIL {
            self.order_mut(next).prev = prev;
        }
        let lvl = self.level_mut(side, t);
        if lvl.head == slot {
            lvl.head = next;
        }
        if lvl.tail == slot {
            lvl.tail = prev;
        }
        lvl.qty_total -= rem;
        lvl.count -= 1;
    }

    /// Read-only ladder rescan — uses `level()`, not `level_mut()`, so it
    /// never triggers a copy-on-write of an untouched chunk.
    fn repair_best(&mut self, side: u8, t: u32) {
        if side == 0 {
            if self.best_bid != t as i32 || self.level(0, t).head != NIL {
                return;
            }
            let mut nb = -1i32;
            for i in (0..=t).rev() {
                if self.level(0, i).head != NIL {
                    nb = i as i32;
                    break;
                }
            }
            self.best_bid = nb;
        } else {
            if self.best_ask != t as i32 || self.level(1, t).head != NIL {
                return;
            }
            let mut na = -1i32;
            for i in t..self.n_levels {
                if self.level(1, i).head != NIL {
                    na = i as i32;
                    break;
                }
            }
            self.best_ask = na;
        }
    }

    /// Same op semantics as `Book::cancel` (keep in lockstep).
    pub fn cancel(&mut self, order_id: i64) {
        let slot = self
            .idmap
            .remove(&order_id)
            .expect("cancel: unknown order id");
        let (side, price, rem) = {
            let o = self.order(slot);
            (o.side, o.price, o.qty - o.filled)
        };
        let t = self.tick_of(price);
        self.unlink(slot, side, t, rem);
        self.free_slot(slot);
        self.repair_best(side, t);
    }

    /// Same op semantics as `Book::fill` (keep in lockstep).
    pub fn fill(&mut self, order_id: i64) {
        let slot = self.idmap.remove(&order_id).expect("fill: unknown order id");
        let (side, price, rem) = {
            let o = self.order_mut(slot);
            let rem = o.qty - o.filled;
            o.filled = o.qty;
            (o.side, o.price, rem)
        };
        let t = self.tick_of(price);
        self.unlink(slot, side, t, rem);
        self.free_slot(slot);
        self.repair_best(side, t);
    }
```

Add the trait impl at the bottom of the file (before `mod tests`):

```rust
impl crate::churn::ChurnStore for CowBook {
    fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        CowBook::insert(self, order_id, price, qty, side)
    }
    fn cancel(&mut self, order_id: i64) {
        CowBook::cancel(self, order_id)
    }
    fn fill(&mut self, order_id: i64) {
        CowBook::fill(self, order_id)
    }
}
```

- [ ] **Step 4: Mirror the v2 format in `cowsnap.rs`**

`encode_root` gets exactly the change `snapshot.rs` got in Task 3 — one added line, `enc.free_head(root.free_head)`, next to the other fixed-block writes. Its orders loop already walks `0..root.hwm` through `root.order(slot)` and stays as-is. `restore_cow` gains the same version check, capacity check, `free_head` read, and the `if o.order_id != 0` guard on the id-map insert, writing through `order_mut` as it already does.

- [ ] **Step 5: Run tests to verify they pass**

```sh
cd rust && cargo test -p smr-collections-common
```

Expected: PASS, including `cow_churn_image_matches_flat_churn_image`.

- [ ] **Step 6: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/smr-collections/common/src/cowbook.rs rust/smr-collections/common/src/cowsnap.rs
git commit -m "feat(smrcoll): CowBook cancel/fill + free_head in Root

Root carries free_head so a restored replica reproduces allocation order.
The ladder rescan reads through level(), never level_mut(), so it does not
trigger copy-on-write of untouched chunks."
```

---

### Task 6: The four Rust cells

**Files:**
- Create: `rust/smr-collections/churn/{Cargo.toml,src/main.rs}`
- Create: `rust/smr-collections/mvcc_churn/{Cargo.toml,src/main.rs}`
- Create: `rust/smr-collections/live_stw_churn/{Cargo.toml,src/main.rs}`
- Create: `rust/smr-collections/live_mvcc_churn/{Cargo.toml,src/main.rs}`
- Modify: `rust/Cargo.toml` (workspace `members`)

**Interfaces:**
- Consumes: everything from Tasks 1–5

- [ ] **Step 1: Add the four crates to the workspace**

In `rust/Cargo.toml`, add to `members`:

```toml
    "smr-collections/churn",
    "smr-collections/mvcc_churn",
    "smr-collections/live_stw_churn",
    "smr-collections/live_mvcc_churn",
```

Each crate's `Cargo.toml` copies the existing `smr-collections/insert/Cargo.toml` verbatim with the package name changed — for example:

```toml
[package]
name = "smr-collections-churn"
version.workspace = true
edition.workspace = true

[dependencies]
bench-common = { path = "../../bench-common" }
smr-collections-common = { path = "../common" }
```

- [ ] **Step 2: Write `churn/src/main.rs`**

```rust
//! smr-collections **churn** — insert/cancel/fill at a real-exchange
//! order-to-trade ratio against the flat stop-the-world book. Cancels recycle
//! slots through the free list, so this is the steady state a matching engine
//! actually lives in.

use bench_common::smrcoll::{SmrConfig, rss_bytes};
use smr_collections_common::book::Book;
use smr_collections_common::churn::{Churn, emit_churn, run_churn};

const EXPERIMENT: &str = "churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = Book::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    let rss0 = rss_bytes();
    let samples = run_churn(&cfg, &mut book, &mut churn);
    let rss1 = rss_bytes();
    emit_churn(EXPERIMENT, &samples, rss1.saturating_sub(rss0));
}
```

- [ ] **Step 3: Write `mvcc_churn/src/main.rs`**

Identical apart from the store and the experiment name:

```rust
//! smr-collections **mvcc_churn** — the churn workload against the chunked
//! copy-on-write book. Cancels scatter writes across chunks rather than
//! appending to the newest one, so this is where CoW's first-touch copy cost
//! is exercised hardest.

use bench_common::smrcoll::{SmrConfig, rss_bytes};
use smr_collections_common::churn::{Churn, emit_churn, run_churn};
use smr_collections_common::cowbook::CowBook;

const EXPERIMENT: &str = "mvcc_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = CowBook::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    let rss0 = rss_bytes();
    let samples = run_churn(&cfg, &mut book, &mut churn);
    let rss1 = rss_bytes();
    emit_churn(EXPERIMENT, &samples, rss1.saturating_sub(rss0));
}
```

- [ ] **Step 4: Write `live_stw_churn/src/main.rs`**

This mirrors the existing `live_stw` cell (`rust/smr-collections/live_stw/src/main.rs`) with the churn stream in place of the update-only one, and adds `rss_peak_bytes`.

```rust
//! smr-collections **live_stw_churn** — writer-observed latency under the
//! churn workload while stop-the-world snapshots run inline at a fixed
//! cadence. The op that triggers a snapshot pays the whole serialize
//! (writer_max is the stall); the per-op split shows which op absorbed it.

use bench_common::smrcoll::{SmrConfig, emit_int, emit_latency, emit_live, rss_bytes};
use smr_collections_common::book::Book;
use smr_collections_common::churn::{Churn, ChurnOp, ChurnSamples};
use smr_collections_common::snapshot::encode;
use std::time::Instant;

const EXPERIMENT: &str = "live_stw_churn";

fn main() {
    let cfg = match SmrConfig::from_env() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("smr-collections-{EXPERIMENT}: {m}");
            std::process::exit(1);
        }
    };
    let mut book = Book::new(&cfg);
    let mut churn = Churn::new(&cfg);
    churn.prebuild(&mut book, cfg.steady);
    for _ in 0..cfg.warmup {
        let op = churn.next_op();
        Churn::apply(&mut book, op);
    }
    let mut buf = vec![0u8; 64 + cfg.cap * 64 + (cfg.levels as usize) * 2 * 32];
    // warm the encode path + buffer pages so the k=0 trigger measures
    // steady-state stall, not first-touch cost
    encode(&book, &mut buf);

    let mut writer_ns = vec![0u64; cfg.live_iters];
    let mut snap_ns: Vec<u64> = Vec::with_capacity(cfg.live_iters / cfg.snap_every + 1);
    let mut snap_len = 0usize;
    let mut s = ChurnSamples::default();
    let mut rss_peak = rss_bytes();
    for k in 0..cfg.live_iters {
        let op = churn.next_op();
        let fired = k % cfg.snap_every == 0;
        let t0 = Instant::now();
        if fired {
            snap_len = encode(&book, &mut buf);
            snap_ns.push(t0.elapsed().as_nanos() as u64);
        }
        Churn::apply(&mut book, op);
        let ns = t0.elapsed().as_nanos() as u64;
        // Sample RSS only AFTER the clock closes. `rss_bytes()` reads
        // /proc/self/statm — microseconds against 50-300 ns ops — so calling it
        // inside the timed region would inflate `writer_max`, the one metric
        // this cell exists to report precisely.
        if fired {
            rss_peak = rss_peak.max(rss_bytes());
        }
        writer_ns[k] = ns;
        match op {
            ChurnOp::Insert { .. } => s.insert_ns.push(ns),
            ChurnOp::Cancel(_) => s.cancel_ns.push(ns),
            ChurnOp::Fill(_) => s.fill_ns.push(ns),
        }
    }
    emit_live(EXPERIMENT, &writer_ns, &snap_ns, 0, snap_len);
    if !s.insert_ns.is_empty() {
        emit_latency(EXPERIMENT, "insert", &s.insert_ns);
    }
    if !s.cancel_ns.is_empty() {
        emit_latency(EXPERIMENT, "cancel", &s.cancel_ns);
    }
    if !s.fill_ns.is_empty() {
        emit_latency(EXPERIMENT, "fill", &s.fill_ns);
    }
    emit_int(EXPERIMENT, "rss_peak_bytes", rss_peak, "bytes", 1);
}
```

- [ ] **Step 5: Write `live_mvcc_churn/src/main.rs`**

Same as Step 4 with three substitutions, mirroring how `live_mvcc` differs from `live_stw`: `CowBook` for `Book`, `EXPERIMENT = "live_mvcc_churn"`, and the snapshot taken as a capture-then-encode rather than an inline serialize —

```rust
        if k % cfg.snap_every == 0 {
            let root = book.capture();
            snap_ns.push(t0.elapsed().as_nanos() as u64);
            snap_len = encode_root(&root, &mut buf);
            rss_peak = rss_peak.max(rss_bytes());
        }
```

with `use smr_collections_common::cowsnap::encode_root;`. Read `rust/smr-collections/live_mvcc/src/main.rs` first and follow whatever it does for the serializer thread and the `skipped` counter — this cell must differ from it only in the workload.

- [ ] **Step 6: Build and smoke-run all four**

```sh
cd rust && cargo build --release && cargo clippy --all-targets -- -D warnings && cargo fmt --check
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_WARMUP=1000 SMRC_ITERS=20000 \
  cargo run --release -p smr-collections-churn
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_WARMUP=1000 SMRC_ITERS=20000 \
  cargo run --release -p smr-collections-mvcc_churn
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_LIVE_ITERS=20000 SMRC_SNAP_EVERY=5000 \
  cargo run --release -p smr-collections-live_stw_churn
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_LIVE_ITERS=20000 SMRC_SNAP_EVERY=5000 \
  cargo run --release -p smr-collections-live_mvcc_churn
```

Expected: each prints result-contract JSON lines on stdout carrying
`"focus_area":"smr-collections"`, the right `experiment`, and metrics
`insert_*`, `cancel_*`, `fill_*`, `rss_growth_bytes` (plus `writer_*`,
`snapshot_*`, `rss_peak_bytes` for the live cells). Nothing but result lines on
stdout.

**This is a local fitness check, not a result.** Do not journal it.

- [ ] **Step 7: Commit**

```sh
git add rust/Cargo.toml rust/smr-collections/churn rust/smr-collections/mvcc_churn \
        rust/smr-collections/live_stw_churn rust/smr-collections/live_mvcc_churn
git commit -m "feat(smrcoll): rust churn cells — churn, mvcc_churn, live_{stw,mvcc}_churn

Four cells emitting per-op-type distributions (insert/cancel/fill) plus
rss_growth_bytes; the live pair adds writer_max and rss_peak_bytes while a
snapshot is in flight."
```

---

### Task 7: The canonical digest

**Files:**
- Create: `rust/smr-collections/common/src/digest.rs`
- Modify: `rust/smr-collections/common/src/lib.rs`

**Interfaces:**
- Produces: `digest_book(&Book) -> Vec<u8>`, `digest_root(&Root) -> Vec<u8>`

The digest is the representation-free view of a book: no slot handles, levels expressed as order-ID FIFOs. It is how ultima — which never recycles slots — is checked against the flat stores, whose slot numbering it has no reason to reproduce.

- [ ] **Step 1: Write the failing test**

Create `rust/smr-collections/common/src/digest.rs` with its test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::churn::Churn;
    use crate::cowbook::CowBook;
    use bench_common::smrcoll::SmrConfig;

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
            apply_batch: 64,
            multi_table: false,
            live_iters: 200_000,
            snap_every: 20_000,
            otr_bps: 100,
        }
    }

    #[test]
    fn flat_and_cow_agree_on_the_digest() {
        let c = cfg();
        let (mut b, mut cb) = (Book::new(&c), CowBook::new(&c));
        let (mut cha, mut chb) = (Churn::new(&c), Churn::new(&c));
        cha.prebuild(&mut b, c.steady);
        chb.prebuild(&mut cb, c.steady);
        for _ in 0..10_000 {
            let (op, op2) = (cha.next_op(), chb.next_op());
            Churn::apply(&mut b, op);
            Churn::apply(&mut cb, op2);
        }
        let root = cb.capture();
        assert_eq!(digest_book(&b), digest_root(&root));
    }

    #[test]
    fn digest_ignores_slot_numbering() {
        // Two books with the same logical content but different allocation
        // history must digest identically.
        let c = cfg();
        let mut a = Book::new(&c);
        a.insert(1, 5, 10, 0);
        a.insert(3, 5, 20, 0);

        let mut d = Book::new(&c);
        d.insert(9, 5, 99, 0); // burns slot 0
        d.cancel(9);
        d.insert(1, 5, 10, 0); // reuses slot 0
        d.insert(3, 5, 20, 0);

        assert_ne!(d.free_head, a.free_head, "histories really do differ");
        assert_eq!(digest_book(&a), digest_book(&d));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```sh
cd rust && cargo test -p smr-collections-common digest
```

Expected: FAIL to compile — `digest_book` not found.

- [ ] **Step 3: Implement**

Prepend to `rust/smr-collections/common/src/digest.rs`:

```rust
//! Representation-free canonical digest of a book: levels as order-ID FIFOs,
//! orders sorted by order ID, no slot handles anywhere. Stores that allocate
//! slots differently (a recycling pool vs a monotone key space) still agree
//! on this, which is what "the same book" actually means.

use crate::book::{Book, NIL};
use crate::cowbook::Root;

fn put_level(out: &mut Vec<u8>, side: u8, tick: u32, qty_total: i64, count: u32) {
    out.push(side);
    out.extend_from_slice(&tick.to_le_bytes());
    out.extend_from_slice(&qty_total.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
}

fn put_order(out: &mut Vec<u8>, order_id: i64, price: i64, qty: i64, filled: i64, side: u8) {
    out.extend_from_slice(&order_id.to_le_bytes());
    out.extend_from_slice(&price.to_le_bytes());
    out.extend_from_slice(&qty.to_le_bytes());
    out.extend_from_slice(&filled.to_le_bytes());
    out.push(side);
}

pub fn digest_book(b: &Book) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 << 16);
    let mut live: Vec<(i64, i64, i64, i64, u8)> = Vec::new();
    for slot in 0..b.hwm as usize {
        let o = &b.pool[slot];
        if o.order_id != 0 {
            live.push((o.order_id, o.price, o.qty, o.filled, o.side));
        }
    }
    out.extend_from_slice(&b.best_bid.to_le_bytes());
    out.extend_from_slice(&b.best_ask.to_le_bytes());
    out.extend_from_slice(&(live.len() as u32).to_le_bytes());
    for (side, lane) in [(0u8, &b.bids), (1u8, &b.asks)] {
        for (t, lvl) in lane.iter().enumerate() {
            if lvl.head == NIL {
                continue;
            }
            put_level(&mut out, side, t as u32, lvl.qty_total, lvl.count);
            let mut s = lvl.head;
            while s != NIL {
                out.extend_from_slice(&b.pool[s as usize].order_id.to_le_bytes());
                s = b.pool[s as usize].next;
            }
        }
    }
    live.sort_unstable();
    for (id, price, qty, filled, side) in live {
        put_order(&mut out, id, price, qty, filled, side);
    }
    out
}

pub fn digest_root(r: &Root) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 << 16);
    let mut live: Vec<(i64, i64, i64, i64, u8)> = Vec::new();
    for slot in 0..r.hwm {
        let o = r.order(slot);
        if o.order_id != 0 {
            live.push((o.order_id, o.price, o.qty, o.filled, o.side));
        }
    }
    out.extend_from_slice(&r.best_bid.to_le_bytes());
    out.extend_from_slice(&r.best_ask.to_le_bytes());
    out.extend_from_slice(&(live.len() as u32).to_le_bytes());
    for side in [0u8, 1u8] {
        for t in 0..r.n_levels {
            let lvl = r.level(side, t);
            if lvl.head == NIL {
                continue;
            }
            put_level(&mut out, side, t, lvl.qty_total, lvl.count);
            let mut s = lvl.head;
            while s != NIL {
                out.extend_from_slice(&r.order(s).order_id.to_le_bytes());
                s = r.order(s).next;
            }
        }
    }
    live.sort_unstable();
    for (id, price, qty, filled, side) in live {
        put_order(&mut out, id, price, qty, filled, side);
    }
    out
}
```

Add `pub mod digest;` to `rust/smr-collections/common/src/lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

```sh
cd rust && cargo test -p smr-collections-common digest
```

Expected: PASS.

- [ ] **Step 5: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/smr-collections/common/src/digest.rs rust/smr-collections/common/src/lib.rs
git commit -m "feat(smrcoll): canonical representation-free book digest

Levels as order-ID FIFOs, orders sorted by ID, no slot handles. Lets stores
with different allocation policies be checked for logical equality without
forcing one to emulate the other's slot numbering."
```

---

### Task 8: `UltimaBook` — cancel, fill, sparse encode

**Files:**
- Modify: `rust/smr-collections/ultima-common/src/lib.rs`

**Interfaces:**
- Consumes: `ChurnStore` (Task 4), `digest_book` (Task 7)
- Produces: `UltimaBook::{cancel,fill}`, `cancel_batch_txn{,_multi}`, `digest_ultima(&Store, version) -> Vec<u8>`, `impl ChurnStore for UltimaBook`

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `rust/smr-collections/ultima-common/src/lib.rs`:

```rust
    #[test]
    fn ultima_churn_matches_flat_digest() {
        let c = cfg();
        let mut flat = smr_collections_common::book::Book::new(&c);
        let mut ult = UltimaBook::new(&c);
        let mut cha = smr_collections_common::churn::Churn::new(&c);
        let mut chb = smr_collections_common::churn::Churn::new(&c);
        cha.prebuild(&mut flat, c.steady);
        chb.prebuild(&mut ult, c.steady);
        for _ in 0..5_000 {
            let (op, op2) = (cha.next_op(), chb.next_op());
            smr_collections_common::churn::Churn::apply(&mut flat, op);
            smr_collections_common::churn::Churn::apply(&mut ult, op2);
        }
        let v = ult.current_version();
        assert_eq!(
            digest_ultima(&ult.store, v),
            smr_collections_common::digest::digest_book(&flat),
            "ultima diverged from the flat book"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

```sh
cd rust && cargo test -p smr-collections-ultima-common ultima_churn
```

Expected: FAIL — `no method named cancel found for struct UltimaBook`.

- [ ] **Step 3: Switch insert to an explicit id**

In `apply_insert` (and its `_mt` twin), the churn stream's sparse order IDs make the auto-increment id wrong. Replace the `orders.insert(...)` call and its assert with:

```rust
            orders
                .insert_with_id(order_id as u64, OrderRec { /* fields unchanged */ })
                .expect("order insert");
```

Keep `let slot = (order_id - 1) as u32;` — with no recycling it stays a valid monotone handle. Delete the `assert_eq!(id, order_id as u64, …)` line, which no longer has a meaning.

- [ ] **Step 4: Implement cancel and fill**

Add to `impl UltimaBook`, mirroring `apply_update`'s structure:

```rust
    /// Remove a resting order. A cancel and a full fill are the same operation
    /// here: the row is deleted either way, and the level loses the same
    /// remaining quantity. The flat store distinguishes them only in the
    /// `filled` field it leaves behind in a freed pool slot, which ultima has
    /// no equivalent of.
    fn apply_cancel(&self, wtx: &mut ultima_db::WriteTx, order_id: i64) {
        let (lid, side, t, rem, prev, next, slot) = {
            let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
            let o = orders.get(order_id as u64).expect("order").clone();
            let rem = o.qty - o.filled;
            let t = self.tick_of(o.price);
            let lid = self.level_id(o.side, t);
            // Fix the neighbours' links before the row goes away.
            if o.prev != NIL {
                let pid = o.prev as u64 + 1;
                let mut p = orders.get(pid).expect("prev order").clone();
                p.next = o.next;
                orders.update(pid, p).expect("prev update");
            }
            if o.next != NIL {
                let nid = o.next as u64 + 1;
                let mut n = orders.get(nid).expect("next order").clone();
                n.prev = o.prev;
                orders.update(nid, n).expect("next update");
            }
            orders.delete(order_id as u64).expect("order delete");
            (lid, o.side, t, rem, o.prev, o.next, o.slot)
        };
        let emptied = {
            let mut levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            let mut lvl = levels.get(lid).expect("level").clone();
            if lvl.head == slot {
                lvl.head = next;
            }
            if lvl.tail == slot {
                lvl.tail = prev;
            }
            lvl.qty_total -= rem;
            lvl.count -= 1;
            let emptied = lvl.head == NIL;
            levels.update(lid, lvl).expect("level update");
            emptied
        };
        if emptied {
            self.repair_best(wtx, side, t);
        }
    }

    /// Ladder rescan after a removal emptied level `t` — same semantics as
    /// `Book::repair_best`, reading level rows instead of an array.
    fn repair_best(&self, wtx: &mut ultima_db::WriteTx, side: u8, t: u32) {
        let mut meta = wtx.open_table::<MetaRec>("meta").expect("meta");
        let mut m = meta.get(1).expect("meta rec").clone();
        let levels = wtx.open_table::<LevelRec>("levels").expect("levels");
        if side == 0 {
            if m.best_bid != t as i32 {
                return;
            }
            let mut nb = -1i32;
            for i in (0..=t).rev() {
                if levels
                    .get(self.level_id(0, i))
                    .is_some_and(|l| l.head != NIL)
                {
                    nb = i as i32;
                    break;
                }
            }
            m.best_bid = nb;
        } else {
            if m.best_ask != t as i32 {
                return;
            }
            let mut na = -1i32;
            for i in t..self.n_levels {
                if levels
                    .get(self.level_id(1, i))
                    .is_some_and(|l| l.head != NIL)
                {
                    na = i as i32;
                    break;
                }
            }
            m.best_ask = na;
        }
        meta.update(1, m).expect("meta update");
    }

    pub fn cancel(&mut self, order_id: i64) {
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        self.apply_cancel(&mut wtx, order_id);
        wtx.commit().expect("commit");
    }

    /// See `apply_cancel`: a full fill and a cancel are the same op here.
    pub fn fill(&mut self, order_id: i64) {
        self.cancel(order_id)
    }
```

**Borrow note:** `repair_best` opens `meta` and `levels` in one scope. If the two `open_table` calls conflict, take the `open_tables2::<MetaRec, LevelRec>` path the batched cells already use (`lib.rs:367`) rather than restructuring the logic.

Add the trait impl:

```rust
impl smr_collections_common::churn::ChurnStore for UltimaBook {
    fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        UltimaBook::insert(self, order_id, price, qty, side)
    }
    fn cancel(&mut self, order_id: i64) {
        UltimaBook::cancel(self, order_id)
    }
    fn fill(&mut self, order_id: i64) {
        UltimaBook::fill(self, order_id)
    }
}
```

- [ ] **Step 5: Add the batched cancel path**

Mirror `update_batch_txn_multi` (`lib.rs:357`): one txn, tables opened once via `open_tables2::<OrderRec, LevelRec>`, `apply_cancel`'s body inlined against the handed-in writers. Name it `cancel_batch_txn_multi`, with a `cancel_batch_txn` per-command twin exactly as the insert/update pairs do.

- [ ] **Step 6: Make `encode_at` tolerate a sparse key space**

The orders group currently passes `m.hwm as u16` as its count and assumes `orders.iter()` yields every slot `0..hwm`. With deletes it does not. Change the count to the table's live length and emit each row's own slot:

```rust
        let mut og = enc.orders_encoder(orders.len() as u16, OrdersEncoder::default());
        for (oid, o) in orders.iter() {
            og.advance().expect("orders advance");
            og.slot(o.slot);
            og.order_id(oid as i64);
            // remaining field writes unchanged
        }
```

Leave the ascending-id `debug_assert` in place — it still holds, ids are just sparse now. Set `enc.free_head(NIL)` in the fixed block: ultima has no free list.

- [ ] **Step 7: Add `digest_ultima`**

A free function alongside `encode_at`, producing bytes identical to `digest_book` for the same logical state. Open a read txn at `version`, then follow `digest_book`'s exact field order: `best_bid`, `best_ask`, live count, then `(side, tick)` ascending emitting each occupied level's header and its head→tail order-ID chain (each link's ID is `slot + 1`), then every live order sorted by ID. Iterate ticks `0..n_levels` per side via `self.level_id(side, t)` rather than iterating the levels table, so ordering does not depend on the id mapping.

- [ ] **Step 8: Run tests to verify they pass**

```sh
cd rust && cargo test -p smr-collections-ultima-common
```

Expected: PASS, including the pre-existing `ultima_matches_golden_bytes` and `batched_insert_matches_golden_bytes` — an insert-only workload deletes nothing, so those still byte-match the regenerated golden.

- [ ] **Step 9: Commit**

```sh
cd rust && cargo fmt && cargo clippy --all-targets -- -D warnings
git add rust/smr-collections/ultima-common/src/lib.rs
git commit -m "feat(smrcoll): UltimaBook cancel/fill via orders.delete()

Slots stay monotone — a B-tree has no pool, and emulating the flat store's
free list would mean benchmarking the emulation. Insert switches to
insert_with_id because the churn stream's order IDs are sparse; encode_at
no longer assumes 0..hwm is dense; equivalence with the flat book is checked
by canonical digest rather than by bytes."
```

---

### Task 9: The three ultima cells

**Files:**
- Create: `rust/smr-collections/{ultima_churn,ultima_batch_churn,live_ultima_churn}/{Cargo.toml,src/main.rs}`
- Modify: `rust/Cargo.toml`

- [ ] **Step 1: `ultima_churn`**

Identical in shape to `churn/src/main.rs` from Task 6 with `UltimaBook::new(&cfg)` as the store and `EXPERIMENT = "ultima_churn"`. Its `Cargo.toml` copies `smr-collections/ultima_insert/Cargo.toml` with the package name changed.

- [ ] **Step 2: `ultima_batch_churn`**

Follow `ultima_batch_insert/src/main.rs`: collect `cfg.apply_batch` ops from `churn.next_op()` into a `Vec<ChurnOp>`, then apply the whole batch in one txn, timing the batch. Because a batch mixes op types, emit `batch_mean` for the txn plus the per-op-type split derived from `batch_ns / batch_len` — mirror whatever `ultima_batch_insert` already does for `batch_*` rather than inventing a convention.

Route inserts, cancels, and fills within a batch through the multi-table path under `SMRC_MULTI_TABLE`, matching the existing batched cells.

- [ ] **Step 3: `live_ultima_churn`**

Mirror `live_ultima/src/main.rs` with the churn stream, `pin_current()` at the snapshot trigger, and the `rss_peak_bytes` / per-op-split additions from Task 6 Step 4 — **including Task 6's rule that `rss_bytes()` is sampled only after the timed window closes**, never inside it.

- [ ] **Step 4: Build and smoke-run**

```sh
cd rust && cargo build --release && cargo clippy --all-targets -- -D warnings && cargo fmt --check
SMRC_CAP=65536 SMRC_STEADY=4000 SMRC_WARMUP=500 SMRC_ITERS=5000 \
  cargo run --release -p smr-collections-ultima_churn
SMRC_CAP=65536 SMRC_STEADY=4000 SMRC_WARMUP=500 SMRC_ITERS=5000 \
  SMRC_MULTI_TABLE=1 cargo run --release -p smr-collections-ultima_batch_churn
SMRC_CAP=65536 SMRC_STEADY=4000 SMRC_LIVE_ITERS=5000 SMRC_SNAP_EVERY=1000 \
  cargo run --release -p smr-collections-live_ultima_churn
```

Expected: result-contract lines only on stdout. **Local fitness check — do not journal.**

- [ ] **Step 5: Commit**

```sh
git add rust/Cargo.toml rust/smr-collections/ultima_churn \
        rust/smr-collections/ultima_batch_churn rust/smr-collections/live_ultima_churn
git commit -m "feat(smrcoll): ultima churn cells

ultima_churn, ultima_batch_churn, live_ultima_churn — the cells that put a
number on version reclamation under a cancel-dominated stream."
```

---

## Plan Self-Review

**Spec coverage.** Op stream → Task 4. `Book` → Task 2. `CowBook` → Task 5. `UltimaBook` → Task 8. Schema v2 → Task 3. Canonical digest → Task 7. Metrics → Tasks 4, 6, 9. Config → Task 1. Error handling → Tasks 1–3 (capacity panic, unknown-ID `expect`, version/capacity/crc rejection on restore). Testing item 1 → Task 4; item 2's flat-vs-CoW half → Task 5 and its ultima half → Task 8, with the cross-*language* half in plans 2 and 3; items 3–5 → Tasks 2, 5, 8. Infra and docs are plan 3.

**One gap carried forward deliberately.** The spec's testing item 3 asks for a general invariant that walking `head → next → tail` visits exactly `count` orders; Task 2 instead tests the link fixups directly on three- and two-order levels. The digest tests in Tasks 7–8 walk those chains over 10 K churn ops, which covers the same ground empirically.

**Two known unknowns, both flagged at the point of use rather than guessed:**

- The SBE Rust codegen's snake_case split is not predictable from the field name (`nLevels` → `nl_evels`). Task 3 Step 2 greps the regenerated source for the real accessor name before any code is written against it.
- Task 8's `repair_best` opens two tables in one scope; if the borrow does not check out, the note there says to take the existing `open_tables2` path rather than restructure the logic.

**Sequencing note.** Task 8 is the one place where a wrong assumption would be expensive, because it is the only task touching a pinned external dependency (`ultima_db` rev `8831c4e`). Its Step 1 test — flat-vs-ultima digest equality over 5 K churn ops — fails loudly and early if anything about `delete`/`insert_with_id` behaves differently than read here.
