package net.knego.hiperf.common;

/** Fixed-capacity LOB benchmark config from SMRC_* env vars (plan Appendix A.1). */
public record SmrConfig(int cap, int levels, long tick, long priceMin, int steady, int warmup, int iters) {

    public static SmrConfig fromEnv() {
        int cap = Env.readPositiveInt("SMRC_CAP", 262144);
        int levels = Env.readPositiveInt("SMRC_LEVELS", 1024);
        long tick = Env.readPositiveInt("SMRC_TICK", 1);
        int steady = Env.readPositiveInt("SMRC_STEADY", 60000);
        int warmup = Env.readPositiveInt("SMRC_WARMUP", 10000);
        int iters = Env.readPositiveInt("SMRC_ITERS", 100000);
        long priceMin = readSignedLong("SMRC_PRICE_MIN", 0);
        if (levels > 65535) {
            throw new IllegalArgumentException("SMRC_LEVELS must be <= 65535");
        }
        if (steady > cap || steady > 65535) {
            throw new IllegalArgumentException("SMRC_STEADY must be <= SMRC_CAP and <= 65535");
        }
        if ((long) warmup + iters > cap) {
            throw new IllegalArgumentException("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP");
        }
        return new SmrConfig(cap, levels, tick, priceMin, steady, warmup, iters);
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
