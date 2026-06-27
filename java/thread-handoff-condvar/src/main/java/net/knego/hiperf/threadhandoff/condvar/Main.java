package net.knego.hiperf.threadhandoff.condvar;

import java.util.concurrent.atomic.AtomicBoolean;
import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / condvar (Java): a monitor (synchronized + wait/notify)
 * rendezvous. Isolates park/unpark + signal cost. Three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "condvar";

    /**
     * One-slot mailbox carrying a single long token, with a bounded spin
     * fast-path over a monitor (park/unpark) slow-path. In a tight ping-pong
     * the counterpart usually responds within nanoseconds, so {@code recv}
     * busy-checks the {@code full} flag for a bounded budget before falling
     * back to the real {@code wait()}. The {@link AtomicBoolean} publishes the
     * value (its release/acquire make the plain {@code value} write visible);
     * the monitor predicate still re-checks {@code full} under the lock, so the
     * parking fallback remains correct (no lost wakeups).
     */
    static final class Mailbox {
        /** Bounded spin budget before parking (kept finite: this is NOT the spin cell). */
        private static final int SPIN_BUDGET = 1000;

        private final Object lock = new Object();
        private final AtomicBoolean full = new AtomicBoolean(false);
        private long value;

        void send(long v) {
            value = v;
            full.set(true); // release: publishes `value` to a spinning receiver
            synchronized (lock) {
                lock.notify(); // wake a parked receiver (slow-path fallback)
            }
        }

        long recv() {
            // Bounded spin fast-path: acquire `full` cheaply, no monitor entry.
            for (int i = 0; i < SPIN_BUDGET; i++) {
                if (full.get()) {
                    full.set(false);
                    return value;
                }
                Thread.onSpinWait();
            }
            // Slow-path fallback: real monitor wait (predicate re-checked under lock).
            synchronized (lock) {
                while (!full.get()) {
                    try {
                        lock.wait();
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        throw new RuntimeException("interrupted while waiting for handoff", e);
                    }
                }
            }
            full.set(false);
            return value;
        }
    }

    public static void main(String[] args) {
        try {
            HandoffConfig cfg = HandoffConfig.fromEnv();
            int total = cfg.warmup() + cfg.iterations();

            Mailbox req = new Mailbox();
            Mailbox resp = new Mailbox();

            Thread responder = new Thread(() -> {
                for (int i = 0; i < total; i++) {
                    resp.send(req.recv());
                }
            }, "responder");
            responder.start();

            long[] samples = Handoff.measure(cfg, () -> {
                req.send(1);
                resp.recv();
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
