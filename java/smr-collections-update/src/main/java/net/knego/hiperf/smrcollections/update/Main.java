package net.knego.hiperf.smrcollections.update;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/update (Java): time amend/partial-fill on existing orders. */
public final class Main {
    private static final String EXPERIMENT = "update";

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
            long[] samples = SmrCollections.measure(cfg.warmup(), cfg.iters(), () -> {
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
            });
            SmrCollections.emitLatency(EXPERIMENT, "update", samples);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
