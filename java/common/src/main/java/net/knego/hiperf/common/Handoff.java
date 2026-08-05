package net.knego.hiperf.common;

import java.util.Arrays;

/**
 * Shared thread-handoff driver: the warmup + timed round-trip loop and result
 * emission. Mirrors {@link Measure} but for the infallible in-process handoff,
 * and adds the throughput emitter for the ring experiment.
 */
public final class Handoff {

    /** Focus area shared by all thread-handoff experiments. */
    public static final String FOCUS_AREA = "thread-handoff";

    private Handoff() {}

    /** A single ping-pong handoff round trip (infallible, in-process). */
    @FunctionalInterface
    public interface RoundTrip {
        void run();
    }

    /**
     * Run {@code cfg.warmup()} discarded round trips, then time
     * {@code cfg.iterations()} round trips into a pre-allocated array (ns).
     */
    public static long[] measure(HandoffConfig cfg, RoundTrip roundTrip) {
        for (int i = 0; i < cfg.warmup(); i++) {
            roundTrip.run();
        }
        long[] samples = new long[cfg.iterations()];
        for (int i = 0; i < cfg.iterations(); i++) {
            long start = System.nanoTime();
            roundTrip.run();
            samples[i] = System.nanoTime() - start;
        }
        return samples;
    }

    /**
     * {@link #measure} with a busy-waited gap of {@code cfg.gapNs()} before
     * every round trip (warmup included), left OUTSIDE the timed window. The
     * gap lets a responder's idle ladder ramp between requests; it is a
     * busy-wait, not a sleep, so the requester's send timing does not inherit
     * the timer overshoot the backoff cells exist to measure.
     */
    public static long[] measurePaced(HandoffConfig cfg, RoundTrip roundTrip) {
        long gap = cfg.gapNs();
        for (int i = 0; i < cfg.warmup(); i++) {
            busyWait(gap);
            roundTrip.run();
        }
        long[] samples = new long[cfg.iterations()];
        for (int i = 0; i < cfg.iterations(); i++) {
            busyWait(gap);
            long start = System.nanoTime();
            roundTrip.run();
            samples[i] = System.nanoTime() - start;
        }
        return samples;
    }

    private static void busyWait(long ns) {
        long deadline = System.nanoTime() + ns;
        while (System.nanoTime() < deadline) {
            Thread.onSpinWait();
        }
    }

    /** Sort and emit handoff_rtt_p50/p99/mean (ns) for the experiment. */
    public static void emit(String experiment, long[] samples) {
        Arrays.sort(samples);
        long p50 = Stats.percentile(samples, 50);
        long p99 = Stats.percentile(samples, 99);
        double mean = Stats.mean(samples);
        long n = samples.length;
        new Result(FOCUS_AREA, experiment, "handoff_rtt_p50", p50, "ns", n, "").emit();
        new Result(FOCUS_AREA, experiment, "handoff_rtt_p99", p99, "ns", n, "").emit();
        new Result(FOCUS_AREA, experiment, "handoff_rtt_mean", mean, "ns", n, "").emit();
    }

    /** Emit the single handoff_throughput (ops_per_sec) line. */
    public static void emitThroughput(String experiment, double opsPerSec, long samples) {
        new Result(FOCUS_AREA, experiment, "handoff_throughput", opsPerSec, "ops_per_sec", samples, "").emit();
    }
}
