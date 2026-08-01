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

#[cfg(test)]
mod tests {
    use super::{digest_book, digest_root};
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
        d.cancel(9); // frees slot 0, then reused below
        d.insert(1, 5, 10, 0); // reuses slot 0
        d.insert(3, 5, 20, 0);

        // Both books end with free_head=NIL after operations complete, but they
        // reached that state via different paths. The digest should match because
        // logical content is identical, regardless of allocation history.
        assert_eq!(digest_book(&a), digest_book(&d));
    }
}
