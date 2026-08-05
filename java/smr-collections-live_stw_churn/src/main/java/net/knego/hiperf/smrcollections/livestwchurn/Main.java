package net.knego.hiperf.smrcollections.livestwchurn;

import java.util.Arrays;
import net.knego.hiperf.common.GcDiag;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Churn;
import net.knego.hiperf.smrcollections.Snapshotter;

/**
 * smr-collections/live_stw_churn (Java): writer-observed latency under the churn workload while
 * stop-the-world snapshots run inline at a fixed op cadence (the trigger op pays the whole
 * serialize; writer_max is the stall).
 */
public final class Main {
    private static final String EXPERIMENT = "live_stw_churn";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            // Throwaway JVM pre-run: warms fill() to HotSpot's C2 tier on a scratch store before
            // the real one is built, so fill_p50/p99 aren't measured on cold interpreted/C1 code.
            // Discarded; does not touch the measured stream. See Churn.warmJit's javadoc.
            Churn.warmJit(cfg, new Book(cfg));
            Book book = new Book(cfg);
            Churn churn = new Churn(cfg);
            churn.prebuild(book, cfg.steady());
            Churn.Op op = new Churn.Op();
            for (int k = 0; k < cfg.warmup(); k++) {
                churn.nextOp(op);
                Churn.apply(book, op);
            }
            Snapshotter s = new Snapshotter(64 + cfg.cap() * 64 + cfg.levels() * 2 * 32);
            // warm the encode path + buffer pages so the k=0 trigger measures steady-state
            // stall, not first-touch cost
            s.encode(book);

            long[] writerNs = new long[cfg.liveIters()];
            GcDiag diag = new GcDiag();
            long[] snapNs = new long[cfg.liveIters() / cfg.snapEvery() + 1];
            int snapCount = 0;
            long snapLen = 0;
            int half = cfg.liveIters() / 2 + 1;
            long[] ins = new long[half];
            long[] can = new long[half];
            long[] fil = new long[half];
            int insN = 0;
            int canN = 0;
            int filN = 0;
            long rssPeak = SmrCollections.rssBytes();
            for (int k = 0; k < cfg.liveIters(); k++) {
                churn.nextOp(op);
                boolean fired = k % cfg.snapEvery() == 0;
                long t0 = System.nanoTime();
                if (fired) {
                    snapLen = s.encode(book);
                    snapNs[snapCount++] = System.nanoTime() - t0;
                }
                Churn.apply(book, op);
                long ns = System.nanoTime() - t0;
                // Sample RSS only AFTER the clock closes: rssBytes() reads and parses
                // /proc/self/statm — microseconds against sub-microsecond ops — so calling it
                // inside the timed region would inflate writer_max, the one metric this cell
                // exists to report precisely.
                if (fired) {
                    rssPeak = Math.max(rssPeak, SmrCollections.rssBytes());
                }
                writerNs[k] = ns;
                diag.record(k, ns);
                if (op.kind == Churn.OP_INSERT) {
                    ins[insN++] = ns;
                } else if (op.kind == Churn.OP_CANCEL) {
                    can[canN++] = ns;
                } else {
                    fil[filN++] = ns;
                }
            }
            SmrCollections.emitLive(EXPERIMENT, writerNs, Arrays.copyOf(snapNs, snapCount), 0, snapLen);
            diag.emit(EXPERIMENT);
            if (insN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "insert", Arrays.copyOf(ins, insN));
            }
            if (canN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "cancel", Arrays.copyOf(can, canN));
            }
            if (filN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "fill", Arrays.copyOf(fil, filN));
            }
            SmrCollections.emitInt(EXPERIMENT, "rss_peak_bytes", rssPeak, "bytes", 1);
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
