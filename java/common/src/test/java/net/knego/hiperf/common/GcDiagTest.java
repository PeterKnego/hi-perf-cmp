package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class GcDiagTest {
    @Test
    void disabledRecordsNothingAndEmitsNoLine() {
        GcDiag d = new GcDiag(false);
        d.record(0, 1_000_000);
        assertNull(d.line("live_mvcc"));
    }

    @Test
    void enabledTracksTheSingleMaxWithItsIterationAndWallClock() {
        GcDiag d = new GcDiag(true);
        long before = System.currentTimeMillis();
        d.record(0, 100);
        d.record(1, 5_000);
        d.record(2, 300); // not a new max: must not displace iter 1
        long after = System.currentTimeMillis();
        String line = d.line("live_mvcc");
        assertTrue(line.startsWith("gc-diag live_mvcc "), line);
        assertTrue(line.contains("writer_max_ns=5000"), line);
        assertTrue(line.contains("iter=1"), line);
        long epoch = Long.parseLong(line.replaceAll(".*at_epoch_ms=(\\d+).*", "$1"));
        assertTrue(epoch >= before && epoch <= after, "wall clock outside the record window");
    }

    @Test
    void enabledButNeverRecordedEmitsNoLine() {
        assertNull(new GcDiag(true).line("live_stw"));
    }

    @Test
    void equalDurationDoesNotDisplaceTheEarlierMax() {
        GcDiag d = new GcDiag(true);
        d.record(3, 700);
        d.record(9, 700);
        assertEquals("iter=3", d.line("x").replaceAll(".*(iter=\\d+).*", "$1"));
    }

    @Test
    void everyOpAtOrAboveThresholdBecomesAnEventLineBelowDoesNot() {
        GcDiag d = new GcDiag(true, 1_000);
        d.record(0, 999); // below threshold: no event
        d.record(1, 1_000);
        d.record(2, 400_000);
        d.record(3, 5_000); // not a new max, still an event
        var events = d.eventLines("live_mvcc");
        assertEquals(3, events.size());
        assertTrue(events.get(0).contains("iter=1"), events.get(0));
        assertTrue(events.get(1).contains("stall_ns=400000"), events.get(1));
        assertTrue(events.get(2).contains("iter=3"), events.get(2));
        assertTrue(events.get(2).contains("at_epoch_ms="), events.get(2));
        assertTrue(events.get(0).startsWith("gc-diag live_mvcc "), events.get(0));
    }

    @Test
    void disabledBuffersNoEvents() {
        GcDiag d = new GcDiag(false, 1);
        d.record(0, 1_000_000);
        assertTrue(d.eventLines("x").isEmpty());
    }
}
