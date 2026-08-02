package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class ChurnTest {

    private static SmrConfig churnCfg() {
        return new SmrConfig(4096, 64, 1L, 0L, 2000, 0, 0, 4096, 200_000, 20_000, 100);
    }

    @Test
    void opStreamIsDeterministic() {
        SmrConfig c = churnCfg();
        Churn a = new Churn(c);
        Churn b = new Churn(c);
        Churn.Op oa = new Churn.Op();
        Churn.Op ob = new Churn.Op();
        for (int k = 0; k < 10_000; k++) {
            a.nextOp(oa);
            b.nextOp(ob);
            assertEquals(oa.kind, ob.kind, "op " + k + " kind diverged");
            assertEquals(oa.orderId, ob.orderId, "op " + k + " id diverged");
        }
    }

    @Test
    void streamAlternatesAndHonoursOtr() {
        SmrConfig c = churnCfg();
        Churn ch = new Churn(c);
        Book store = new Book(c);
        ch.prebuild(store, c.steady());
        Churn.Op op = new Churn.Op();
        int ins = 0;
        int can = 0;
        int fil = 0;
        for (int i = 0; i < 100_000; i++) {
            ch.nextOp(op);
            if (op.kind == Churn.OP_INSERT) {
                ins++;
            } else if (op.kind == Churn.OP_CANCEL) {
                can++;
            } else {
                fil++;
            }
        }
        assertEquals(50_000, ins, "half the ops are inserts");
        assertEquals(50_000, can + fil, "the other half depart");
        assertTrue(fil >= 300 && fil <= 800, "fills out of band: " + fil);
    }

    @Test
    void liveSetStaysConstant() {
        SmrConfig c = churnCfg();
        Churn ch = new Churn(c);
        Book store = new Book(c);
        ch.prebuild(store, c.steady());
        Churn.Op op = new Churn.Op();
        for (int i = 0; i < 20_000; i++) {
            ch.nextOp(op);
            Churn.apply(store, op);
        }
        int live = 0;
        for (int slot = 0; slot < store.hwm(); slot++) {
            if (store.pool[slot].orderId != 0) {
                live++;
            }
        }
        assertEquals(c.steady(), live, "alternating stream holds the live set flat");
    }

    @Test
    void snapshotRestoreReplayIsBitIdentical() {
        SmrConfig c = churnCfg();
        Churn ch = new Churn(c);
        Book hot = new Book(c);
        ch.prebuild(hot, c.steady());
        Churn.Op op = new Churn.Op();
        for (int i = 0; i < 5000; i++) {
            ch.nextOp(op);
            Churn.apply(hot, op);
        }
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(hot);
        byte[] img = java.util.Arrays.copyOf(s.backing(), n);
        Book cold = Snapshotter.restore(img, n, c);
        // Replay the SAME ops into both.
        Churn.Op[] ops = new Churn.Op[5000];
        for (int i = 0; i < ops.length; i++) {
            ops[i] = new Churn.Op();
            ch.nextOp(ops[i]);
        }
        for (Churn.Op o : ops) {
            Churn.apply(hot, o);
            Churn.apply(cold, o);
        }
        Snapshotter sh = new Snapshotter(4 * 1024 * 1024);
        Snapshotter sc = new Snapshotter(4 * 1024 * 1024);
        int nh = sh.encode(hot);
        int nc = sc.encode(cold);
        assertArrayEquals(
                java.util.Arrays.copyOf(sh.backing(), nh),
                java.util.Arrays.copyOf(sc.backing(), nc),
                "restored replica diverged from the never-restarted one");
    }

    /** The cross-language check: Java must reproduce the image Rust exported, byte for byte. */
    @Test
    void crossLanguageChurnGoldenBytes() throws Exception {
        // Same path idiom as the existing GoldenTest (GoldenTest.java:15).
        byte[] golden = Files.readAllBytes(
                Path.of("..", "..", "rust", "smr-collections", "testdata", "golden_churn_snapshot.bin"));
        SmrConfig c = churnCfg();
        Book b = new Book(c);
        Churn ch = new Churn(c);
        ch.prebuild(b, c.steady());
        Churn.Op op = new Churn.Op();
        for (int i = 0; i < 10_000; i++) {
            ch.nextOp(op);
            Churn.apply(b, op);
        }
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        assertArrayEquals(golden, java.util.Arrays.copyOf(s.backing(), n),
                "java churn bytes differ from rust golden");
    }
}
