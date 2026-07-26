package net.knego.hiperf.smrcollections.mvccinsert;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.CowBook;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/mvcc_insert (Java): insert cost on the chunked-CoW book. */
public final class Main {
    private static final String EXPERIMENT = "mvcc_insert";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            int[] i = {0};
            long[] samples = SmrCollections.measure(cfg.warmup(), cfg.iters(), () -> {
                Workload.nextInsert(rng, i[0], cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
                i[0]++;
            });
            SmrCollections.emitLatency(EXPERIMENT, "insert", samples);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
