package net.knego.hiperf.networkrtt;

import net.knego.hiperf.common.Result;

/**
 * network-rtt benchmark (Java) — STUB.
 *
 * <p>Emits one result-contract JSON line on stdout. Real measurement logic to
 * be added later. See docs/result-contract.md for the schema.
 */
public final class Main {
    public static void main(String[] args) {
        // Placeholder result. Replace metric/value/unit/samples once the real
        // round-trip-time benchmark is implemented.
        new Result("network-rtt", "placeholder", 0, "ns", 0, "stub").emit();
    }
}
