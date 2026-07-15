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
        int slot = hwm++;
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
            ids.put(pool[slot].orderId, pool[slot]);
        }
    }
}
