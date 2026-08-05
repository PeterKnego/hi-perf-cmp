package net.knego.hiperf.common;

/**
 * thread-handoff configuration from the {@code TH_*} env vars; positive
 * integers. {@code gapNs} paces the backoff cells (the requester busy-waits
 * this long between round trips so the responder's idle ladder ramps);
 * ignored by the unpaced cells.
 */
public record HandoffConfig(int warmup, int iterations, int ringCap, long gapNs) {

    public static HandoffConfig fromEnv() {
        return new HandoffConfig(
                Env.readPositiveInt("TH_WARMUP", 10000),
                Env.readPositiveInt("TH_ITERATIONS", 100000),
                Env.readPositiveInt("TH_RING_CAP", 1024),
                Env.readPositiveInt("TH_GAP_NS", 100000));
    }
}
