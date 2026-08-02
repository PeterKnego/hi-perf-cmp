package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.concurrent.SynchronousQueue;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class CowSnapshotTest {

    private static SmrConfig goldenCfg() {
        return new SmrConfig(4096, 64, 1, 0, 2000, 0, 0, 512, 200000, 20000, 100);
    }

    private static CowBook buildCow(SmrConfig c, int n) {
        CowBook b = new CowBook(c);
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        for (int i = 0; i < n; i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins);
            b.insert(ins.orderId, ins.price, ins.qty, ins.side);
        }
        return b;
    }

    private static int maxBytes(SmrConfig c) {
        return 64 + c.cap() * 64 + c.levels() * 2 * 32;
    }

    @Test
    void cowBookMatchesGoldenBytes() throws Exception {
        SmrConfig c = goldenCfg();
        CowBook cb = buildCow(c, c.steady());
        CowSnapshotter s = new CowSnapshotter(maxBytes(c));
        int len = s.encodeRoot(cb.capture());
        byte[] got = Arrays.copyOf(s.backing(), len);
        byte[] want = Files.readAllBytes(
                Path.of("../../rust/smr-collections/testdata/golden_snapshot.bin"));
        assertArrayEquals(want, got, "CowBook bytes == golden bytes");
    }

    @Test
    void cowEncodeEqualsStwEncodeAfterMixedOps() {
        SmrConfig c = goldenCfg();
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
        for (int i = 0; i < 500; i++) {
            Workload.nextUpdate(r1, c.steady(), ua);
            Workload.nextUpdate(r2, c.steady(), ux);
            b.update(ua.orderId, ua.fillQty);
            cb.update(ux.orderId, ux.fillQty);
        }
        Snapshotter stw = new Snapshotter(maxBytes(c));
        int n1 = stw.encode(b);
        CowSnapshotter cow = new CowSnapshotter(maxBytes(c));
        int n2 = cow.encodeRoot(cb.capture());
        assertArrayEquals(
                Arrays.copyOf(stw.backing(), n1), Arrays.copyOf(cow.backing(), n2));
    }

    @Test
    void restoreCowRoundTripsAndRejectsCorruption() {
        SmrConfig c = goldenCfg();
        CowBook cb = buildCow(c, c.steady());
        CowSnapshotter s = new CowSnapshotter(maxBytes(c));
        int len = s.encodeRoot(cb.capture());
        byte[] img = Arrays.copyOf(s.backing(), len);
        CowBook r = CowSnapshotter.restoreCow(img, len, c);
        CowSnapshotter s2 = new CowSnapshotter(maxBytes(c));
        int len2 = s2.encodeRoot(r.capture());
        assertArrayEquals(img, Arrays.copyOf(s2.backing(), len2));
        byte[] bad = img.clone();
        bad[0] ^= 0xFF;
        assertThrows(IllegalArgumentException.class, () -> CowSnapshotter.restoreCow(bad, len, c));
    }

    /** Capture at update k under concurrent encoding == STW replay to k. */
    @Test
    void concurrentCaptureEqualsStwReplay() throws Exception {
        SmrConfig c = goldenCfg();
        final int totalUpdates = 2000;
        final int captureAt = 700;

        Book ref = new Book(c);
        Workload.SplitMix rr = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        Workload.Update up = new Workload.Update();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(rr, i, c.levels(), c.tick(), c.priceMin(), ins);
            ref.insert(ins.orderId, ins.price, ins.qty, ins.side);
        }
        for (int i = 0; i < captureAt; i++) {
            Workload.nextUpdate(rr, c.steady(), up);
            ref.update(up.orderId, up.fillQty);
        }
        Snapshotter stw = new Snapshotter(maxBytes(c));
        int wn = stw.encode(ref);
        byte[] want = Arrays.copyOf(stw.backing(), wn);

        CowBook cb = buildCow(c, c.steady());
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins); // skip consumed draws
        }
        SynchronousQueue<CowRoot> rootQ = new SynchronousQueue<>();
        SynchronousQueue<byte[]> gotQ = new SynchronousQueue<>();
        Thread ser = new Thread(() -> {
            try {
                CowRoot root = rootQ.take();
                CowSnapshotter s = new CowSnapshotter(maxBytes(goldenCfg()));
                int n = s.encodeRoot(root);
                gotQ.put(Arrays.copyOf(s.backing(), n));
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        });
        ser.start();
        for (int k = 0; k < totalUpdates; k++) {
            if (k == captureAt) {
                rootQ.put(cb.capture());
            }
            Workload.nextUpdate(rng, c.steady(), up);
            cb.update(up.orderId, up.fillQty);
        }
        byte[] got = gotQ.take();
        ser.join();
        assertArrayEquals(want, got, "concurrent capture == STW replay");
        assertEquals(want.length, got.length);
    }

    @Test
    void cowCancelImageMatchesFlatImage() {
        SmrConfig c = new SmrConfig(4096, 64, 1L, 0L, 2000, 0, 0, 512, 200_000, 20_000, 100);
        Book b = new Book(c);
        CowBook cb = new CowBook(c);
        Workload.SplitMix r1 = new Workload.SplitMix(Workload.SEED);
        Workload.SplitMix r2 = new Workload.SplitMix(Workload.SEED);
        Workload.Insert i1 = new Workload.Insert();
        Workload.Insert i2 = new Workload.Insert();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(r1, i, c.levels(), c.tick(), c.priceMin(), i1);
            Workload.nextInsert(r2, i, c.levels(), c.tick(), c.priceMin(), i2);
            b.insert(i1.orderId, i1.price, i1.qty, i1.side);
            cb.insert(i2.orderId, i2.price, i2.qty, i2.side);
        }
        for (long id = 1; id <= c.steady(); id += 3) {
            b.cancel(id);
            cb.cancel(id);
        }
        Snapshotter s1 = new Snapshotter(4 * 1024 * 1024);
        int n1 = s1.encode(b);
        byte[] flat = java.util.Arrays.copyOf(s1.backing(), n1);
        CowSnapshotter s2 = new CowSnapshotter(4 * 1024 * 1024);
        int n2 = s2.encodeRoot(cb.capture());
        byte[] cow = java.util.Arrays.copyOf(s2.backing(), n2);
        assertArrayEquals(flat, cow, "CoW image == flat image");
    }
}
