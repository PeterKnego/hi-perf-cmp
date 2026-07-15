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

/** SBE snapshot codec: encode into a reused buffer + crc32c; restore a fresh book. */
public final class Snapshotter {

    private final byte[] backing;
    private final UnsafeBuffer buffer;
    private final MessageHeaderEncoder headerEnc = new MessageHeaderEncoder();
    private final BookSnapshotEncoder enc = new BookSnapshotEncoder();
    private int lastLen;

    public Snapshotter(int maxBytes) {
        this.backing = new byte[maxBytes];
        this.buffer = new UnsafeBuffer(backing);
    }

    private static long u32(int v) {
        return v & 0xFFFFFFFFL;
    }

    private static Side sideEnum(byte side) {
        return side == 0 ? Side.BID : Side.ASK;
    }

    private static byte sideByte(Side s) {
        return (byte) (s == Side.ASK ? 1 : 0);
    }

    /** Encode the book into the internal buffer; returns total length (SBE + crc32c). */
    public int encode(Book b) {
        enc.wrapAndApplyHeader(buffer, 0, headerEnc);
        enc.priceMin(b.priceMin);
        enc.tickSize(b.tick);
        enc.nLevels(u32(b.nLevels));
        enc.capacity(u32(b.pool.length));
        enc.hwm(u32(b.hwm));
        enc.bestBid(b.bestBid);
        enc.bestAsk(b.bestAsk);

        int levelCount = 0;
        for (Level[] lane : new Level[][] {b.bids, b.asks}) {
            for (Level lvl : lane) {
                if (lvl.head != Book.NIL) {
                    levelCount++;
                }
            }
        }
        BookSnapshotEncoder.LevelsEncoder lg = enc.levelsCount(levelCount);
        byte[] sides = {0, 1};
        Level[][] lanes = {b.bids, b.asks};
        for (int s = 0; s < 2; s++) {
            Level[] lane = lanes[s];
            for (int t = 0; t < lane.length; t++) {
                Level lvl = lane[t];
                if (lvl.head == Book.NIL) {
                    continue;
                }
                lg.next();
                lg.side(sideEnum(sides[s]));
                lg.levelTick(u32(t));
                lg.qtyTotal(lvl.qtyTotal);
                lg.orderCount(u32(lvl.count)); // SBE field `orderCount` (see Appendix B); struct field stays `count`
                lg.head(u32(lvl.head));
                lg.tail(u32(lvl.tail));
            }
        }

        BookSnapshotEncoder.OrdersEncoder og = enc.ordersCount(b.hwm);
        for (int slot = 0; slot < b.hwm; slot++) {
            Order o = b.pool[slot];
            og.next();
            og.slot(u32(slot));
            og.orderId(o.orderId);
            og.price(o.price);
            og.qty(o.qty);
            og.filled(o.filled);
            og.side(sideEnum(o.side));
            og.nextSlot(u32(o.next)); // SBE field `nextSlot` (Iterator.next() collision); struct field stays `next`
            og.prev(u32(o.prev));
        }

        int sbeLen = enc.limit();
        CRC32C crc = new CRC32C();
        crc.update(backing, 0, sbeLen);
        buffer.putInt(sbeLen, (int) crc.getValue(), ByteOrder.LITTLE_ENDIAN);
        lastLen = sbeLen + 4;
        return lastLen;
    }

    /** View of the last-encoded image (indices 0..returnedLength). */
    public byte[] backing() {
        return backing;
    }

    public int lastLen() {
        return lastLen;
    }

    /** Restore a fresh book, verifying the crc32c trailer. */
    public static Book restore(byte[] data, int len, SmrConfig cfg) {
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

        Book b = new Book(cfg);
        b.nLevels = (int) dec.nLevels();
        b.hwm = (int) dec.hwm();
        b.bestBid = dec.bestBid();
        b.bestAsk = dec.bestAsk();
        // priceMin/tick are final (from cfg); the wire values equal cfg by construction.

        BookSnapshotDecoder.LevelsDecoder levels = dec.levels();
        while (levels.hasNext()) {
            levels.next();
            byte side = sideByte(levels.side());
            int t = (int) levels.levelTick();
            Level lvl = (side == 0 ? b.bids : b.asks)[t];
            lvl.qtyTotal = levels.qtyTotal();
            lvl.count = (int) levels.orderCount();
            lvl.head = (int) levels.head();
            lvl.tail = (int) levels.tail();
        }
        BookSnapshotDecoder.OrdersDecoder orders = dec.orders();
        while (orders.hasNext()) {
            orders.next();
            int slot = (int) orders.slot();
            Order o = b.pool[slot];
            o.slot = slot;
            o.orderId = orders.orderId();
            o.price = orders.price();
            o.qty = orders.qty();
            o.filled = orders.filled();
            o.side = sideByte(orders.side());
            o.next = (int) orders.nextSlot();
            o.prev = (int) orders.prev();
        }
        b.rebuildIds();
        return b;
    }
}
