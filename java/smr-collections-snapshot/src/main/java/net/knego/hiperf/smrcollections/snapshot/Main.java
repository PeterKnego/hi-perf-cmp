package net.knego.hiperf.smrcollections.snapshot;

import java.util.Arrays;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.common.Stats;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Snapshotter;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/snapshot (Java): time serialize + restore of a steady book. */
public final class Main {
    private static final String EXPERIMENT = "snapshot";

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
            int maxBytes = 64 + cfg.cap() * 64 + cfg.levels() * 2 * 32;
            Snapshotter s = new Snapshotter(maxBytes);
            long[] snap = new long[cfg.iters()];
            long[] rest = new long[cfg.iters()];
            int snapLen = 0;
            for (int i = 0; i < cfg.warmup(); i++) {
                int len = s.encode(book);
                Snapshotter.restore(s.backing(), len, cfg);
            }
            for (int k = 0; k < cfg.iters(); k++) {
                long t0 = System.nanoTime();
                int len = s.encode(book);
                snap[k] = System.nanoTime() - t0;
                snapLen = len;
                long t1 = System.nanoTime();
                Book r = Snapshotter.restore(s.backing(), len, cfg);
                rest[k] = System.nanoTime() - t1;
                if (r.hwm() < 0) {
                    throw new IllegalStateException("unreachable");
                }
            }
            long[] snapCopy = Arrays.copyOf(snap, snap.length);
            SmrCollections.emitLatency(EXPERIMENT, "snapshot", snap);
            SmrCollections.emitLatency(EXPERIMENT, "restore", rest);
            SmrCollections.emitInt(EXPERIMENT, "snapshot_bytes", snapLen, "bytes", 1);
            double mean = Stats.mean(snapCopy);
            SmrCollections.emitDouble(EXPERIMENT, "snapshot_throughput", snapLen / (mean / 1e9), "bytes_per_sec", cfg.iters());
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
