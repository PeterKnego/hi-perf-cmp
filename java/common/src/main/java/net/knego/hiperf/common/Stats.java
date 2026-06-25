package net.knego.hiperf.common;

/**
 * Pure statistics over RTT samples. Comparability-critical: the percentile and
 * mean formulas must match the Rust and Go implementations exactly.
 */
public final class Stats {

    private Stats() {}

    /**
     * Nearest-rank percentile with no interpolation, over an array sorted
     * ascending: {@code sorted[ floor( p/100 * (n - 1) ) ]}.
     *
     * @param sorted samples sorted ascending; must be non-empty
     * @param p      percentile in [0, 100]
     */
    public static long percentile(long[] sorted, int p) {
        int n = sorted.length;
        int idx = (int) Math.floor((p / 100.0) * (n - 1));
        return sorted[idx];
    }

    /** Arithmetic mean of the samples. */
    public static double mean(long[] samples) {
        long sum = 0;
        for (long v : samples) {
            sum += v;
        }
        return (double) sum / samples.length;
    }
}
