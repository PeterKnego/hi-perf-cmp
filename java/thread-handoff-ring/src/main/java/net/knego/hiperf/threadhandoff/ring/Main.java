package net.knego.hiperf.threadhandoff.ring;

import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / ring (Java): bounded SPSC ring, busy-wait, pipelined depth N.
 * Emits one handoff_throughput line.
 */
public final class Main {

    private static final String EXPERIMENT = "ring";

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            long total = (long) cfg.warmup() + cfg.iterations();

            Spsc ring = new Spsc(cfg.ringCap());

            Thread consumer = new Thread(() -> {
                for (long i = 0; i < total; i++) {
                    ring.pop();
                }
            }, "consumer");
            consumer.start();

            // Warmup pushes, then a drain barrier so timing excludes warmup.
            for (int i = 0; i < cfg.warmup(); i++) {
                ring.push(1);
            }
            while (ring.consumed() < cfg.warmup()) {
                Thread.onSpinWait();
            }

            long startNanos = System.nanoTime();
            for (int i = 0; i < cfg.iterations(); i++) {
                ring.push(1);
            }
            while (ring.consumed() < total) {
                Thread.onSpinWait();
            }
            long elapsedNanos = System.nanoTime() - startNanos;

            consumer.join();

            double throughput = cfg.iterations() / (elapsedNanos / 1_000_000_000.0);
            Handoff.emitThroughput(EXPERIMENT, throughput, cfg.iterations());
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
