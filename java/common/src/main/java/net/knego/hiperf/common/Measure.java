package net.knego.hiperf.common;

import java.util.Arrays;

/**
 * Shared measurement driver. Keeps the warmup + timed round-trip loop and the
 * sort/emit step identical across every network-rtt experiment so the only
 * thing that varies between transports is the per-round-trip callback.
 *
 * <p>The timed loop times {@link Config#iterations()} synchronous round trips
 * into a pre-allocated {@code long[]} (allocated before timing starts, so no
 * allocation happens on the timed path). The unit is nanoseconds.
 */
public final class Measure {

    /** Focus area shared by all network-rtt experiments. */
    public static final String FOCUS_AREA = "network-rtt";

    private static final String UNIT = "ns";

    private Measure() {}

    /** A single ping-pong round trip; throws on any I/O or echo-mismatch error. */
    @FunctionalInterface
    public interface RoundTrip {
        void run() throws Exception;
    }

    /**
     * Run {@code cfg.warmup()} discarded round trips, then time
     * {@code cfg.iterations()} round trips into a freshly allocated samples
     * array (one entry per iteration, in nanoseconds).
     *
     * @return the per-iteration round-trip durations in nanoseconds
     */
    public static long[] run(Config cfg, RoundTrip roundTrip) throws Exception {
        for (int i = 0; i < cfg.warmup(); i++) {
            roundTrip.run();
        }

        long[] samples = new long[cfg.iterations()]; // pre-allocated; no alloc in timed path
        for (int i = 0; i < cfg.iterations(); i++) {
            long start = System.nanoTime();
            roundTrip.run();
            samples[i] = System.nanoTime() - start;
        }
        return samples;
    }

    /**
     * Sort {@code samples} ascending and emit the three contract lines
     * ({@code rtt_p50}, {@code rtt_p99}, {@code rtt_mean}) for the given
     * experiment, with unit {@code ns} and {@code samples = samples.length}.
     */
    public static void emit(String experiment, long[] samples) {
        Arrays.sort(samples);
        long p50 = Stats.percentile(samples, 50);
        long p99 = Stats.percentile(samples, 99);
        double mean = Stats.mean(samples);
        long n = samples.length;
        new Result(FOCUS_AREA, experiment, "rtt_p50", p50, UNIT, n, "").emit();
        new Result(FOCUS_AREA, experiment, "rtt_p99", p99, UNIT, n, "").emit();
        new Result(FOCUS_AREA, experiment, "rtt_mean", mean, UNIT, n, "").emit();
    }
}
