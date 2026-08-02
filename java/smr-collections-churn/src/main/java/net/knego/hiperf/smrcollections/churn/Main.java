package net.knego.hiperf.smrcollections.churn;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Churn;

/**
 * smr-collections/churn (Java): insert/cancel/fill at a real-exchange order-to-trade ratio
 * against the flat stop-the-world book. Cancels recycle slots through the free list, so this is
 * the steady state a matching engine actually lives in.
 */
public final class Main {
    private static final String EXPERIMENT = "churn";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            Book book = new Book(cfg);
            Churn churn = new Churn(cfg);
            churn.prebuild(book, cfg.steady());
            long[] rss0 = new long[1];
            Churn.Samples s = Churn.run(cfg, book, churn, rss0);
            long growth = Math.max(0L, SmrCollections.rssBytes() - rss0[0]);
            Churn.emit(EXPERIMENT, s, growth);
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
