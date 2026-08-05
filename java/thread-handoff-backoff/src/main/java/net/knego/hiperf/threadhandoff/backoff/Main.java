package net.knego.hiperf.threadhandoff.backoff;

import java.util.concurrent.atomic.AtomicLong;
import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;
import org.agrona.concurrent.BackoffIdleStrategy;

/**
 * thread-handoff / backoff (Java): paced ping-pong where the responder waits
 * under Agrona's real {@link BackoffIdleStrategy} (spin -> yield ->
 * {@code LockSupport.parkNanos} doubling 1µs -> 1ms, the aeron-go default
 * parameters). {@code parkNanos} overshoots by ~tens of µs on Linux — honest
 * rungs, two orders better than Go's {@code time.Sleep} collapse; that
 * difference is what the grid compares. The requester busy-waits TH_GAP_NS
 * between round trips (untimed) so the responder's ladder ramps, then times
 * the round trip while spinning — the requester is the measurement side, the
 * responder the system-under-test. Emits three handoff_rtt_* lines. See the
 * backoff design spec.
 */
public final class Main {

    private static final String EXPERIMENT = "backoff";

    // aeron-go / Agrona ladder defaults, fixed for cross-language comparability.
    private static final long MAX_SPINS = 10;
    private static final long MAX_YIELDS = 20;
    private static final long MIN_PARK_NS = 1_000;
    private static final long MAX_PARK_NS = 1_000_000;

    public static void main(String[] args) throws InterruptedException {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            AtomicLong req = new AtomicLong(0);  // 0 == empty; token is non-zero 1
            AtomicLong resp = new AtomicLong(0);

            Thread responder = new Thread(() -> {
                BackoffIdleStrategy ladder =
                        new BackoffIdleStrategy(MAX_SPINS, MAX_YIELDS, MIN_PARK_NS, MAX_PARK_NS);
                for (int i = 0; i < total; i++) {
                    while (req.get() == 0) {
                        ladder.idle(0);
                    }
                    ladder.idle(1); // work: reset the ladder
                    req.set(0);
                    resp.set(1);
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measurePaced(cfg, () -> {
                req.set(1);
                while (resp.get() == 0) {
                    Thread.onSpinWait();
                }
                resp.set(0);
            });

            responder.join();
            Handoff.emit(EXPERIMENT, samples);
        } catch (IllegalArgumentException e) {
            System.err.println("thread-handoff-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        }
    }
}
