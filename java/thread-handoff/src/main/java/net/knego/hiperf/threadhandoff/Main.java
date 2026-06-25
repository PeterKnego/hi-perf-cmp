package net.knego.hiperf.threadhandoff;

import net.knego.hiperf.common.Result;

/**
 * thread-handoff benchmark (Java) — STUB.
 *
 * <p>Emits one result-contract JSON line on stdout. Real measurement logic to
 * be added later. See docs/result-contract.md for the schema.
 */
public final class Main {
    public static void main(String[] args) {
        // Placeholder result. Replace metric/value/unit/samples once the real
        // thread-handoff benchmark is implemented.
        new Result("thread-handoff", "placeholder", 0, "ns", 0, "stub").emit();
    }
}
