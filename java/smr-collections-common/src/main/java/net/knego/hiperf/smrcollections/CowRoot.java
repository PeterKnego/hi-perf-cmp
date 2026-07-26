package net.knego.hiperf.smrcollections;

/** A frozen point-in-time view: chunk refs + scalars; never mutated after capture. */
public final class CowRoot {
    public final long priceMin;
    public final long tick;
    public final int nLevels;
    public final int capacity;
    public final int hwm;
    public final int bestBid;
    public final int bestAsk;
    final int chunk;
    final CowBook.OrderChunk[] orderChunks;
    final CowBook.LvlChunk[] bidChunks;
    final CowBook.LvlChunk[] askChunks;

    CowRoot(long priceMin, long tick, int nLevels, int capacity, int hwm, int bestBid, int bestAsk,
            int chunk, CowBook.OrderChunk[] orderChunks, CowBook.LvlChunk[] bidChunks, CowBook.LvlChunk[] askChunks) {
        this.priceMin = priceMin;
        this.tick = tick;
        this.nLevels = nLevels;
        this.capacity = capacity;
        this.hwm = hwm;
        this.bestBid = bestBid;
        this.bestAsk = bestAsk;
        this.chunk = chunk;
        this.orderChunks = orderChunks;
        this.bidChunks = bidChunks;
        this.askChunks = askChunks;
    }

    CowBook.LvlChunk lvl(byte side, int t) {
        return (side == 0 ? bidChunks : askChunks)[t / CowBook.LEVEL_CHUNK];
    }

    CowBook.OrderChunk ord(int slot) {
        return orderChunks[slot / chunk];
    }

    public long orderFilled(int slot) {
        return ord(slot).filled[slot % chunk];
    }
}
