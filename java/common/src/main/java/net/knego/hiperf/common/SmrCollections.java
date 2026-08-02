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

    /** Live-experiment metric set: writer latency (+max), snapshot latency, counts, size. */
    public static void emitLive(String experiment, long[] writerNs, long[] snapNs, long skipped, long snapLen) {
        long max = 0;
        for (long v : writerNs) {
            if (v > max) {
                max = v;
            }
        }
        emitLatency(experiment, "writer", writerNs);
        emitInt(experiment, "writer_max", max, "ns", writerNs.length);
        // A distribution with no samples is skipped rather than emitted as zeros — if every
        // timed capture was skipped (serializer busy the whole run), snapNs is empty and
        // Stats.percentile would throw (idx = -1). Mirrors Churn.emit's same-shaped guard.
        // writer_*, snap_count, snap_skipped and snapshot_bytes still emit unconditionally: a
        // zero snapshot count is real information.
        if (snapNs.length > 0) {
            emitLatency(experiment, "snapshot", snapNs);
        }
        emitInt(experiment, "snap_count", snapNs.length, "count", 1);
        emitInt(experiment, "snap_skipped", skipped, "count", 1);
        emitInt(experiment, "snapshot_bytes", snapLen, "bytes", 1);
    }

    /**
     * Resident set size in bytes, from Linux /proc/self/statm field 2 (resident pages), or 0
     * where unreadable. The bench hosts are x86-64 Linux with 4 KiB pages, which is the only
     * case that must be right. Allocates, so callers must keep it out of timed regions.
     */
    public static long rssBytes() {
        try {
            String s = java.nio.file.Files.readString(java.nio.file.Path.of("/proc/self/statm"));
            String[] f = s.trim().split("\\s+");
            if (f.length < 2) {
                return 0L;
            }
            return Long.parseLong(f[1]) * 4096L;
        } catch (Exception e) {
            return 0L;
        }
    }
}
