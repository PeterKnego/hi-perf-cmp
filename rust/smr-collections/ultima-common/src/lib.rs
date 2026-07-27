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
// Re-exported so cell binaries can name the pin type without a direct
// ultima_db dependency.
pub use ultima_db::VersionPin;

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
                // Retention stays at the default (10). Keeping a captured
                // version alive for the live_ultima serializer is done with
                // `pin_current()` (a `Send` `VersionPin` travels with the
                // handoff), not a retention window — a large window made
                // every commit pay an O(retained) auto-GC scan on the
                // previously pinned ultima_db rev.
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

        let mut og = enc.orders_encoder(m.hwm as u16, OrdersEncoder::default());
        // Order ids are sequential from 1 in insertion order: id order IS
        // slot order 0..hwm. Same ascending-id guarantee as the levels walk
        // above (ultima_db's `Table::iter()` over its id-keyed B-tree),
        // checked the same way.
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
}
