package net.knego.hiperf.threadhandoff.spin;

import java.util.concurrent.atomic.AtomicLong;
import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / spin (Java): single-slot atomic handoff, busy-wait. Lowest
 * latency, burns a core. Emits three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "spin";

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            AtomicLong req = new AtomicLong(0);  // 0 == empty; token is non-zero 1
            AtomicLong resp = new AtomicLong(0);

            Thread responder = new Thread(() -> {
                for (int i = 0; i < total; i++) {
                    while (req.get() == 0) {
                        Thread.onSpinWait();
                    }
                    req.set(0);
                    resp.set(1);
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measure(cfg, () -> {
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
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            System.err.println("thread-handoff-" + EXPERIMENT + ": interrupted: " + e.getMessage());
            System.exit(1);
        }
    }
}
