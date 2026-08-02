package net.knego.hiperf.smrcollections.mvccchurn;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Churn;
import net.knego.hiperf.smrcollections.CowBook;

/**
 * smr-collections/mvcc_churn (Java): the churn workload against the chunked copy-on-write book.
 * Cancels scatter writes across chunks rather than appending to the newest one, so this is where
 * CoW's first-touch copy cost is exercised hardest.
 */
public final class Main {
    private static final String EXPERIMENT = "mvcc_churn";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
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
