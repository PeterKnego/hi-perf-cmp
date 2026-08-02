package net.knego.hiperf.smrcollections;

import net.knego.hiperf.common.SmrConfig;
import org.agrona.collections.Long2ObjectHashMap;

/** Fixed-capacity limit order book: flat ladder + pooled orders + Agrona id-map. */
public final class Book {
    /** Sentinel handle (empty head/tail, link end). As unsigned = 0xFFFFFFFF. */
    public static final int NIL = -1;

    public final long priceMin;
    public final long tick;
    public int nLevels;
    public final Level[] bids;
    public final Level[] asks;
    public final Order[] pool;
    public int hwm;
    public int bestBid = -1;
    public int bestAsk = -1;
    public final Long2ObjectHashMap<Order> ids;

    /**
     * Head of the intrusive LIFO free list (NIL when empty). Freed slots chain through their own
     * `next` field. This is state a snapshot must capture — restore reproduces allocation order
     * from it.
     */
    public int freeHead = NIL;

    public Book(SmrConfig cfg) {
        this.priceMin = cfg.priceMin();
        this.tick = cfg.tick();
        this.nLevels = cfg.levels();
        this.bids = new Level[cfg.levels()];
        this.asks = new Level[cfg.levels()];
        for (int i = 0; i < cfg.levels(); i++) {
            bids[i] = new Level();
            asks[i] = new Level();
        }
        this.pool = new Order[cfg.cap()];
        for (int i = 0; i < cfg.cap(); i++) {
            pool[i] = new Order();
        }
        this.ids = new Long2ObjectHashMap<>(cfg.cap() * 2, 0.5f);
    }

    private int tickOf(long price) {
        return (int) ((price - priceMin) / tick);
    }

    private Level[] lane(byte side) {
        return side == 0 ? bids : asks;
    }

    public void insert(long orderId, long price, long qty, byte side) {
        int t = tickOf(price);
        int slot = allocSlot();
        Level lvl = lane(side)[t];
        Order o = pool[slot];
        o.orderId = orderId;
        o.price = price;
        o.qty = qty;
        o.filled = 0;
        o.side = side;
        o.slot = slot;
        o.next = NIL;
        o.prev = lvl.tail;
        if (lvl.tail != NIL) {
            pool[lvl.tail].next = slot;
        } else {
            lvl.head = slot;
        }
        lvl.tail = slot;
        lvl.qtyTotal += qty;
        lvl.count++;
        ids.put(orderId, o);
        if (side == 0 && (bestBid < 0 || t > bestBid)) {
            bestBid = t;
        }
        if (side == 1 && (bestAsk < 0 || t < bestAsk)) {
            bestAsk = t;
        }
    }

    private int allocSlot() {
        if (freeHead != NIL) {
            int slot = freeHead;
            freeHead = pool[slot].next;
            return slot;
        }
        if (hwm == pool.length) {
            throw new IllegalStateException("order pool exhausted: SMRC_CAP=" + pool.length + " reached");
        }
        return hwm++;
    }

    private void freeSlot(int slot) {
        Order o = pool[slot];
        o.orderId = 0; // freed marker: the snapshot walk skips these
        o.next = freeHead;
        o.prev = NIL;
        freeHead = slot;
    }

    /** Unlink slot from its level's intrusive FIFO and debit rem from the level's remaining qty. */
    private void unlink(int slot, byte side, int t, long rem) {
        Order o = pool[slot];
        int prev = o.prev;
        int next = o.next;
        if (prev != NIL) {
            pool[prev].next = next;
        }
        if (next != NIL) {
            pool[next].prev = prev;
        }
        Level lvl = lane(side)[t];
        if (lvl.head == slot) {
            lvl.head = next;
        }
        if (lvl.tail == slot) {
            lvl.tail = prev;
        }
        lvl.qtyTotal -= rem;
        lvl.count--;
    }

    /**
     * Restore the cached best for side after a removal emptied level t. O(levels) worst case and
     * deliberately on the timed path — real books maintain this, and hiding it would hide the
     * worst-case cancel.
     */
    private void repairBest(byte side, int t) {
        if (side == 0) {
            if (bestBid != t || bids[t].head != NIL) {
                return;
            }
            int nb = -1;
            for (int i = t; i >= 0; i--) {
                if (bids[i].head != NIL) {
                    nb = i;
                    break;
                }
            }
            bestBid = nb;
            return;
        }
        if (bestAsk != t || asks[t].head != NIL) {
            return;
        }
        int na = -1;
        for (int i = t; i < nLevels; i++) {
            if (asks[i].head != NIL) {
                na = i;
                break;
            }
        }
        bestAsk = na;
    }

    /** Remove a resting order; its remaining quantity leaves the level. */
    public void cancel(long orderId) {
        Order o = ids.remove(orderId);
        long rem = o.qty - o.filled;
        byte side = o.side;
        int t = tickOf(o.price);
        int slot = o.slot;
        unlink(slot, side, t, rem);
        freeSlot(slot);
        repairBest(side, t);
    }

    /**
     * Fill an order to completion, then remove it. Same structural work as cancel; the difference
     * is that the departing quantity is booked as filled rather than withdrawn.
     */
    public void fill(long orderId) {
        Order o = ids.remove(orderId);
        long rem = o.qty - o.filled;
        o.filled = o.qty;
        byte side = o.side;
        int t = tickOf(o.price);
        int slot = o.slot;
        unlink(slot, side, t, rem);
        freeSlot(slot);
        repairBest(side, t);
    }

    public void update(long orderId, long fillQty) {
        Order o = ids.get(orderId);
        long add = Math.min(fillQty, o.qty - o.filled);
        o.filled += add;
        lane(o.side)[tickOf(o.price)].qtyTotal -= add;
    }

    public int getSlot(long orderId) {
        return ids.get(orderId).slot;
    }

    public int bestBid() {
        return bestBid;
    }

    public int bestAsk() {
        return bestAsk;
    }

    public int hwm() {
        return hwm;
    }

    public long levelQty(byte side, int tick) {
        return lane(side)[tick].qtyTotal;
    }

    /** Re-index the id-map from the pool (used after restore). */
    public void rebuildIds() {
        ids.clear();
        for (int slot = 0; slot < hwm; slot++) {
            if (pool[slot].orderId != 0) {
                ids.put(pool[slot].orderId, pool[slot]);
            }
        }
    }
}
