package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.Arrays;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class SnapshotTest {
    private static SmrConfig cfg() {
        return new SmrConfig(4096, 64, 1, 0, 2000, 0, 0);
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
}
