//! The limit order book: flat ladder + intrusive-FIFO order pool + id-map.
//! Single-writer, fixed-capacity, zero steady-state allocation. Handle links
//! are `u32` slot indices (position-independent), never pointers — see the
//! plan's "Intrusive links" rationale.

use bench_common::smrcoll::SmrConfig;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Sentinel handle: empty head/tail, link end, "no slot".
pub const NIL: u32 = u32::MAX;

/// One resting order in the pool. `#[repr(C)]`, fixed POD layout.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Order {
    pub order_id: i64,
    pub price: i64,
    pub qty: i64,
    pub filled: i64,
    pub next: u32,
    pub prev: u32,
    pub side: u8,
}

/// One price level (ladder slot): intrusive FIFO head/tail + aggregates.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PriceLevel {
    pub head: u32,
    pub tail: u32,
    pub qty_total: i64,
    pub count: u32,
}

impl PriceLevel {
    pub const EMPTY: PriceLevel = PriceLevel {
        head: NIL,
        tail: NIL,
        qty_total: 0,
        count: 0,
    };
}

/// A fixed-integer-key no-op hasher (fixed hashing — no SipHash/RandomState).
#[derive(Default)]
pub struct NoHash(u64);
impl Hasher for NoHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("only write_i64 is used")
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
    fn write_i64(&mut self, v: i64) {
        self.0 = v as u64;
    }
}
pub type IdMap = HashMap<i64, u32, BuildHasherDefault<NoHash>>;

#[derive(Debug)]
pub struct Book {
    pub price_min: i64,
    pub tick: i64,
    pub n_levels: u32,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub pool: Vec<Order>,
    pub hwm: u32,
    pub best_bid: i32,
    pub best_ask: i32,
    pub idmap: IdMap,
    /// Head of the intrusive LIFO free list (`NIL` when empty). Freed slots
    /// chain through their own `next` field. This is state a snapshot must
    /// capture — restore reproduces allocation order from it.
    pub free_head: u32,
}

impl Book {
    pub fn new(cfg: &SmrConfig) -> Book {
        let n = cfg.levels as usize;
        Book {
            price_min: cfg.price_min,
            tick: cfg.tick,
            n_levels: cfg.levels,
            bids: vec![PriceLevel::EMPTY; n],
            asks: vec![PriceLevel::EMPTY; n],
            pool: vec![
                Order {
                    order_id: 0,
                    price: 0,
                    qty: 0,
                    filled: 0,
                    next: NIL,
                    prev: NIL,
                    side: 0
                };
                cfg.cap
            ],
            hwm: 0,
            best_bid: -1,
            best_ask: -1,
            idmap: IdMap::with_capacity_and_hasher(cfg.cap, BuildHasherDefault::default()),
            free_head: NIL,
        }
    }

    #[inline]
    fn tick_of(&self, price: i64) -> u32 {
        ((price - self.price_min) / self.tick) as u32
    }

    #[inline]
    fn lane(&mut self, side: u8) -> &mut Vec<PriceLevel> {
        if side == 0 {
            &mut self.bids
        } else {
            &mut self.asks
        }
    }

    pub fn insert(&mut self, order_id: i64, price: i64, qty: i64, side: u8) {
        let t = self.tick_of(price);
        let slot = self.alloc_slot();
        let prev_tail = self.lane(side)[t as usize].tail;
        self.pool[slot as usize] = Order {
            order_id,
            price,
            qty,
            filled: 0,
            next: NIL,
            prev: prev_tail,
            side,
        };
        {
            let lvl = &mut self.lane(side)[t as usize];
            if lvl.tail == NIL {
                lvl.head = slot;
            }
            lvl.tail = slot;
            lvl.qty_total += qty;
            lvl.count += 1;
        }
        if prev_tail != NIL {
            self.pool[prev_tail as usize].next = slot;
        }
        self.idmap.insert(order_id, slot);
        if side == 0 && (self.best_bid < 0 || t as i32 > self.best_bid) {
            self.best_bid = t as i32;
        }
        if side == 1 && (self.best_ask < 0 || (t as i32) < self.best_ask) {
            self.best_ask = t as i32;
        }
    }

    pub fn update(&mut self, order_id: i64, fill_qty: i64) {
        let slot = self.idmap[&order_id] as usize;
        let (side, price, add) = {
            let o = &mut self.pool[slot];
            let add = fill_qty.min(o.qty - o.filled);
            o.filled += add;
            (o.side, o.price, add)
        };
        let t = self.tick_of(price);
        self.lane(side)[t as usize].qty_total -= add;
    }

    pub fn get_slot(&self, order_id: i64) -> u32 {
        self.idmap[&order_id]
    }
    pub fn best_bid(&self) -> i32 {
        self.best_bid
    }
    pub fn best_ask(&self) -> i32 {
        self.best_ask
    }
    pub fn hwm(&self) -> u32 {
        self.hwm
    }
    pub fn level_qty(&self, side: u8, tick: u32) -> i64 {
        let lane = if side == 0 { &self.bids } else { &self.asks };
        lane[tick as usize].qty_total
    }

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
        // Not load-bearing for the free list itself (nothing reads a freed
        // slot's `prev`), but IS load-bearing for snapshot byte-identity: the
        // freed slot's `prev` field is still serialized verbatim, so leaving
        // stale data here would make the image depend on what the slot held
        // before it was freed rather than being deterministic from the op
        // stream alone.
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
        let slot = self
            .idmap
            .remove(&order_id)
            .expect("fill: unknown order id");
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
}

/// Deterministic workload derivation (see plan Appendix A.3).
pub mod workload {
    use crate::rng::SplitMix;

    pub struct Insert {
        pub order_id: i64,
        pub price: i64,
        pub qty: i64,
        pub side: u8,
    }
    pub struct Update {
        pub order_id: i64,
        pub fill_qty: i64,
    }

    /// The i-th insert (0-based). `n_levels`, `tick`, `price_min` from config.
    pub fn next_insert(
        rng: &mut SplitMix,
        i: usize,
        n_levels: u32,
        tick: i64,
        price_min: i64,
    ) -> Insert {
        let r1 = rng.next();
        let r2 = rng.next();
        let t = (r1 % n_levels as u64) as i64;
        let side = ((r1 >> 32) & 1) as u8;
        Insert {
            order_id: (i as i64) + 1,
            price: price_min + t * tick,
            qty: 1 + (r2 % 1000) as i64,
            side,
        }
    }

    /// The next update against a book of `n` live orders.
    pub fn next_update(rng: &mut SplitMix, n: usize) -> Update {
        let u = rng.next();
        Update {
            order_id: ((u % n as u64) + 1) as i64,
            fill_qty: 1 + ((u >> 32) % 100) as i64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bench_common::smrcoll::SmrConfig;

    fn cfg() -> SmrConfig {
        SmrConfig {
            cap: 1024,
            levels: 16,
            tick: 1,
            price_min: 0,
            steady: 100,
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
    fn insert_places_order_and_updates_level_and_best() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0); // bid @ tick 5, qty 10
        b.insert(2, 5, 7, 0); // bid @ tick 5, qty 7 (same level, FIFO)
        b.insert(3, 8, 3, 1); // ask @ tick 8
        assert_eq!(b.level_qty(0, 5), 17);
        assert_eq!(b.level_qty(1, 8), 3);
        assert_eq!(b.best_bid(), 5);
        assert_eq!(b.best_ask(), 8);
        assert_eq!(b.get_slot(2), 1); // second acquire => slot 1
    }

    #[test]
    fn update_partial_fill_reduces_level_qty_capped() {
        let mut b = Book::new(&cfg());
        b.insert(1, 5, 10, 0);
        b.update(1, 4); // fill 4 -> remaining 6
        assert_eq!(b.level_qty(0, 5), 6);
        b.update(1, 100); // over-fill capped at remaining 6 -> 0
        assert_eq!(b.level_qty(0, 5), 0);
    }

    #[test]
    fn level_qty_equals_sum_remaining_invariant() {
        let mut b = Book::new(&cfg());
        for i in 0..50i64 {
            b.insert(i + 1, i % 16, 10, (i % 2) as u8);
        }
        for i in 0..50i64 {
            b.update(i + 1, 3);
        }
        // recompute expected per level
        let mut expect = std::collections::HashMap::new();
        for i in 0..50i64 {
            let t = i % 16;
            let side = (i % 2) as u8;
            *expect.entry((side, t)).or_insert(0i64) += 10 - 3;
        }
        for ((side, t), q) in expect {
            assert_eq!(b.level_qty(side, t as u32), q, "level {side}/{t}");
        }
    }

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
}
