package net.knego.hiperf.smrcollections;

/** One price level: intrusive-FIFO head/tail (pool slot handles) + aggregates. */
public final class Level {
    public int head = Book.NIL;
    public int tail = Book.NIL;
    public long qtyTotal;
    public int count;
}
