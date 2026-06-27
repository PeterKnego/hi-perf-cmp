package net.knego.hiperf.threadhandoff.condvar;

import net.knego.hiperf.common.Handoff;
import net.knego.hiperf.common.HandoffConfig;

/**
 * thread-handoff / condvar (Java): a monitor (synchronized + wait/notify)
 * rendezvous. Isolates park/unpark + signal cost. Three handoff_rtt_* lines.
 */
public final class Main {

    private static final String EXPERIMENT = "condvar";

    /** One-slot monitor mailbox carrying a single long token. */
    static final class Mailbox {
        private long value;
        private boolean full;

        synchronized void send(long v) {
            value = v;
            full = true;
            notify();
        }

        synchronized long recv() {
            while (!full) {
                try {
                    wait();
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new RuntimeException("interrupted while waiting for handoff", e);
                }
            }
            full = false;
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
