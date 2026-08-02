package net.knego.hiperf.smrcollections.livemvccchurn;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Churn;
import net.knego.hiperf.smrcollections.CowBook;
import net.knego.hiperf.smrcollections.CowRoot;
import net.knego.hiperf.smrcollections.CowSnapshotter;

/**
 * smr-collections/live_mvcc_churn (Java): writer-observed latency under the churn workload while
 * a serializer thread encodes captured CoW roots concurrently. The writer pays only the
 * O(#chunks) capture plus CoW chunk copies as it re-dirties state; the per-op split shows which
 * op absorbed the capture cost.
 */
public final class Main {
    private static final String EXPERIMENT = "live_mvcc_churn";

    private record CapMsg(CowRoot root, long t0) {}

    public static void main(String[] args) throws InterruptedException {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            // Throwaway JVM pre-run: warms fill() to HotSpot's C2 tier on a scratch store before
            // the real one is built, so fill_p50/p99 aren't measured on cold interpreted/C1 code.
            // Discarded; does not touch the measured stream. See Churn.warmJit's javadoc.
            Churn.warmJit(cfg, new CowBook(cfg));
            CowBook book = new CowBook(cfg);
            Churn churn = new Churn(cfg);
            churn.prebuild(book, cfg.steady());
            Churn.Op op = new Churn.Op();
            for (int k = 0; k < cfg.warmup(); k++) {
                churn.nextOp(op);
                Churn.apply(book, op);
            }

            int maxBytes = 64 + cfg.cap() * 64 + cfg.levels() * 2 * 32;
            ArrayBlockingQueue<CapMsg> q = new ArrayBlockingQueue<>(1);
            AtomicBoolean busy = new AtomicBoolean(false);
            List<Long> snapDur = new ArrayList<>();
            long[] snapLenBox = new long[1];
            Thread ser = new Thread(() -> {
                CowSnapshotter s = new CowSnapshotter(maxBytes);
                while (true) {
                    CapMsg m;
                    try {
                        m = q.take();
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        return;
                    }
                    if (m.root() == null) {
                        return; // poison pill
                    }
                    snapLenBox[0] = s.encodeRoot(m.root());
                    snapDur.add(System.nanoTime() - m.t0());
                    busy.set(false);
                }
            });
            ser.start();

            // Warm handshake: push one untimed capture through the serializer
            // before the timed loop starts, so first-touch page faults of its
            // encode buffer (and JIT-cold code) land here instead of in the
            // first timed sample. Same message shape as the timed sends; the
            // first recorded duration is dropped below.
            busy.set(true);
            q.put(new CapMsg(book.capture(), System.nanoTime()));
            while (busy.get()) {
                Thread.onSpinWait();
            }

            long[] writerNs = new long[cfg.liveIters()];
            long skipped = 0;
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
                boolean captured = false;
                if (fired) {
                    if (busy.get()) {
                        skipped++;
                    } else {
                        busy.set(true);
                        q.put(new CapMsg(book.capture(), t0));
                        captured = true;
                    }
                }
                Churn.apply(book, op);
                long ns = System.nanoTime() - t0;
                // Sample RSS only AFTER the clock closes, and only on an iteration that
                // actually captured (not merely on `fired`, which is also true on iterations
                // where the serializer was busy and the capture was skipped). rssBytes() reads
                // /proc/self/statm — microseconds against sub-microsecond ops — so calling it
                // inside the timed region would inflate writer_max, the one metric this cell
                // exists to report precisely. This sample only sees the writer-side capture()
                // cost, not the encode: encodeRoot() runs concurrently on the serializer
                // thread, so growth from it and from ongoing CoW chunk copies lags behind this
                // point by design; the sample taken after the serializer joins below catches
                // the final in-flight window's growth.
                if (captured) {
                    rssPeak = Math.max(rssPeak, SmrCollections.rssBytes());
                }
                writerNs[k] = ns;
                if (op.kind == Churn.OP_INSERT) {
                    ins[insN++] = ns;
                } else if (op.kind == Churn.OP_CANCEL) {
                    can[canN++] = ns;
                } else {
                    fil[filN++] = ns;
                }
            }
            q.put(new CapMsg(null, 0));
            ser.join();
            // Catch growth from the final in-flight window: the last capture's encodeRoot() and
            // any CoW chunk copies made since may still have been landing concurrently with the
            // loop above, so only a post-join sample sees their full effect. Without it,
            // rss_peak_bytes reads systematically low against the twins in the other languages.
            rssPeak = Math.max(rssPeak, SmrCollections.rssBytes());
            // The first recorded duration is the warm handshake above (the
            // serializer is a single-consumer FIFO over the queue, so put
            // order == process order) — exclude it from the emitted stats
            // and counts.
            long[] snapNs = snapDur.stream().skip(1).mapToLong(Long::longValue).toArray();
            SmrCollections.emitLive(EXPERIMENT, writerNs, snapNs, skipped, snapLenBox[0]);
            if (insN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "insert", java.util.Arrays.copyOf(ins, insN));
            }
            if (canN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "cancel", java.util.Arrays.copyOf(can, canN));
            }
            if (filN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "fill", java.util.Arrays.copyOf(fil, filN));
            }
            SmrCollections.emitInt(EXPERIMENT, "rss_peak_bytes", rssPeak, "bytes", 1);
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
