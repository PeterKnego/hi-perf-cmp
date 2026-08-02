package net.knego.hiperf.smrcollections;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;

/**
 * The churn workload: a deterministic insert/cancel/fill stream at a configurable order-to-trade
 * ratio (default 1 %, the real-exchange figure).
 *
 * <p>Op generation is deliberately outside the timed region — the driver produces an op, the
 * caller times only the store's application of it, so the per-op numbers are store work alone.
 * Note this makes them NOT directly comparable with the older insert/update cells, which time
 * their own generation; see the design spec's "Must be recorded in the next run's journal entry".
 */
public final class Churn {

    public static final byte OP_INSERT = 0;
    public static final byte OP_CANCEL = 1;
    public static final byte OP_FILL = 2;

    /** The store surface a churn stream drives. Book and CowBook both implement it. */
    public interface Store {
        void insert(long orderId, long price, long qty, byte side);

        void cancel(long orderId);

        void fill(long orderId);
    }

    /** Reusable op holder — filled by nextOp, never allocated per call. */
    public static final class Op {
        public byte kind;
        public long orderId;
        public long price;
        public long qty;
        public byte side;
    }

    private final Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
    private final Workload.Insert scratch = new Workload.Insert();
    /** Order IDs currently resting, dense so a victim is one uniform draw. */
    private final long[] live;
    private int liveN;
    /**
     * Global op index: drives both the insert/depart alternation and the order ID, so IDs are
     * sparse (1, 3, 5, …) but never reused.
     */
    private int i;

    private final int otrBps;
    private final int levels;
    private final long tick;
    private final long priceMin;

    public Churn(SmrConfig cfg) {
        // Peak occupancy is steady+1, not steady: after prebuild the op index is
        // even, so the next op is an insert before any departure. Rust's Vec and
        // Go's slice grow transparently; a fixed array must be sized for it.
        this.live = new long[cfg.cap() + 1];
        this.otrBps = cfg.otrBps();
        this.levels = cfg.levels();
        this.tick = cfg.tick();
        this.priceMin = cfg.priceMin();
    }

    private void insertOp(Op out) {
        Workload.nextInsert(rng, i, levels, tick, priceMin, scratch);
        i++;
        live[liveN++] = scratch.orderId;
        out.kind = OP_INSERT;
        out.orderId = scratch.orderId;
        out.price = scratch.price;
        out.qty = scratch.qty;
        out.side = scratch.side;
    }

    /**
     * Fill {@code out} with the next op. Even index inserts, odd index departs; a departure is a
     * fill with probability otrBps/10000, otherwise a cancel.
     */
    public void nextOp(Op out) {
        if (i % 2 == 0 || liveN == 0) {
            insertOp(out);
            return;
        }
        i++;
        int v = (int) Long.remainderUnsigned(rng.next(), liveN);
        long id = live[v];
        boolean isFill = Long.remainderUnsigned(rng.next(), 10_000L) < otrBps;
        // swap-remove, matching Rust's Vec::swap_remove exactly — the op streams must be identical.
        live[v] = live[liveN - 1];
        liveN--;
        out.kind = isFill ? OP_FILL : OP_CANCEL;
        out.orderId = id;
    }

    /** Bring the store to its steady-state live set with inserts only. */
    public void prebuild(Store store, int steady) {
        Op op = new Op();
        for (int k = 0; k < steady; k++) {
            insertOp(op);
            apply(store, op);
        }
    }

    public static void apply(Store store, Op op) {
        if (op.kind == OP_INSERT) {
            store.insert(op.orderId, op.price, op.qty, op.side);
        } else if (op.kind == OP_CANCEL) {
            store.cancel(op.orderId);
        } else {
            store.fill(op.orderId);
        }
    }

    /** Per-op-type sample buffers, preallocated so the timed loop never allocates. */
    public static final class Samples {
        public final long[] insertNs;
        public final long[] cancelNs;
        public final long[] fillNs;
        public int insertN;
        public int cancelN;
        public int fillN;

        Samples(int half) {
            this.insertNs = new long[half];
            this.cancelNs = new long[half];
            this.fillNs = new long[half];
        }
    }

    /**
     * Warm up, then time cfg.iters() ops into per-op-type buffers. Only the store call is inside
     * the clock. {@code rssOut[0]} receives the RSS baseline, taken after the buffers are
     * allocated so their pages are not counted as store growth.
     */
    public static Samples run(SmrConfig cfg, Store store, Churn c, long[] rssOut) {
        Op op = new Op();
        for (int k = 0; k < cfg.warmup(); k++) {
            c.nextOp(op);
            apply(store, op);
        }
        Samples s = new Samples(cfg.iters() / 2 + 1);
        // HotSpot eagerly zeroes new arrays, so the pages behind insertNs/cancelNs/fillNs are
        // already resident by the time we read RSS here — the baseline includes them rather
        // than attributing their growth to the store. Load-bearing, not incidental.
        rssOut[0] = SmrCollections.rssBytes();
        for (int k = 0; k < cfg.iters(); k++) {
            c.nextOp(op);
            long t0 = System.nanoTime();
            apply(store, op);
            long ns = System.nanoTime() - t0;
            if (op.kind == OP_INSERT) {
                s.insertNs[s.insertN++] = ns;
            } else if (op.kind == OP_CANCEL) {
                s.cancelNs[s.cancelN++] = ns;
            } else {
                s.fillNs[s.fillN++] = ns;
            }
        }
        return s;
    }

    /**
     * Emit the per-op-type distributions plus RSS growth. A distribution with no samples is
     * skipped rather than emitted as zeros — at SMRC_OTR_BPS=0 there are no fills, and a
     * fabricated zero would read as a real measurement.
     */
    public static void emit(String experiment, Samples s, long rssGrowth) {
        if (s.insertN > 0) {
            SmrCollections.emitLatency(experiment, "insert", java.util.Arrays.copyOf(s.insertNs, s.insertN));
        }
        if (s.cancelN > 0) {
            SmrCollections.emitLatency(experiment, "cancel", java.util.Arrays.copyOf(s.cancelNs, s.cancelN));
        }
        if (s.fillN > 0) {
            SmrCollections.emitLatency(experiment, "fill", java.util.Arrays.copyOf(s.fillNs, s.fillN));
        }
        SmrCollections.emitInt(experiment, "rss_growth_bytes", rssGrowth, "bytes", 1);
    }
}
