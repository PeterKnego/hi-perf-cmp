package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;

class HandoffTest {

    @Test
    void measureRunsWarmupPlusIterationsAndReturnsIterationsSamples() {
        HandoffConfig cfg = new HandoffConfig(3, 5, 16);
        AtomicInteger calls = new AtomicInteger();
        long[] samples = Handoff.measure(cfg, calls::incrementAndGet);
        assertEquals(5, samples.length, "one sample per measured iteration");
        assertEquals(8, calls.get(), "warmup (3) + iterations (5) calls");
    }
}
