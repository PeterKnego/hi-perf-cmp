package net.knego.hiperf.common;

/** Fixed-capacity LOB benchmark config from SMRC_* env vars (plan Appendix A.1). */
public record SmrConfig(
        int cap, int levels, long tick, long priceMin, int steady, int warmup, int iters,
        int chunk, int liveIters, int snapEvery, int otrBps) {

    public static SmrConfig fromEnv() {
        int cap = Env.readPositiveInt("SMRC_CAP", 262144);
        int levels = Env.readPositiveInt("SMRC_LEVELS", 1024);
        long tick = Env.readPositiveInt("SMRC_TICK", 1);
        int steady = Env.readPositiveInt("SMRC_STEADY", 60000);
        int warmup = Env.readPositiveInt("SMRC_WARMUP", 10000);
        int iters = Env.readPositiveInt("SMRC_ITERS", 100000);
        long priceMin = readSignedLong("SMRC_PRICE_MIN", 0);
        int chunk = Env.readPositiveInt("SMRC_CHUNK", 4096);
        int liveIters = Env.readPositiveInt("SMRC_LIVE_ITERS", 200000);
        int snapEvery = Env.readPositiveInt("SMRC_SNAP_EVERY", 20000);
        int otrBps = Env.readNonNegativeInt("SMRC_OTR_BPS", 100);
        if (otrBps > 10000) {
            throw new IllegalArgumentException("SMRC_OTR_BPS must be in 0..=10000, got " + otrBps);
        }
        if (levels > 65535) {
            throw new IllegalArgumentException("SMRC_LEVELS must be <= 65535");
        }
        if (steady > cap || steady > 65535) {
            throw new IllegalArgumentException("SMRC_STEADY must be <= SMRC_CAP and <= 65535");
        }
        if (chunk > cap) {
            throw new IllegalArgumentException("SMRC_CHUNK must be <= SMRC_CAP");
        }
        if (snapEvery > liveIters) {
            throw new IllegalArgumentException("SMRC_SNAP_EVERY must be <= SMRC_LIVE_ITERS");
        }
        return new SmrConfig(cap, levels, tick, priceMin, steady, warmup, iters, chunk, liveIters, snapEvery, otrBps);
    }

    /**
     * Cells that bump-allocate (no free list) need a pool slot for every op they will ever run.
     * Churn cells recycle slots and must NOT call this.
     */
    public void requireBumpCapacity() {
        if ((long) warmup + iters > cap) {
            throw new IllegalArgumentException("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP");
        }
    }

    private static long readSignedLong(String name, long def) {
        String s = Env.trimmedOrNull(System.getenv(name));
        if (s == null) {
            return def;
        }
        try {
            return Long.parseLong(s);
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException(name + ": not an integer: " + s);
        }
    }
}
