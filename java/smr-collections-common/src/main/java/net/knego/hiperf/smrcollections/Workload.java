package net.knego.hiperf.smrcollections;

/** Deterministic workload RNG + draws (plan Appendix A.2/A.3), identical across languages. */
public final class Workload {

    public static final long SEED = 0x123456789ABCDEF0L;

    private Workload() {}

    public static final class SplitMix {
        private long state;

        public SplitMix(long seed) {
            this.state = seed;
        }

        public long next() {
            state += 0x9E3779B97F4A7C15L;
            long z = state;
            z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
            z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
            return z ^ (z >>> 31);
        }
    }

    public static final class Insert {
        public long orderId, price, qty;
        public byte side;
    }

    public static final class Update {
        public long orderId, fillQty;
    }

    /** i-th insert; fills the reusable {@code out}. n is the u64 level count. */
    public static void nextInsert(SplitMix rng, int i, int nLevels, long tick, long priceMin, Insert out) {
        long r1 = rng.next();
        long r2 = rng.next();
        long t = Long.remainderUnsigned(r1, nLevels);
        out.side = (byte) ((r1 >>> 32) & 1);
        out.orderId = i + 1;
        out.price = priceMin + t * tick;
        out.qty = 1 + Long.remainderUnsigned(r2, 1000);
    }

    public static void nextUpdate(SplitMix rng, int n, Update out) {
        long u = rng.next();
        out.orderId = Long.remainderUnsigned(u, n) + 1;
        out.fillQty = 1 + Long.remainderUnsigned(u >>> 32, 100);
    }
}
