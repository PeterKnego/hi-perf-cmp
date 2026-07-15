package net.knego.hiperf.common;

import java.util.Arrays;

/** smr-collections timed loop + emit helpers (mirrors {@link Handoff}). */
public final class SmrCollections {

    public static final String FOCUS_AREA = "smr-collections";

    private SmrCollections() {}

    /** warmup discarded ops, then time iters ops (ns) into a pre-allocated array. */
    public static long[] measure(int warmup, int iters, Runnable op) {
        for (int i = 0; i < warmup; i++) {
            op.run();
        }
        long[] samples = new long[iters];
        for (int i = 0; i < iters; i++) {
            long start = System.nanoTime();
            op.run();
            samples[i] = System.nanoTime() - start;
        }
        return samples;
    }

    /** Sort and emit {prefix}_p50/p99/mean (ns). */
    public static void emitLatency(String experiment, String prefix, long[] samples) {
        Arrays.sort(samples);
        long n = samples.length;
        new Result(FOCUS_AREA, experiment, prefix + "_p50", Stats.percentile(samples, 50), "ns", n, "").emit();
        new Result(FOCUS_AREA, experiment, prefix + "_p99", Stats.percentile(samples, 99), "ns", n, "").emit();
        new Result(FOCUS_AREA, experiment, prefix + "_mean", Stats.mean(samples), "ns", n, "").emit();
    }

    public static void emitInt(String experiment, String metric, long value, String unit, long samples) {
        new Result(FOCUS_AREA, experiment, metric, (double) value, unit, samples, "").emit();
    }

    public static void emitDouble(String experiment, String metric, double value, String unit, long samples) {
        new Result(FOCUS_AREA, experiment, metric, value, unit, samples, "").emit();
    }
}
