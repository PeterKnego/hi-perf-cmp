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
    const EMPTY: PriceLevel = PriceLevel {
        head: NIL,
        tail: NIL,
        qty_total: 0,
        count: 0,
    };
}

/// A no-op hasher for `u64` keys (fixed hashing — no SipHash/RandomState).
#[derive(Default)]
pub struct NoHash(u64);
impl Hasher for NoHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("only write_u64 is used")
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
    fn write_i64(&mut self, v: i64) {
        self.0 = v as u64;
    }
}
type IdMap = HashMap<i64, u32, BuildHasherDefault<NoHash>>;

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
        let slot = self.hwm;
        self.hwm += 1;
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
            b.insert(i + 1, (i % 16) as i64, 10, (i % 2) as u8);
        }
        for i in 0..50i64 {
            b.update(i + 1, 3);
        }
        // recompute expected per level
        let mut expect = std::collections::HashMap::new();
        for i in 0..50i64 {
            let t = (i % 16) as i64;
            let side = (i % 2) as u8;
            *expect.entry((side, t)).or_insert(0i64) += 10 - 3;
        }
        for ((side, t), q) in expect {
            assert_eq!(b.level_qty(side, t as u32), q, "level {side}/{t}");
        }
    }
}
