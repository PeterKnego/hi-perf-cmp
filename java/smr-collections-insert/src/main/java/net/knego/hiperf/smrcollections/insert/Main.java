package net.knego.hiperf.smrcollections.insert;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/insert (Java): time inserting resting orders. */
public final class Main {
    private static final String EXPERIMENT = "insert";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            Book book = new Book(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            int[] i = {0};
            long[] samples = SmrCollections.measure(cfg.warmup(), cfg.iters(), () -> {
                Workload.nextInsert(rng, i[0]++, cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
            });
            SmrCollections.emitLatency(EXPERIMENT, "insert", samples);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
