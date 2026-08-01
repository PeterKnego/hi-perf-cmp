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
use smr_collections_common::book::{NIL, NoHash};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use ultima_db::{
    AddOptions, BulkLoadInput, BulkLoadOptions, BulkSource, Store, StoreConfig, WriterMode,
};
// Re-exported so cell binaries can name the pin type without a direct
// ultima_db dependency.
pub use ultima_db::VersionPin;

const HEADER_LEN: usize = 8;

/// `order_id -> ultima row id`, with the same fixed integer hasher the flat
/// `Book` uses so the probe costs the same in both stores.
type IdMap = HashMap<i64, u64, BuildHasherDefault<NoHash>>;

/// A row's slot handle is exactly `row_id - 1` — the table's auto-increment
/// counter is dense and monotone and slots are never recycled — so the slot is
/// derived rather than stored, and a link's row id is `slot + 1`.
#[inline]
fn slot_of(row: u64) -> u32 {
    (row - 1) as u32
}

#[inline]
fn row_of(slot: u32) -> u64 {
    slot as u64 + 1
}

#[derive(Clone, Debug)]
pub struct OrderRec {
    /// The command's order id. Once the churn stream's ids go sparse they can
    /// no longer double as the ultima row id, so the row has to carry its own.
    pub order_id: i64,
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
    /// `order_id -> ultima row id`. The churn stream's order ids are sparse
    /// (1, 3, 5, …) and never reused, so they can no longer double as row ids
    /// — the row id comes from the table's auto-increment counter instead.
    /// This mirrors the flat `Book`'s id-map, which pays the same probe on
    /// every update/cancel/fill; ultima previously got that lookup for free.
    ids: IdMap,
}

impl UltimaBook {
    /// Store + config only — no seeding txn. Used by `restore_ultima`,
    /// which installs the full table set via `bulk_load_batch` instead.
    fn empty(cfg: &SmrConfig) -> UltimaBook {
        let store = Store::new(
            StoreConfig::builder()
                .writer_mode(WriterMode::SingleWriter)
                .require_explicit_version(true)
                // Retention stays at the default (10). Keeping a captured
                // version alive for the live_ultima serializer is done with
                // `pin_current()` (a `Send` `VersionPin` travels with the
                // handoff), not a retention window — a large window made
                // every commit pay an O(retained) auto-GC scan on the
                // previously pinned ultima_db rev.
                .build(),
        )
        .expect("store");
        UltimaBook {
            store: Arc::new(store),
            version: 0,
            price_min: cfg.price_min,
            tick: cfg.tick,
            n_levels: cfg.levels,
            ids: IdMap::with_capacity_and_hasher(cfg.cap, BuildHasherDefault::default()),
        }
    }

    pub fn new(cfg: &SmrConfig) -> UltimaBook {
        let mut ub = Self::empty(cfg);
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

    /// Resolve a command's order id to its ultima row id.
    #[inline]
    fn row_id(&self, order_id: i64) -> u64 {
        self.ids[&order_id]
    }

    /// Pin the current version for handoff to a serializer thread.
    ///
    /// `VersionPin` is `Send + Clone`, so it travels with the version number
    /// and keeps the snapshot alive until the serializer's `begin_read`
    /// (inside `encode_at`) opens its own view — no retention-window sizing
    /// needed. The commit-vs-pin race documented on `Store::pin_version`
    /// cannot occur here: all commits go through `&mut self`, so nothing can
    /// commit between reading `self.version` and pinning it.
    pub fn pin_current(&self) -> VersionPin {
        self.store
            .pin_version(Some(self.version))
            .expect("current version exists")
    }

    fn apply_insert(
        &mut self,
        wtx: &mut ultima_db::WriteTx,
        order_id: i64,
        price: i64,
        qty: i64,
        side: u8,
    ) {
        let t = self.tick_of(price);
        let lid = self.level_id(side, t);
        let mut lvl = {
            let levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            levels.get(lid).expect("level").clone()
        };
        let prev_tail = lvl.tail;
        let slot;
        {
            let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
            // The row id is the table's auto-increment counter: dense, monotone
            // and never recycled (a B-tree has no pool to recycle from), so
            // `slot = row - 1` stays a valid handle and a link's row id stays
            // `slot + 1` — which is what the level FIFO walks rely on.
            let row = orders
                .insert(OrderRec {
                    order_id,
                    price,
                    qty,
                    filled: 0,
                    side,
                    next: NIL,
                    prev: prev_tail,
                })
                .expect("order insert");
            slot = slot_of(row);
            self.ids.insert(order_id, row);
            if prev_tail != NIL {
                let pid = row_of(prev_tail); // row id of the previous tail
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
    }

    fn apply_update(&self, wtx: &mut ultima_db::WriteTx, order_id: i64, fill_qty: i64) {
        let row = self.row_id(order_id);
        let (lid, add) = {
            let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
            let mut o = orders.get(row).expect("order").clone();
            let add = fill_qty.min(o.qty - o.filled);
            o.filled += add;
            let t = self.tick_of(o.price);
            let lid = self.level_id(o.side, t);
            orders.update(row, o).expect("order update");
            (lid, add)
        };
        {
            let mut levels = wtx.open_table::<LevelRec>("levels").expect("levels");
            let mut lvl = levels.get(lid).expect("level").clone();
            lvl.qty_total -= add;
            levels.update(lid, lvl).expect("level update");
        }
    }

    /// Remove a resting order. A cancel and a full fill are the same operation
    /// here: the row is deleted either way, and the level loses the same
    /// remaining quantity. The flat store distinguishes them only in the
    /// `filled` field it leaves behind in a freed pool slot, which ultima has
    /// no equivalent of.
    fn apply_cancel(&mut self, wtx: &mut ultima_db::WriteTx, order_id: i64) {
        let row = self
            .ids
            .remove(&order_id)
            .expect("cancel: unknown order id");
        let slot = slot_of(row);
        let (lid, side, t, rem, prev, next) = {
            let mut orders = wtx.open_table::<OrderRec>("orders").expect("orders");
            let o = orders.get(row).expect("order").clone();
            let rem = o.qty - o.filled;
            let t = self.tick_of(o.price);
            let lid = self.level_id(o.side, t);
            // Fix the neighbours' links before the row goes away.
            if o.prev != NIL {
                let pid = row_of(o.prev);
                let mut p = orders.get(pid).expect("prev order").clone();
                p.next = o.next;
                orders.update(pid, p).expect("prev update");
            }
            if o.next != NIL {
                let nid = row_of(o.next);
                let mut n = orders.get(nid).expect("next order").clone();
                n.prev = o.prev;
                orders.update(nid, n).expect("next update");
            }
            orders.delete(row).expect("order delete");
            (lid, o.side, t, rem, o.prev, o.next)
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
    /// `Book::repair_best`, reading level rows instead of an array. Opens the
    /// two tables together: `WriteTx::open_table` takes `&mut self`, so two
    /// live handles from one txn need the `open_tables2` path.
    fn repair_best(&self, wtx: &mut ultima_db::WriteTx, side: u8, t: u32) {
        let (mut meta, levels) = wtx
            .open_tables2::<MetaRec, LevelRec>("meta", "levels")
            .expect("open_tables2");
        let mut m = meta.get(1).expect("meta rec").clone();
        if !self.rescan_best(&levels, &mut m, side, t) {
            return;
        }
        meta.update(1, m).expect("meta update");
    }

    /// The rescan itself. Returns false when the emptied level was not the
    /// cached best, so nothing needs writing back. Shared by the per-op and
    /// batched paths, which both hold a `TableWriter` over `levels`.
    fn rescan_best(
        &self,
        levels: &ultima_db::TableWriter<'_, LevelRec>,
        m: &mut MetaRec,
        side: u8,
        t: u32,
    ) -> bool {
        let occupied = |lid: u64| levels.get(lid).is_some_and(|l| l.head != NIL);
        if side == 0 {
            if m.best_bid != t as i32 {
                return false;
            }
            let mut nb = -1i32;
            for i in (0..=t).rev() {
                if occupied(self.level_id(0, i)) {
                    nb = i as i32;
                    break;
                }
            }
            m.best_bid = nb;
        } else {
            if m.best_ask != t as i32 {
                return false;
            }
            let mut na = -1i32;
            for i in t..self.n_levels {
                if occupied(self.level_id(1, i)) {
                    na = i as i32;
                    break;
                }
            }
            m.best_ask = na;
        }
        true
    }

    pub fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        self.apply_insert(&mut wtx, order_id, price, qty, side);
        wtx.commit().expect("commit");
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

    pub fn update(&mut self, order_id: i64, fill_qty: i64) {
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        self.apply_update(&mut wtx, order_id, fill_qty);
        wtx.commit().expect("commit");
    }

    /// Apply a batch of insert commands in ONE write txn (the SMR
    /// consensus-batch pattern). Per-command work is identical to
    /// `insert` — the cells differ only in txn amortization.
    /// Per-command insert through already-open table writers (issue #20): the
    /// body is identical to `apply_insert`, only the tables are handed in
    /// (opened once per batch) instead of re-opened here.
    #[allow(clippy::too_many_arguments)]
    fn apply_insert_mt(
        &mut self,
        levels: &mut ultima_db::TableWriter<'_, LevelRec>,
        orders: &mut ultima_db::TableWriter<'_, OrderRec>,
        meta: &mut ultima_db::TableWriter<'_, MetaRec>,
        order_id: i64,
        price: i64,
        qty: i64,
        side: u8,
    ) {
        let t = self.tick_of(price);
        let lid = self.level_id(side, t);
        let mut lvl = levels.get(lid).expect("level").clone();
        let prev_tail = lvl.tail;
        let row = orders
            .insert(OrderRec {
                order_id,
                price,
                qty,
                filled: 0,
                side,
                next: NIL,
                prev: prev_tail,
            })
            .expect("order insert");
        let slot = slot_of(row);
        self.ids.insert(order_id, row);
        if prev_tail != NIL {
            let pid = row_of(prev_tail);
            let mut p = orders.get(pid).expect("prev order").clone();
            p.next = slot;
            orders.update(pid, p).expect("prev update");
        }
        if lvl.tail == NIL {
            lvl.head = slot;
        }
        lvl.tail = slot;
        lvl.qty_total += qty;
        lvl.count += 1;
        levels.update(lid, lvl).expect("level update");
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

    /// Per-command update through already-open writers; body identical to
    /// `apply_update`.
    fn apply_update_mt(
        &self,
        orders: &mut ultima_db::TableWriter<'_, OrderRec>,
        levels: &mut ultima_db::TableWriter<'_, LevelRec>,
        order_id: i64,
        fill_qty: i64,
    ) {
        let row = self.row_id(order_id);
        let mut o = orders.get(row).expect("order").clone();
        let add = fill_qty.min(o.qty - o.filled);
        o.filled += add;
        let t = self.tick_of(o.price);
        let lid = self.level_id(o.side, t);
        orders.update(row, o).expect("order update");
        let mut lvl = levels.get(lid).expect("level").clone();
        lvl.qty_total -= add;
        levels.update(lid, lvl).expect("level update");
    }

    /// Per-command cancel through already-open writers; body identical to
    /// `apply_cancel`, only the tables are handed in (opened once per batch).
    fn apply_cancel_mt(
        &mut self,
        orders: &mut ultima_db::TableWriter<'_, OrderRec>,
        levels: &mut ultima_db::TableWriter<'_, LevelRec>,
        meta: &mut ultima_db::TableWriter<'_, MetaRec>,
        order_id: i64,
    ) {
        let row = self
            .ids
            .remove(&order_id)
            .expect("cancel: unknown order id");
        let slot = slot_of(row);
        let o = orders.get(row).expect("order").clone();
        let rem = o.qty - o.filled;
        let t = self.tick_of(o.price);
        let lid = self.level_id(o.side, t);
        // Fix the neighbours' links before the row goes away.
        if o.prev != NIL {
            let pid = row_of(o.prev);
            let mut p = orders.get(pid).expect("prev order").clone();
            p.next = o.next;
            orders.update(pid, p).expect("prev update");
        }
        if o.next != NIL {
            let nid = row_of(o.next);
            let mut n = orders.get(nid).expect("next order").clone();
            n.prev = o.prev;
            orders.update(nid, n).expect("next update");
        }
        orders.delete(row).expect("order delete");
        let mut lvl = levels.get(lid).expect("level").clone();
        if lvl.head == slot {
            lvl.head = o.next;
        }
        if lvl.tail == slot {
            lvl.tail = o.prev;
        }
        lvl.qty_total -= rem;
        lvl.count -= 1;
        let emptied = lvl.head == NIL;
        levels.update(lid, lvl).expect("level update");
        if emptied {
            let mut m = meta.get(1).expect("meta rec").clone();
            if self.rescan_best(levels, &mut m, o.side, t) {
                meta.update(1, m).expect("meta update");
            }
        }
    }

    /// Batched insert opening the three tables ONCE per txn via `open_tables3`
    /// (issue #20). Golden-equivalent to `insert_batch_txn`; the difference is
    /// only that the tables are not re-opened per command.
    pub fn insert_batch_txn_multi(&mut self, cmds: &[(i64, i64, i64, u8)]) {
        if cmds.is_empty() {
            return;
        }
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        {
            let (mut levels, mut orders, mut meta) = wtx
                .open_tables3::<LevelRec, OrderRec, MetaRec>("levels", "orders", "meta")
                .expect("open_tables3");
            for &(order_id, price, qty, side) in cmds {
                self.apply_insert_mt(
                    &mut levels,
                    &mut orders,
                    &mut meta,
                    order_id,
                    price,
                    qty,
                    side,
                );
            }
        }
        wtx.commit().expect("commit");
    }

    /// Batched update opening the two tables ONCE per txn via `open_tables2`.
    /// Golden-equivalent to `update_batch_txn`.
    pub fn update_batch_txn_multi(&mut self, cmds: &[(i64, i64)]) {
        if cmds.is_empty() {
            return;
        }
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        {
            let (mut orders, mut levels) = wtx
                .open_tables2::<OrderRec, LevelRec>("orders", "levels")
                .expect("open_tables2");
            for &(order_id, fill_qty) in cmds {
                self.apply_update_mt(&mut orders, &mut levels, order_id, fill_qty);
            }
        }
        wtx.commit().expect("commit");
    }

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

    /// Batched cancel opening the three tables ONCE per txn via `open_tables3`
    /// — `meta` is in the set because an emptied best level makes `apply_cancel`
    /// rescan the ladder. Equivalent to `cancel_batch_txn`.
    pub fn cancel_batch_txn_multi(&mut self, cmds: &[i64]) {
        if cmds.is_empty() {
            return;
        }
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        {
            let (mut orders, mut levels, mut meta) = wtx
                .open_tables3::<OrderRec, LevelRec, MetaRec>("orders", "levels", "meta")
                .expect("open_tables3");
            for &order_id in cmds {
                self.apply_cancel_mt(&mut orders, &mut levels, &mut meta, order_id);
            }
        }
        wtx.commit().expect("commit");
    }

    /// Batched analog of `cancel`; see `insert_batch_txn`.
    pub fn cancel_batch_txn(&mut self, cmds: &[i64]) {
        if cmds.is_empty() {
            return;
        }
        self.version += 1;
        let mut wtx = self.store.begin_write(Some(self.version)).expect("wtx");
        for &order_id in cmds {
            self.apply_cancel(&mut wtx, order_id);
        }
        wtx.commit().expect("commit");
    }
}

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
        // The ultima variant has no free list yet (cancel/fill are Book-only,
        // task 2); NIL mirrors what Book.free_head is for any cancel-free
        // run, keeping byte-parity with snapshot::encode for the same state.
        enc.free_head(NIL);

        let mut lg = enc.levels_encoder(level_count, LevelsEncoder::default());
        // Level ids are side*nLevels + tick + 1: id order IS bids-then-asks,
        // ascending tick — the STW encoder's lane order. This relies on
        // `TableReader::iter()` enumerating in ascending-id order, which is
        // id-ordered by construction: ultima_db's `Table::iter()` is
        // documented "Iterate over all records in ID order" and delegates to
        // `self.range(..)` over `Table.data`, a persistent CoW B-tree keyed
        // by record id (src/table.rs at the pinned rev; `dashmap` appears
        // elsewhere in the store but not for per-table record storage).
        // Checked below via `debug_assert!` so the test suite (which runs in
        // debug) proves monotonicity, not just trusts it.
        #[cfg(debug_assertions)]
        let mut prev_lid: Option<u64> = None;
        for (lid, l) in levels.iter() {
            #[cfg(debug_assertions)]
            {
                if let Some(prev) = prev_lid {
                    debug_assert!(
                        lid > prev,
                        "levels.iter() must yield strictly ascending ids (got {lid} after {prev})"
                    );
                }
                prev_lid = Some(lid);
            }
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

        // Live rows only: once cancels exist the key space is sparse, so the
        // group count is the table's length, not `hwm`, and each row carries
        // its own slot. This is where the ultima image legitimately parts ways
        // with the flat stores' dense-pool-with-holes image — equivalence is
        // proven by `digest_ultima` instead. (Insert-only runs delete nothing,
        // so `len() == hwm` and the golden image is unchanged.)
        let mut og = enc.orders_encoder(orders.len() as u16, OrdersEncoder::default());
        // Row ids stay strictly ascending in slot order — the auto-increment
        // counter is monotone and slots are never recycled — they are just
        // sparse now. Same ascending-id guarantee as the levels walk above
        // (ultima_db's `Table::iter()` over its id-keyed B-tree), checked the
        // same way.
        #[cfg(debug_assertions)]
        let mut prev_oid: Option<u64> = None;
        for (oid, o) in orders.iter() {
            #[cfg(debug_assertions)]
            {
                if let Some(prev) = prev_oid {
                    debug_assert!(
                        oid > prev,
                        "orders.iter() must yield strictly ascending ids (got {oid} after {prev})"
                    );
                }
                prev_oid = Some(oid);
            }
            og.advance().expect("orders advance");
            og.slot(slot_of(oid));
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

/// Canonical, representation-free digest of the state at `version` — the exact
/// bytes `smr_collections_common::digest::digest_book` produces for the same
/// logical book. This, not the snapshot image, is what proves ultima agrees
/// with the flat stores once cancels exist: ultima allocates slots monotonically
/// and never recycles, so its slot numbering (and therefore its image) diverges
/// from a recycling pool's while the book itself is identical.
pub fn digest_ultima(store: &Store, version: u64) -> Vec<u8> {
    let rtx = store.begin_read(Some(version)).expect("read txn");
    let levels = rtx.open_table::<LevelRec>("levels").expect("levels");
    let orders = rtx.open_table::<OrderRec>("orders").expect("orders");
    let meta_t = rtx.open_table::<MetaRec>("meta").expect("meta");
    let m = meta_t.get(1).expect("meta rec");

    let mut out = Vec::with_capacity(1 << 16);
    out.extend_from_slice(&m.best_bid.to_le_bytes());
    out.extend_from_slice(&m.best_ask.to_le_bytes());
    out.extend_from_slice(&(orders.len() as u32).to_le_bytes());
    // Ticks are walked 0..n_levels per side rather than by iterating the levels
    // table, so the ordering cannot depend on the level-id mapping.
    for side in 0..2u8 {
        for t in 0..m.n_levels {
            let lid = side as u64 * m.n_levels as u64 + t as u64 + 1;
            let Some(l) = levels.get(lid) else { continue };
            if l.head == NIL {
                continue;
            }
            out.push(side);
            out.extend_from_slice(&t.to_le_bytes());
            out.extend_from_slice(&l.qty_total.to_le_bytes());
            out.extend_from_slice(&l.count.to_le_bytes());
            let mut s = l.head;
            while s != NIL {
                let o = orders.get(row_of(s)).expect("chain order");
                out.extend_from_slice(&o.order_id.to_le_bytes());
                s = o.next;
            }
        }
    }
    let mut live: Vec<(i64, i64, i64, i64, u8)> = orders
        .iter()
        .map(|(_, o)| (o.order_id, o.price, o.qty, o.filled, o.side))
        .collect();
    live.sort_unstable();
    for (id, price, qty, filled, side) in live {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&price.to_le_bytes());
        out.extend_from_slice(&qty.to_le_bytes());
        out.extend_from_slice(&filled.to_le_bytes());
        out.push(side);
    }
    out
}

/// Restore a fresh UltimaBook from an encoded image (crc-verified): build the
/// empty store, then install the decoded state as one atomic bulk-load batch.
pub fn restore_ultima(bytes: &[u8], cfg: &SmrConfig) -> Result<UltimaBook, String> {
    if bytes.len() < 4 {
        return Err("snapshot too short".into());
    }
    let sbe_len = bytes.len() - 4;
    let want = u32::from_le_bytes(bytes[sbe_len..].try_into().unwrap());
    if crc32c::crc32c(&bytes[..sbe_len]) != want {
        return Err("crc32c mismatch".into());
    }
    let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(&bytes[..sbe_len]), 0);
    let dec = BookSnapshotDecoder::default().header(header, 0);
    let (price_min, tick, n_levels) = (dec.price_min(), dec.tick_size(), dec.nl_evels());
    let (capacity, hwm) = (dec.capacity(), dec.hwm());
    let (best_bid, best_ask) = (dec.best_bid(), dec.best_ask());
    if n_levels != cfg.levels {
        return Err("nLevels mismatch vs config".into());
    }

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
        let id = row_of(slot);
        // Slots are monotone but sparse once cancels have run, so the group
        // only has to be strictly ascending — `BulkSource::sorted_vec` needs
        // sorted keys, not contiguous ones.
        if orders.last().is_some_and(|&(prev, _)| id <= prev) {
            return Err("orders group not in ascending slot order".into());
        }
        let order_id = og.order_id();
        ub.ids.insert(order_id, id);
        orders.push((
            id,
            OrderRec {
                order_id,
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
            apply_batch: 64,
            multi_table: false,
            live_iters: 200_000,
            snap_every: 20_000,
            otr_bps: 100,
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
        // With default retention (10), v would be GC'd by the 200 commits
        // below — the pin is what keeps it alive.
        let pin = ub.pin_current();
        assert_eq!(pin.version(), v);
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
    fn multitable_insert_matches_golden_bytes() {
        // The open_tables3 apply path (#20) must produce the same state as the
        // per-command open path — same golden image.
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
            ub.insert_batch_txn_multi(chunk);
        }
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode_at(&ub.store, ub.current_version(), &mut buf);
        let golden = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../testdata/golden_snapshot.bin"
        ))
        .expect("golden file");
        assert_eq!(&buf[..n], &golden[..], "open_tables3 apply == golden bytes");
    }

    #[test]
    fn multitable_mixed_ops_match_single_open() {
        // open_tables3/2 path == per-command open path over an insert+update mix.
        let c = cfg();
        let mut a = UltimaBook::new(&c);
        let mut b = UltimaBook::new(&c);
        let mut r1 = SplitMix::new(SEED);
        let mut r2 = SplitMix::new(SEED);
        let ins: Vec<(i64, i64, i64, u8)> = (0..c.steady)
            .map(|i| {
                let x = next_insert(&mut r1, i, c.levels, c.tick, c.price_min);
                let _ = next_insert(&mut r2, i, c.levels, c.tick, c.price_min);
                (x.order_id, x.price, x.qty, x.side)
            })
            .collect();
        for chunk in ins.chunks(c.apply_batch) {
            a.insert_batch_txn(chunk);
            b.insert_batch_txn_multi(chunk);
        }
        let ups: Vec<(i64, i64)> = (0..300)
            .map(|_| {
                let x = next_update(&mut r1, c.steady);
                let _ = next_update(&mut r2, c.steady);
                (x.order_id, x.fill_qty)
            })
            .collect();
        for chunk in ups.chunks(c.apply_batch) {
            a.update_batch_txn(chunk);
            b.update_batch_txn_multi(chunk);
        }
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n1 = encode_at(&a.store, a.current_version(), &mut buf1);
        let n2 = encode_at(&b.store, b.current_version(), &mut buf2);
        assert_eq!(&buf1[..n1], &buf2[..n2], "multi-table == single-open bytes");
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
        let (tx, rx) = std::sync::mpsc::sync_channel::<VersionPin>(1);
        let ser = std::thread::spawn(move || {
            let pin = rx.recv().expect("pin");
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            let n = encode_at(&store, pin.version(), &mut buf);
            buf.truncate(n);
            buf
        });
        for k in 0..total_updates {
            if k == capture_at {
                tx.send(ub.pin_current()).expect("send pin");
            }
            let up = next_update(&mut rng, c.steady);
            ub.update(up.order_id, up.fill_qty);
        }
        let got = ser.join().expect("serializer");
        assert_eq!(&want[..wn], &got[..], "concurrent encode_at == STW replay");
    }

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

        // ...and the digest is doing real work: the two stores hold the same
        // book at genuinely different addresses, so their snapshot IMAGES do
        // not match. A recycling pool refills the holes it makes; ultima's
        // slots march on. If this ever started matching, the digest test above
        // would have degenerated into a byte-equality test.
        let mut fb = vec![0u8; 4 * 1024 * 1024];
        let mut ub = vec![0u8; 4 * 1024 * 1024];
        let fnn = snapshot::encode(&flat, &mut fb);
        let un = encode_at(&ult.store, v, &mut ub);
        assert_ne!(
            &fb[..fnn],
            &ub[..un],
            "slot layouts should have diverged once cancels ran"
        );
    }

    /// A thin book, so levels actually empty and the ladder rescan runs.
    ///
    /// This is NOT covered by `ultima_churn_matches_flat_digest`: at the
    /// default config 2000 live orders spread over 128 levels never leave one
    /// empty, so `repair_best` is never called there. Measured directly — over
    /// that op stream the cached best changes 0 times.
    fn thin_cfg() -> SmrConfig {
        SmrConfig {
            cap: 64,
            levels: 4,
            steady: 6,
            ..cfg()
        }
    }

    #[test]
    fn churn_on_a_thin_book_matches_flat_digest() {
        let c = thin_cfg();
        let mut flat = smr_collections_common::book::Book::new(&c);
        let mut ult = UltimaBook::new(&c);
        let mut cha = smr_collections_common::churn::Churn::new(&c);
        let mut chb = smr_collections_common::churn::Churn::new(&c);
        cha.prebuild(&mut flat, c.steady);
        chb.prebuild(&mut ult, c.steady);
        let mut best_moves = 0usize;
        let (mut pb, mut pa) = (flat.best_bid, flat.best_ask);
        for k in 0..5_000 {
            let (op, op2) = (cha.next_op(), chb.next_op());
            smr_collections_common::churn::Churn::apply(&mut flat, op);
            smr_collections_common::churn::Churn::apply(&mut ult, op2);
            if flat.best_bid != pb || flat.best_ask != pa {
                best_moves += 1;
            }
            pb = flat.best_bid;
            pa = flat.best_ask;
            // Step-by-step, so a divergence names the op that caused it.
            assert_eq!(
                digest_ultima(&ult.store, ult.current_version()),
                smr_collections_common::digest::digest_book(&flat),
                "ultima diverged from the flat book at op {k}"
            );
        }
        assert!(
            best_moves > 100,
            "this test is only meaningful if the ladder rescan runs; \
             best moved only {best_moves} times"
        );
    }

    #[test]
    fn cancel_emptying_best_level_rescans_like_flat() {
        // The flat book's `cancel_emptying_best_level_rescans` scenario, run
        // against both stores: bids fall back down the ladder, asks rise up it,
        // and an emptied side goes to -1.
        let c = thin_cfg();
        let mut flat = smr_collections_common::book::Book::new(&c);
        let mut ult = UltimaBook::new(&c);
        let same = |f: &smr_collections_common::book::Book, u: &UltimaBook, what: &str| {
            assert_eq!(
                digest_ultima(&u.store, u.current_version()),
                smr_collections_common::digest::digest_book(f),
                "{what}"
            );
        };
        // Each side gets THREE occupied levels, so the rescan direction is
        // actually observable: with only one occupied level below the best,
        // scanning up and scanning down give the same answer and a reversed
        // loop would pass. Sparse ids (1, 3, 5, …) mirror the churn stream.
        for &(id, price, qty, side) in &[
            (1i64, 0i64, 10i64, 0u8), // bid @0
            (3, 2, 10, 0),            // bid @2
            (5, 3, 10, 0),            // bid @3 -> best bid 3
            (7, 0, 10, 1),            // ask @0 -> best ask 0
            (9, 1, 10, 1),            // ask @1
            (11, 3, 10, 1),           // ask @3
        ] {
            flat.insert(id, price, qty, side);
            ult.insert(id, price, qty, side);
        }
        same(&flat, &ult, "after inserts");
        assert_eq!((flat.best_bid, flat.best_ask), (3, 0), "starting bests");

        flat.cancel(5);
        ult.cancel(5);
        assert_eq!(flat.best_bid, 2, "bids fall to the NEAREST occupied below");
        same(&flat, &ult, "best bid falls back to the next occupied");

        flat.cancel(7);
        ult.cancel(7);
        assert_eq!(flat.best_ask, 1, "asks rise to the NEAREST occupied above");
        same(&flat, &ult, "best ask rises to the next occupied");

        flat.cancel(3);
        ult.cancel(3);
        same(&flat, &ult, "best bid falls again");
        flat.cancel(1);
        ult.cancel(1);
        assert_eq!(flat.best_bid, -1, "no bids left");
        same(&flat, &ult, "bid side empty");

        // A fill is the same structural op here; it must also rescan.
        flat.fill(9);
        ult.fill(9);
        same(&flat, &ult, "fill rescans the ask ladder too");
        flat.fill(11);
        ult.fill(11);
        assert_eq!(flat.best_ask, -1, "no asks left");
        same(&flat, &ult, "book empty after the last order fills");
    }

    #[test]
    fn batched_cancel_matches_per_op_cancel() {
        // Both batched cancel paths must land the same book as the per-op one.
        let c = cfg();
        let mut per_op = UltimaBook::new(&c);
        let mut batched = UltimaBook::new(&c);
        let mut multi = UltimaBook::new(&c);
        let mut rng = SplitMix::new(SEED);
        let ins: Vec<(i64, i64, i64, u8)> = (0..c.steady)
            .map(|i| {
                let x = next_insert(&mut rng, i, c.levels, c.tick, c.price_min);
                (x.order_id, x.price, x.qty, x.side)
            })
            .collect();
        for &(id, p, q, s) in &ins {
            per_op.insert(id, p, q, s);
            batched.insert(id, p, q, s);
            multi.insert(id, p, q, s);
        }
        // A scattered subset, so head, middle and tail links all get unlinked
        // and some levels empty out and force a ladder rescan.
        let victims: Vec<i64> = ins
            .iter()
            .map(|&(id, ..)| id)
            .filter(|id| id % 3 == 0)
            .collect();
        assert!(
            !victims.is_empty(),
            "the subset must actually cancel something"
        );
        for &id in &victims {
            per_op.cancel(id);
        }
        for chunk in victims.chunks(c.apply_batch) {
            batched.cancel_batch_txn(chunk);
            multi.cancel_batch_txn_multi(chunk);
        }
        let want = digest_ultima(&per_op.store, per_op.current_version());
        assert_eq!(
            want,
            digest_ultima(&batched.store, batched.current_version()),
            "cancel_batch_txn == per-op cancel"
        );
        assert_eq!(
            want,
            digest_ultima(&multi.store, multi.current_version()),
            "cancel_batch_txn_multi == per-op cancel"
        );
    }
}
