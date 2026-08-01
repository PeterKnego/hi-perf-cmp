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
        // CowBook has no free list yet (cancel/fill are Book-only, task 2);
        // NIL here mirrors what Book.free_head is for any cancel-free run,
        // keeping byte-parity with snapshot::encode for the same logical state.
        enc.free_head(NIL);

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
        let (head, tail, qty_total, count) =
            (lg.head(), lg.tail(), lg.qty_total(), lg.order_count());
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
            apply_batch: 64,
            multi_table: false,
            live_iters: 200_000,
            snap_every: 20_000,
            otr_bps: 100,
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
