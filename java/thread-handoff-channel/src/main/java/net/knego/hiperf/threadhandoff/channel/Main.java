package net.knego.hiperf.threadhandoff.channel;

import java.util.concurrent.SynchronousQueue;
import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / channel (Java): a {@link SynchronousQueue} rendezvous in each
 * direction — the idiomatic blocking-queue handoff. The token is a constant
 * cached {@link Long} (a reused box), so there is no per-handoff allocation.
 * Three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "channel";

    /** Constant cached box (Long.valueOf caches small values) — reused, never re-boxed. */
    private static final Long TOKEN = 1L;

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            SynchronousQueue<Long> req = new SynchronousQueue<>();
            SynchronousQueue<Long> resp = new SynchronousQueue<>();

            Thread responder = new Thread(() -> {
                try {
                    for (int i = 0; i < total; i++) {
                        resp.put(req.take());
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measure(cfg, () -> {
                try {
                    req.put(TOKEN);
                    resp.take();
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new RuntimeException("interrupted during handoff", e);
                }
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
