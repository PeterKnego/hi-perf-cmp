package net.knego.hiperf.common;

/** thread-handoff configuration from the {@code TH_*} env vars; positive integers. */
public record HandoffConfig(int warmup, int iterations, int ringCap) {

    public static HandoffConfig fromEnv() {
        return new HandoffConfig(
                Env.readPositiveInt("TH_WARMUP", 10000),
                Env.readPositiveInt("TH_ITERATIONS", 100000),
                Env.readPositiveInt("TH_RING_CAP", 1024));
    }
}
