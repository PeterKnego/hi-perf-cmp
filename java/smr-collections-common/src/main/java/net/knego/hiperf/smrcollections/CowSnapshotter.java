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

/** SBE codec over a frozen CowRoot; byte-identical to {@link Snapshotter}. */
public final class CowSnapshotter {

    private final byte[] backing;
    private final UnsafeBuffer buffer;
    private final MessageHeaderEncoder headerEnc = new MessageHeaderEncoder();
    private final BookSnapshotEncoder enc = new BookSnapshotEncoder();
    private int lastLen;

    public CowSnapshotter(int maxBytes) {
        this.backing = new byte[maxBytes];
        this.buffer = new UnsafeBuffer(backing);
    }

    private static long u32(int v) {
        return v & 0xFFFFFFFFL;
    }

    /** Encode the frozen root; returns total length (SBE + crc32c). */
    public int encodeRoot(CowRoot r) {
        enc.wrapAndApplyHeader(buffer, 0, headerEnc);
        enc.priceMin(r.priceMin);
        enc.tickSize(r.tick);
        enc.nLevels(u32(r.nLevels));
        enc.capacity(u32(r.capacity));
        enc.hwm(u32(r.hwm));
        enc.bestBid(r.bestBid);
        enc.bestAsk(r.bestAsk);
        enc.freeHead(u32(r.freeHead));

        int levelCount = 0;
        for (byte side = 0; side < 2; side++) {
            for (int t = 0; t < r.nLevels; t++) {
                if (r.lvl(side, t).head[t % CowBook.LEVEL_CHUNK] != Book.NIL) {
                    levelCount++;
                }
            }
        }
        BookSnapshotEncoder.LevelsEncoder lg = enc.levelsCount(levelCount);
        for (byte side = 0; side < 2; side++) {
            for (int t = 0; t < r.nLevels; t++) {
                CowBook.LvlChunk c = r.lvl(side, t);
                int lo = t % CowBook.LEVEL_CHUNK;
                if (c.head[lo] == Book.NIL) {
                    continue;
                }
                lg.next();
                lg.side(side == 0 ? Side.BID : Side.ASK);
                lg.levelTick(u32(t));
                lg.qtyTotal(c.qtyTotal[lo]);
                lg.orderCount(u32(c.count[lo]));
                lg.head(u32(c.head[lo]));
                lg.tail(u32(c.tail[lo]));
            }
        }

        BookSnapshotEncoder.OrdersEncoder og = enc.ordersCount(r.hwm);
        for (int slot = 0; slot < r.hwm; slot++) {
            CowBook.OrderChunk c = r.ord(slot);
            int oo = slot % r.chunk;
            og.next();
            og.slot(u32(slot));
            og.orderId(c.orderId[oo]);
            og.price(c.price[oo]);
            og.qty(c.qty[oo]);
            og.filled(c.filled[oo]);
            og.side(c.side[oo] == 0 ? Side.BID : Side.ASK);
            og.nextSlot(u32(c.next[oo]));
            og.prev(u32(c.prev[oo]));
        }

        int sbeLen = enc.limit();
        CRC32C crc = new CRC32C();
        crc.update(backing, 0, sbeLen);
        buffer.putInt(sbeLen, (int) crc.getValue(), ByteOrder.LITTLE_ENDIAN);
        lastLen = sbeLen + 4;
        return lastLen;
    }

    public byte[] backing() {
        return backing;
    }

    public int lastLen() {
        return lastLen;
    }

    /** Restore a fresh CowBook, verifying the crc32c trailer. */
    public static CowBook restoreCow(byte[] data, int len, SmrConfig cfg) {
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
        if (header.version() != BookSnapshotEncoder.SCHEMA_VERSION) {
            throw new IllegalArgumentException("unsupported snapshot schema version "
                    + header.version() + " (expected " + BookSnapshotEncoder.SCHEMA_VERSION + ")");
        }
        BookSnapshotDecoder dec = new BookSnapshotDecoder();
        dec.wrap(buf, header.encodedLength(), header.blockLength(), header.version());

        CowBook b = new CowBook(cfg);
        // priceMin/tick/nLevels are final (from cfg); wire values equal cfg by
        // construction, as in Snapshotter.restore.
        b.hwm = (int) dec.hwm();
        b.bestBid = dec.bestBid();
        b.bestAsk = dec.bestAsk();
        if ((int) dec.capacity() != cfg.cap()) {
            throw new IllegalArgumentException(
                    "snapshot capacity " + dec.capacity() + " != SMRC_CAP " + cfg.cap());
        }
        b.freeHead = (int) dec.freeHead();

        BookSnapshotDecoder.LevelsDecoder levels = dec.levels();
        while (levels.hasNext()) {
            levels.next();
            byte side = (byte) (levels.side() == Side.ASK ? 1 : 0);
            int t = (int) levels.levelTick();
            CowBook.LvlChunk c = (side == 0 ? b.bidChunks : b.askChunks)[t / CowBook.LEVEL_CHUNK];
            int lo = t % CowBook.LEVEL_CHUNK;
            c.qtyTotal[lo] = levels.qtyTotal();
            c.count[lo] = (int) levels.orderCount();
            c.head[lo] = (int) levels.head();
            c.tail[lo] = (int) levels.tail();
        }
        BookSnapshotDecoder.OrdersDecoder orders = dec.orders();
        while (orders.hasNext()) {
            orders.next();
            int slot = (int) orders.slot();
            CowBook.OrderChunk c = b.orderChunks[slot / b.chunk];
            int oo = slot % b.chunk;
            c.orderId[oo] = orders.orderId();
            c.price[oo] = orders.price();
            c.qty[oo] = orders.qty();
            c.filled[oo] = orders.filled();
            c.side[oo] = (byte) (orders.side() == Side.ASK ? 1 : 0);
            c.next[oo] = (int) orders.nextSlot();
            c.prev[oo] = (int) orders.prev();
        }
        b.rebuildIds();
        return b;
    }
}
