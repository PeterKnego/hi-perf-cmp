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
    r#gen: u64,
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
            r#gen: 1,
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
        if self.order_chunks[ci].born < self.r#gen {
            self.order_chunks[ci] = Arc::new(OrderChunk {
                born: self.r#gen,
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
        let r#gen = self.r#gen;
        let lane = if side == 0 {
            &mut self.bid_chunks
        } else {
            &mut self.ask_chunks
        };
        let ci = t as usize / LEVEL_CHUNK;
        if lane[ci].born < r#gen {
            lane[ci] = Arc::new(LevelChunk {
                born: r#gen,
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
        self.r#gen += 1;
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
            apply_batch: 64,
            multi_table: false,
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
                (
                    co.order_id,
                    co.price,
                    co.qty,
                    co.filled,
                    co.next,
                    co.prev,
                    co.side
                ),
                (
                    bo.order_id,
                    bo.price,
                    bo.qty,
                    bo.filled,
                    bo.next,
                    bo.prev,
                    bo.side
                )
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
            cb.insert(
                i as i64 + 1,
                (i % c.levels as usize) as i64,
                10,
                (i % 2) as u8,
            );
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
