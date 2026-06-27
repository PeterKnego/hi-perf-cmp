package net.knego.hiperf.threadhandoff.ring;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class SpscTest {

    @Test
    void preservesOrderAndCount() throws InterruptedException {
        final int n = 100_000;
        Spsc ring = new Spsc(64);
        long[] got = new long[n];
        Thread consumer = new Thread(() -> {
            for (int i = 0; i < n; i++) {
                got[i] = ring.pop();
            }
        });
        consumer.start();
        for (int i = 0; i < n; i++) {
            ring.push(i);
        }
        consumer.join();
        for (int i = 0; i < n; i++) {
            assertEquals(i, got[i], "token " + i + " out of order");
        }
        assertEquals(n, ring.consumed());
    }
}
