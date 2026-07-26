package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertEquals;

import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class CowBookTest {

    private static SmrConfig cfg() {
        return new SmrConfig(1024, 300, 1, 0, 500, 0, 0, 64, 200000, 20000);
    }

    @Test
    void cowBookMatchesBookQueriesAfterMixedOps() {
        SmrConfig c = cfg();
        Book b = new Book(c);
        CowBook cb = new CowBook(c);
        Workload.SplitMix r1 = new Workload.SplitMix(Workload.SEED);
        Workload.SplitMix r2 = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ia = new Workload.Insert();
        Workload.Insert ix = new Workload.Insert();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(r1, i, c.levels(), c.tick(), c.priceMin(), ia);
            Workload.nextInsert(r2, i, c.levels(), c.tick(), c.priceMin(), ix);
            b.insert(ia.orderId, ia.price, ia.qty, ia.side);
            cb.insert(ix.orderId, ix.price, ix.qty, ix.side);
        }
        Workload.Update ua = new Workload.Update();
        Workload.Update ux = new Workload.Update();
        for (int i = 0; i < 1000; i++) {
            Workload.nextUpdate(r1, c.steady(), ua);
            Workload.nextUpdate(r2, c.steady(), ux);
            b.update(ua.orderId, ua.fillQty);
            cb.update(ux.orderId, ux.fillQty);
        }
        assertEquals(b.hwm(), cb.hwm);
        assertEquals(b.bestBid(), cb.bestBid);
        assertEquals(b.bestAsk(), cb.bestAsk);
        for (long id = 1; id <= c.steady(); id++) {
            assertEquals(b.getSlot(id), cb.getSlot(id));
        }
        for (int t = 0; t < c.levels(); t++) {
            assertEquals(b.levelQty((byte) 0, t), cb.levelQty((byte) 0, t));
            assertEquals(b.levelQty((byte) 1, t), cb.levelQty((byte) 1, t));
        }
        for (int slot = 0; slot < cb.hwm; slot++) {
            assertEquals(b.pool[slot].filled, cb.orderFilled(slot));
        }
    }

    @Test
    void captureIsolatesRootFromLaterWrites() {
        SmrConfig c = cfg();
        CowBook cb = new CowBook(c);
        for (int i = 0; i < c.steady(); i++) {
            cb.insert(i + 1, i % c.levels(), 10, (byte) (i % 2));
        }
        CowRoot root = cb.capture();
        long before = root.orderFilled(5);
        cb.update(6, 7); // order 6 lives in slot 5
        assertEquals(before, root.orderFilled(5), "root must be frozen");
        assertEquals(before + 7, cb.orderFilled(5), "writer must advance");
    }

    @Test
    void successiveCapturesSeeSuccessiveStates() {
        SmrConfig c = cfg();
        CowBook cb = new CowBook(c);
        cb.insert(1, 5, 10, (byte) 0);
        CowRoot r1 = cb.capture();
        cb.update(1, 4);
        CowRoot r2 = cb.capture();
        assertEquals(0, r1.orderFilled(0));
        assertEquals(4, r2.orderFilled(0));
    }
}
