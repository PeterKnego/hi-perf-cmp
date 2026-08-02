package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.util.Arrays;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class SnapshotTest {
    private static SmrConfig cfg() {
        return new SmrConfig(4096, 64, 1, 0, 2000, 0, 0, 4096, 200000, 20000, 100);
    }

    private static Book build(SmrConfig c, int n) {
        Book b = new Book(c);
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        for (int i = 0; i < n; i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins);
            b.insert(ins.orderId, ins.price, ins.qty, ins.side);
        }
        return b;
    }

    @Test
    void roundTripPreservesQueries() {
        SmrConfig c = cfg();
        Book b = build(c, c.steady());
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int len = s.encode(b);
        byte[] img = Arrays.copyOf(s.backing(), len);
        Book r = Snapshotter.restore(img, len, c);
        assertEquals(b.bestBid(), r.bestBid());
        assertEquals(b.bestAsk(), r.bestAsk());
        assertEquals(b.hwm(), r.hwm());
        for (long id = 1; id <= c.steady(); id++) {
            assertEquals(b.getSlot(id), r.getSlot(id));
        }
        for (int t = 0; t < c.levels(); t++) {
            assertEquals(b.levelQty((byte) 0, t), r.levelQty((byte) 0, t));
            assertEquals(b.levelQty((byte) 1, t), r.levelQty((byte) 1, t));
        }
    }

    @Test
    void deterministicBytes() {
        SmrConfig c = cfg();
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int l1 = s.encode(build(c, c.steady()));
        byte[] a = Arrays.copyOf(s.backing(), l1);
        int l2 = s.encode(build(c, c.steady()));
        byte[] bb = Arrays.copyOf(s.backing(), l2);
        assertArrayEquals(a, bb);
    }

    private static Book buildBookWithCancels(SmrConfig c, int n, int cancelEvery) {
        Book b = new Book(c);
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        for (int i = 0; i < n; i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins);
            b.insert(ins.orderId, ins.price, ins.qty, ins.side);
            if (i % cancelEvery == cancelEvery - 1) {
                b.cancel(ins.orderId);
            }
        }
        return b;
    }

    private static SmrConfig snapCfg() {
        return new SmrConfig(4096, 64, 1L, 0L, 2000, 0, 0, 4096, 200_000, 20_000, 100);
    }

    @Test
    void roundTripPreservesFreeListOrder() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        assertNotEquals(Book.NIL, b.freeHead, "test needs a non-empty free list");
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        byte[] img = java.util.Arrays.copyOf(s.backing(), n);
        Book r = Snapshotter.restore(img, n, c);
        assertEquals(walkFree(b), walkFree(r), "free list order survives exactly");
    }

    private static java.util.List<Integer> walkFree(Book b) {
        java.util.List<Integer> out = new java.util.ArrayList<>();
        for (int slot = b.freeHead; slot != Book.NIL; slot = b.pool[slot].next) {
            out.add(slot);
        }
        return out;
    }

    @Test
    void restoreAfterCancelsReencodesIdentically() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n1 = s.encode(b);
        byte[] first = java.util.Arrays.copyOf(s.backing(), n1);
        Book r = Snapshotter.restore(first, n1, c);
        Snapshotter s2 = new Snapshotter(4 * 1024 * 1024);
        int n2 = s2.encode(r);
        byte[] second = java.util.Arrays.copyOf(s2.backing(), n2);
        assertArrayEquals(first, second);
    }

    @Test
    void freedSlotsStayOutOfTheIdMap() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        Book r = Snapshotter.restore(java.util.Arrays.copyOf(s.backing(), n), n, c);
        for (int slot = 0; slot < b.hwm(); slot++) {
            long id = b.pool[slot].orderId;
            if (id != 0) {
                assertEquals(slot, r.getSlot(id), "live order " + id + " keeps its slot");
            } else {
                assertEquals(0, r.pool[slot].orderId, "slot " + slot + " stays marked free");
            }
        }
        assertNull(r.ids.get(0L), "orderId 0 must never be a key");
    }

    @Test
    void restoreRejectsCapacityMismatch() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        byte[] img = java.util.Arrays.copyOf(s.backing(), n);
        SmrConfig smaller = new SmrConfig(2048, 64, 1L, 0L, 2000, 0, 0, 2048, 200_000, 20_000, 100);
        assertThrows(IllegalArgumentException.class, () -> Snapshotter.restore(img, n, smaller));
    }
}
