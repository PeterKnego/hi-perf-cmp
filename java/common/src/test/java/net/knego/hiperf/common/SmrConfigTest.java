package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class SmrConfigTest {

    private static SmrConfig cfg(int warmup, int iters, int cap) {
        return new SmrConfig(cap, 64, 1L, 0L, 100, warmup, iters, 256, 200_000, 20_000, 100);
    }

    @Test
    void defaultsCarryOnePercentOtr() {
        assertEquals(100, cfg(10, 10, 4096).otrBps(), "default OTR is 1% = 100 bps");
    }

    @Test
    void churnSizedRunFailsBumpCapacityButIsOtherwiseLegal() {
        // warmup+iters > cap is legal for a slot-recycling churn cell and
        // illegal for a bump-allocating insert cell.
        SmrConfig c = cfg(1000, 10_000, 1024);
        assertThrows(IllegalArgumentException.class, c::requireBumpCapacity);
    }

    @Test
    void bumpSizedRunPassesBumpCapacity() {
        assertDoesNotThrow(cfg(10, 100, 4096)::requireBumpCapacity);
    }

    @Test
    void rssBytesIsReadable() {
        org.junit.jupiter.api.Assertions.assertTrue(
                SmrCollections.rssBytes() > 0, "RSS must be readable from /proc/self/statm");
    }
}
