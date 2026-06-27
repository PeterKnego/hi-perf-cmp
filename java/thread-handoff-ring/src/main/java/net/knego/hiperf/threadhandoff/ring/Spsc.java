package net.knego.hiperf.threadhandoff.ring;

import java.util.concurrent.atomic.AtomicLong;

/**
 * Bounded single-producer single-consumer ring of {@code long} tokens with
 * busy-wait (no parking). {@code head}/{@code tail} are monotonic; {@code head}
 * doubles as the consumed count. The {@link AtomicLong} head/tail (release/
 * acquire via volatile get/set) publish the plain-array slot writes.
 */
public final class Spsc {

    private final long[] buf;
    private final int cap;
    private final AtomicLong head = new AtomicLong(0); // total popped (consumer)
    private final AtomicLong tail = new AtomicLong(0); // total pushed (producer)

    public Spsc(int cap) {
        if (cap <= 0) {
            throw new IllegalArgumentException("ring capacity must be positive");
        }
        this.cap = cap;
        this.buf = new long[cap];
    }

    /** Producer: push one token, busy-waiting while the ring is full. */
    public void push(long v) {
        long t = tail.get();
        while (t - head.get() == cap) {
            Thread.onSpinWait();
        }
        buf[(int) (t % cap)] = v;
        tail.set(t + 1);
    }

    /** Consumer: pop one token, busy-waiting while the ring is empty. */
    public long pop() {
        long h = head.get();
        while (h == tail.get()) {
            Thread.onSpinWait();
        }
        long v = buf[(int) (h % cap)];
        head.set(h + 1);
        return v;
    }

    /** Total tokens popped so far (consumer progress). */
    public long consumed() {
        return head.get();
    }
}
