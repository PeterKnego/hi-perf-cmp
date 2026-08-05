package net.knego.hiperf.smrcollections.livestw;

import net.knego.hiperf.common.GcDiag;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Snapshotter;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/live_stw (Java): writer latency with inline STW snapshots. */
public final class Main {
    private static final String EXPERIMENT = "live_stw";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            Book book = new Book(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            for (int i = 0; i < cfg.steady(); i++) {
                Workload.nextInsert(rng, i, cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
            }
            int n = cfg.steady();
            Workload.Update up = new Workload.Update();
            for (int i = 0; i < cfg.warmup(); i++) {
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
            }
            Snapshotter s = new Snapshotter(64 + cfg.cap() * 64 + cfg.levels() * 2 * 32);
            // warm the encode path + buffer pages so the k=0 trigger measures
            // steady-state stall, not first-touch cost
            s.encode(book);
            long[] writerNs = new long[cfg.liveIters()];
            GcDiag diag = new GcDiag();
            long[] snapNs = new long[cfg.liveIters() / cfg.snapEvery() + 1];
            int snapCount = 0;
            long snapLen = 0;
            for (int k = 0; k < cfg.liveIters(); k++) {
                long t0 = System.nanoTime();
                if (k % cfg.snapEvery() == 0) {
                    snapLen = s.encode(book);
                    snapNs[snapCount++] = System.nanoTime() - t0;
                }
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
                long ns = System.nanoTime() - t0;
                writerNs[k] = ns;
                diag.record(k, ns);
            }
            SmrCollections.emitLive(EXPERIMENT, writerNs, java.util.Arrays.copyOf(snapNs, snapCount), 0, snapLen);
            diag.emit(EXPERIMENT);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
