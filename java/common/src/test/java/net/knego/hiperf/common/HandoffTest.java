package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;

class HandoffTest {

    @Test
    void measureRunsWarmupPlusIterationsAndReturnsIterationsSamples() {
        HandoffConfig cfg = new HandoffConfig(3, 5, 16, 0);
        AtomicInteger calls = new AtomicInteger();
        long[] samples = Handoff.measure(cfg, calls::incrementAndGet);
        assertEquals(5, samples.length, "one sample per measured iteration");
        assertEquals(8, calls.get(), "warmup (3) + iterations (5) calls");
    }

    @Test
    void measurePacedKeepsTheGapBetweenRoundTripsAndOutsideSamples() {
        long gapNs = 1_000_000; // 1 ms: far above clock granularity
        HandoffConfig cfg = new HandoffConfig(1, 4, 16, gapNs);
        List<Long> starts = new ArrayList<>();
        long[] samples = Handoff.measurePaced(cfg, () -> starts.add(System.nanoTime()));
        assertEquals(4, samples.length);
        assertEquals(5, starts.size(), "warmup (1) + iterations (4) calls");
        for (int i = 1; i < starts.size(); i++) {
            long d = starts.get(i) - starts.get(i - 1);
            assertTrue(d >= gapNs, "round trips only " + d + "ns apart, want >= " + gapNs);
        }
        // The gap busy-wait sits OUTSIDE the timed window: each sample is the
        // near-instant round trip, never the millisecond gap.
        for (long s : samples) {
            assertTrue(s < gapNs, "sample " + s + "ns includes the gap");
        }
    }
}
