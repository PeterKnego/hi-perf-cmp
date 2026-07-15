package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;

class SmrCollectionsTest {
    @Test
    void measureRunsWarmupPlusIters() {
        AtomicInteger calls = new AtomicInteger();
        long[] s = SmrCollections.measure(3, 5, calls::incrementAndGet);
        assertEquals(5, s.length);
        assertEquals(8, calls.get());
    }
}
