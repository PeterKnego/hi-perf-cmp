package net.knego.hiperf.smrcollections;

import java.util.Arrays;
import net.knego.hiperf.common.SmrConfig;
import org.agrona.collections.Long2LongHashMap;

/**
 * Chunked copy-on-write LOB: same logical behavior as {@link Book}, but the
 * pool and ladder live in fixed-size structure-of-arrays chunks (parallel
 * primitive arrays, so a chunk copy is arraycopy — never per-object cloning).
 * {@link #capture()} clones the chunk-ref arrays (O(#chunks)) and bumps the
 * generation; the writer copies a chunk before its first write after a capture
 * ({@code born < gen}), so a frozen {@link CowRoot} is never mutated. GC
 * reclaims dropped chunks. The copy decision is ALWAYS the epoch.
 */
public final class CowBook implements Churn.Store {
    public static final int LEVEL_CHUNK = 256;

    static final class OrderChunk {
        long born;
        final long[] orderId, price, qty, filled;
        final int[] next, prev;
        final byte[] side;

        OrderChunk(long born, int n) {
            this.born = born;
            orderId = new long[n];
            price = new long[n];
            qty = new long[n];
            filled = new long[n];
            next = new int[n];
            prev = new int[n];
            side = new byte[n];
        }

        OrderChunk copyFor(long gen) {
            int n = orderId.length;
            OrderChunk c = new OrderChunk(gen, n);
            System.arraycopy(orderId, 0, c.orderId, 0, n);
            System.arraycopy(price, 0, c.price, 0, n);
            System.arraycopy(qty, 0, c.qty, 0, n);
            System.arraycopy(filled, 0, c.filled, 0, n);
            System.arraycopy(next, 0, c.next, 0, n);
            System.arraycopy(prev, 0, c.prev, 0, n);
            System.arraycopy(side, 0, c.side, 0, n);
            return c;
        }
    }

    static final class LvlChunk {
        long born;
        final long[] qtyTotal;
        final int[] head, tail, count;

        LvlChunk(long born, int n) {
            this.born = born;
            qtyTotal = new long[n];
            head = new int[n];
            tail = new int[n];
            count = new int[n];
            Arrays.fill(head, Book.NIL);
            Arrays.fill(tail, Book.NIL);
        }

        LvlChunk copyFor(long gen) {
            int n = qtyTotal.length;
            LvlChunk c = new LvlChunk(gen, n);
            System.arraycopy(qtyTotal, 0, c.qtyTotal, 0, n);
            System.arraycopy(head, 0, c.head, 0, n);
            System.arraycopy(tail, 0, c.tail, 0, n);
            System.arraycopy(count, 0, c.count, 0, n);
            return c;
        }
    }

    public final long priceMin;
    public final long tick;
    public final int nLevels;
    public final int chunk;
    public final int capacity;
    private long gen = 1;
    final OrderChunk[] orderChunks;
    final LvlChunk[] bidChunks;
    final LvlChunk[] askChunks;
    public int hwm;
    public int bestBid = -1;
    public int bestAsk = -1;

    /**
     * Head of the intrusive LIFO free list (NIL when empty). Same semantics as
     * {@link Book#freeHead}: freed slots chain through their own {@code next} field.
     */
    public int freeHead = Book.NIL;

    private final Long2LongHashMap ids = new Long2LongHashMap(Book.NIL);

    public CowBook(SmrConfig cfg) {
        this.priceMin = cfg.priceMin();
        this.tick = cfg.tick();
        this.nLevels = cfg.levels();
        this.chunk = cfg.chunk();
        this.capacity = cfg.cap();
        int nOC = (capacity + chunk - 1) / chunk;
        orderChunks = new OrderChunk[nOC];
        for (int ci = 0; ci < nOC; ci++) {
            orderChunks[ci] = new OrderChunk(1, Math.min(chunk, capacity - ci * chunk));
        }
        int nLC = (nLevels + LEVEL_CHUNK - 1) / LEVEL_CHUNK;
        bidChunks = new LvlChunk[nLC];
        askChunks = new LvlChunk[nLC];
        for (int ci = 0; ci < nLC; ci++) {
            int n = Math.min(LEVEL_CHUNK, nLevels - ci * LEVEL_CHUNK);
            bidChunks[ci] = new LvlChunk(1, n);
            askChunks[ci] = new LvlChunk(1, n);
        }
    }

    private int tickOf(long price) {
        return (int) ((price - priceMin) / tick);
    }

    private LvlChunk[] lane(byte side) {
        return side == 0 ? bidChunks : askChunks;
    }

    /** Read-only chunk lookup: never copies, so a rescan cannot trigger copy-on-write. */
    private OrderChunk orderChunkForRead(int slot) {
        return orderChunks[slot / chunk];
    }

    /** Read-only chunk lookup: never copies, so a rescan cannot trigger copy-on-write. */
    private LvlChunk lvlChunkForRead(byte side, int t) {
        return lane(side)[t / LEVEL_CHUNK];
    }

    private OrderChunk orderChunkForWrite(int slot) {
        int ci = slot / chunk;
        OrderChunk c = orderChunks[ci];
        if (c.born < gen) {
            c = c.copyFor(gen);
            orderChunks[ci] = c;
        }
        return c;
    }

    private LvlChunk lvlChunkForWrite(byte side, int t) {
        LvlChunk[] lane = lane(side);
        int ci = t / LEVEL_CHUNK;
        LvlChunk c = lane[ci];
        if (c.born < gen) {
            c = c.copyFor(gen);
            lane[ci] = c;
        }
        return c;
    }

    /** Same op semantics as {@link Book#insert} (keep in lockstep). */
    public void insert(long orderId, long price, long qty, byte side) {
        int t = tickOf(price);
        int slot = allocSlot();
        LvlChunk lc = lvlChunkForWrite(side, t);
        int lo = t % LEVEL_CHUNK;
        int prevTail = lc.tail[lo];
        OrderChunk oc = orderChunkForWrite(slot);
        int oo = slot % chunk;
        oc.orderId[oo] = orderId;
        oc.price[oo] = price;
        oc.qty[oo] = qty;
        oc.filled[oo] = 0;
        oc.side[oo] = side;
        oc.next[oo] = Book.NIL;
        oc.prev[oo] = prevTail;
        if (prevTail != Book.NIL) {
            OrderChunk pc = orderChunkForWrite(prevTail);
            pc.next[prevTail % chunk] = slot;
        } else {
            lc.head[lo] = slot;
        }
        lc.tail[lo] = slot;
        lc.qtyTotal[lo] += qty;
        lc.count[lo]++;
        ids.put(orderId, slot);
        if (side == 0 && (bestBid < 0 || t > bestBid)) {
            bestBid = t;
        }
        if (side == 1 && (bestAsk < 0 || t < bestAsk)) {
            bestAsk = t;
        }
    }

    /** Same op semantics as {@link Book#update} (keep in lockstep). */
    public void update(long orderId, long fillQty) {
        int slot = (int) ids.get(orderId);
        OrderChunk oc = orderChunkForWrite(slot);
        int oo = slot % chunk;
        long add = Math.min(fillQty, oc.qty[oo] - oc.filled[oo]);
        oc.filled[oo] += add;
        int t = tickOf(oc.price[oo]);
        LvlChunk lc = lvlChunkForWrite(oc.side[oo], t);
        lc.qtyTotal[t % LEVEL_CHUNK] -= add;
    }

    /** Same op semantics as {@link Book#allocSlot} (keep in lockstep). */
    private int allocSlot() {
        if (freeHead != Book.NIL) {
            int slot = freeHead;
            freeHead = orderChunkForRead(slot).next[slot % chunk];
            return slot;
        }
        if (hwm == capacity) {
            throw new IllegalStateException("order pool exhausted: SMRC_CAP=" + capacity + " reached");
        }
        return hwm++;
    }

    /** Same op semantics as {@link Book#freeSlot} (keep in lockstep). */
    private void freeSlot(int slot) {
        OrderChunk oc = orderChunkForWrite(slot);
        int oo = slot % chunk;
        oc.orderId[oo] = 0; // freed marker: the snapshot walk skips these
        oc.next[oo] = freeHead;
        oc.prev[oo] = Book.NIL;
        freeHead = slot;
    }

    /** Same op semantics as {@link Book#unlink} (keep in lockstep). */
    private void unlink(int slot, byte side, int t, long rem) {
        OrderChunk ocRead = orderChunkForRead(slot);
        int prev = ocRead.prev[slot % chunk];
        int next = ocRead.next[slot % chunk];
        if (prev != Book.NIL) {
            OrderChunk pc = orderChunkForWrite(prev);
            pc.next[prev % chunk] = next;
        }
        if (next != Book.NIL) {
            OrderChunk nc = orderChunkForWrite(next);
            nc.prev[next % chunk] = prev;
        }
        LvlChunk lc = lvlChunkForWrite(side, t);
        int lo = t % LEVEL_CHUNK;
        if (lc.head[lo] == slot) {
            lc.head[lo] = next;
        }
        if (lc.tail[lo] == slot) {
            lc.tail[lo] = prev;
        }
        lc.qtyTotal[lo] -= rem;
        lc.count[lo]--;
    }

    /**
     * Same op semantics as {@link Book#repairBest} (keep in lockstep). Reads through the
     * read-only chunk accessor — a rescan must not trigger copy-on-write of untouched chunks.
     */
    private void repairBest(byte side, int t) {
        if (side == 0) {
            if (bestBid != t || lvlChunkForRead((byte) 0, t).head[t % LEVEL_CHUNK] != Book.NIL) {
                return;
            }
            int nb = -1;
            for (int i = t; i >= 0; i--) {
                if (lvlChunkForRead((byte) 0, i).head[i % LEVEL_CHUNK] != Book.NIL) {
                    nb = i;
                    break;
                }
            }
            bestBid = nb;
            return;
        }
        if (bestAsk != t || lvlChunkForRead((byte) 1, t).head[t % LEVEL_CHUNK] != Book.NIL) {
            return;
        }
        int na = -1;
        for (int i = t; i < nLevels; i++) {
            if (lvlChunkForRead((byte) 1, i).head[i % LEVEL_CHUNK] != Book.NIL) {
                na = i;
                break;
            }
        }
        bestAsk = na;
    }

    /** Same op semantics as {@link Book#cancel} (keep in lockstep). */
    public void cancel(long orderId) {
        int slot = (int) ids.remove(orderId);
        OrderChunk oc = orderChunkForRead(slot);
        int oo = slot % chunk;
        long rem = oc.qty[oo] - oc.filled[oo];
        byte side = oc.side[oo];
        int t = tickOf(oc.price[oo]);
        unlink(slot, side, t, rem);
        freeSlot(slot);
        repairBest(side, t);
    }

    /** Same op semantics as {@link Book#fill} (keep in lockstep). */
    public void fill(long orderId) {
        int slot = (int) ids.remove(orderId);
        OrderChunk oc = orderChunkForWrite(slot);
        int oo = slot % chunk;
        long rem = oc.qty[oo] - oc.filled[oo];
        oc.filled[oo] = oc.qty[oo];
        byte side = oc.side[oo];
        int t = tickOf(oc.price[oo]);
        unlink(slot, side, t, rem);
        freeSlot(slot);
        repairBest(side, t);
    }

    /** Freeze the current state (O(#chunks)) and bump the generation. */
    public CowRoot capture() {
        CowRoot r = new CowRoot(priceMin, tick, nLevels, capacity, hwm, bestBid, bestAsk, freeHead,
                chunk, orderChunks.clone(), bidChunks.clone(), askChunks.clone());
        gen++;
        return r;
    }

    public int getSlot(long orderId) {
        return (int) ids.get(orderId);
    }

    public long levelQty(byte side, int t) {
        return lane(side)[t / LEVEL_CHUNK].qtyTotal[t % LEVEL_CHUNK];
    }

    public long orderFilled(int slot) {
        return orderChunks[slot / chunk].filled[slot % chunk];
    }

    /** Re-index the id-map from the pool (used after restore). */
    void rebuildIds() {
        ids.clear();
        for (int slot = 0; slot < hwm; slot++) {
            OrderChunk oc = orderChunks[slot / chunk];
            long orderId = oc.orderId[slot % chunk];
            if (orderId != 0) {
                ids.put(orderId, slot);
            }
        }
    }
}
