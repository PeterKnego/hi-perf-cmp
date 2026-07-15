//! SBE snapshot codec for the book: encode into a preallocated buffer + trailing
//! crc32c; restore into a fresh book (rebuilding only the id-map).

use crate::book::{Book, NIL, Order, PriceLevel};
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

/// Encode the whole book into `buf`; returns SBE length + 4 (crc32c trailer).
pub fn encode(book: &Book, buf: &mut [u8]) -> usize {
    // Count occupied levels (bids then asks) for the group header.
    let mut level_count = 0u16;
    for lane in [&book.bids, &book.asks] {
        for lvl in lane {
            if lvl.head != NIL {
                level_count += 1;
            }
        }
    }
    let sbe_len = {
        let enc = BookSnapshotEncoder::default().wrap(WriteBuf::new(buf), HEADER_LEN);
        let mut header = enc.header(0);
        let mut enc = header.parent().expect("header parent");
        enc.price_min(book.price_min);
        enc.tick_size(book.tick);
        // NOTE: the generated accessor for the `nLevels` field is `nl_evels`,
        // not `n_levels` (an artifact of the SBE Rust codegen's snake_case
        // splitting of "nLevels" -> "nl_evels"). The in-memory `Book.n_levels`
        // field keeps its normal name; only the SBE accessor is spelled oddly.
        enc.nl_evels(book.n_levels);
        enc.capacity(book.pool.len() as u32);
        enc.hwm(book.hwm);
        enc.best_bid(book.best_bid);
        enc.best_ask(book.best_ask);

        let mut lg = enc.levels_encoder(level_count, LevelsEncoder::default());
        for (side, lane) in [(0u8, &book.bids), (1u8, &book.asks)] {
            for (t, lvl) in lane.iter().enumerate() {
                if lvl.head == NIL {
                    continue;
                }
                lg.advance().expect("levels advance");
                lg.side(side_enum(side));
                lg.level_tick(t as u32);
                lg.qty_total(lvl.qty_total);
                lg.order_count(lvl.count); // SBE field is `orderCount` (see Appendix B); struct field stays `count`
                lg.head(lvl.head);
                lg.tail(lvl.tail);
            }
        }
        let enc = lg.parent().expect("levels parent");

        let mut og = enc.orders_encoder(book.hwm as u16, OrdersEncoder::default());
        for slot in 0..book.hwm as usize {
            let o = &book.pool[slot];
            og.advance().expect("orders advance");
            og.slot(slot as u32);
            og.order_id(o.order_id);
            og.price(o.price);
            og.qty(o.qty);
            og.filled(o.filled);
            og.side(side_enum(o.side));
            og.next_slot(o.next); // SBE field is `nextSlot` (Java Iterator.next() collision); struct field stays `next`
            og.prev(o.prev);
        }
        og.get_limit()
    };
    let crc = crc32c::crc32c(&buf[..sbe_len]);
    buf[sbe_len..sbe_len + 4].copy_from_slice(&crc.to_le_bytes());
    sbe_len + 4
}

/// Restore a fresh book from an encoded image; verifies the crc32c trailer.
pub fn restore(bytes: &[u8], cfg: &SmrConfig) -> Result<Book, String> {
    if bytes.len() < 4 {
        return Err("snapshot too short".into());
    }
    let sbe_len = bytes.len() - 4;
    let want = u32::from_le_bytes(bytes[sbe_len..].try_into().unwrap());
    if crc32c::crc32c(&bytes[..sbe_len]) != want {
        return Err("crc32c mismatch".into());
    }
    let mut book = Book::new(cfg);
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
        let t = lg.level_tick() as usize;
        let lvl = PriceLevel {
            head: lg.head(),
            tail: lg.tail(),
            qty_total: lg.qty_total(),
            count: lg.order_count(),
        };
        if side == 0 {
            book.bids[t] = lvl
        } else {
            book.asks[t] = lvl
        }
    }
    let dec = lg.parent().expect("levels parent");

    let mut og = dec.orders_decoder();
    let oc = og.count();
    for _ in 0..oc {
        og.advance().expect("advance").expect("order present");
        let slot = og.slot() as usize;
        let o = Order {
            order_id: og.order_id(),
            price: og.price(),
            qty: og.qty(),
            filled: og.filled(),
            next: og.next_slot(),
            prev: og.prev(),
            side: side_u8(og.side()),
        };
        book.pool[slot] = o;
        book.idmap.insert(o.order_id, slot as u32);
    }
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::workload::next_insert;
    use crate::rng::{SEED, SplitMix};

    fn build(cfg: &SmrConfig, n: usize) -> Book {
        let mut b = Book::new(cfg);
        let mut rng = SplitMix::new(SEED);
        for i in 0..n {
            let ins = next_insert(&mut rng, i, cfg.levels, cfg.tick, cfg.price_min);
            b.insert(ins.order_id, ins.price, ins.qty, ins.side);
        }
        b
    }

    fn cfg() -> SmrConfig {
        SmrConfig {
            cap: 4096,
            levels: 64,
            tick: 1,
            price_min: 0,
            steady: 2000,
            warmup: 0,
            iters: 0,
        }
    }

    #[test]
    fn round_trip_preserves_queries() {
        let c = cfg();
        let b = build(&c, c.steady);
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&b, &mut buf);
        let r = restore(&buf[..n], &c).expect("restore");
        assert_eq!(r.best_bid(), b.best_bid());
        assert_eq!(r.best_ask(), b.best_ask());
        assert_eq!(r.hwm(), b.hwm());
        for id in 1..=c.steady as i64 {
            assert_eq!(r.get_slot(id), b.get_slot(id));
        }
        for t in 0..c.levels {
            assert_eq!(r.level_qty(0, t), b.level_qty(0, t));
            assert_eq!(r.level_qty(1, t), b.level_qty(1, t));
        }
    }

    #[test]
    fn snapshot_is_deterministic_and_restore_stable() {
        let c = cfg();
        let mut buf1 = vec![0u8; 4 * 1024 * 1024];
        let mut buf2 = vec![0u8; 4 * 1024 * 1024];
        let n1 = encode(&build(&c, c.steady), &mut buf1);
        let n2 = encode(&build(&c, c.steady), &mut buf2);
        assert_eq!(&buf1[..n1], &buf2[..n2], "same ops => identical bytes");
        // re-snapshot after restore is identical
        let r = restore(&buf1[..n1], &c).expect("restore");
        let mut buf3 = vec![0u8; 4 * 1024 * 1024];
        let n3 = encode(&r, &mut buf3);
        assert_eq!(
            &buf1[..n1],
            &buf3[..n3],
            "restore round-trips to identical bytes"
        );
    }

    #[test]
    fn corrupt_crc_is_rejected() {
        let c = cfg();
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&build(&c, c.steady), &mut buf);
        buf[0] ^= 0xFF; // corrupt a byte
        assert!(restore(&buf[..n], &c).is_err());
    }

    #[test]
    fn export_golden_when_requested() {
        if std::env::var("SMRC_WRITE_GOLDEN").is_err() {
            return;
        }
        let c = SmrConfig {
            cap: 4096,
            levels: 64,
            tick: 1,
            price_min: 0,
            steady: 2000,
            warmup: 0,
            iters: 0,
        };
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        let n = encode(&build(&c, c.steady), &mut buf);
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../testdata/golden_snapshot.bin"
            ),
            &buf[..n],
        )
        .expect("write golden");
    }
}
