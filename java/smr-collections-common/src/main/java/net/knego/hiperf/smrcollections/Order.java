package net.knego.hiperf.smrcollections;

/** A resting order in the pool. Mutable, pooled, never GC'd steady-state. */
public final class Order {
    public long orderId;
    public long price;
    public long qty;
    public long filled;
    public int next;
    public int prev;
    public int slot;
    public byte side;
}
