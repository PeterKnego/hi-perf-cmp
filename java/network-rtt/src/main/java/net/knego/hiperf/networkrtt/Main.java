package net.knego.hiperf.networkrtt;

import java.util.Arrays;
import net.knego.hiperf.common.Result;

/**
 * network-rtt benchmark (Java).
 *
 * <p>Measures synchronous ping-pong round-trip latency over loopback for both
 * TCP and UDP, using an in-process echo server and a single client connection
 * (one request outstanding at a time). Emits six result-contract JSON lines on
 * stdout; all diagnostics go to stderr. See docs/result-contract.md and the
 * network-rtt design spec.
 */
public final class Main {

    private static final String FOCUS_AREA = "network-rtt";
    private static final String UNIT = "ns";

    public static void main(String[] args) {
        Config cfg;
        try {
            cfg = Config.fromEnv();
        } catch (IllegalArgumentException e) {
            System.err.println("network-rtt: invalid configuration: " + e.getMessage());
            System.exit(1);
            return;
        }

        try {
            long[] tcp = TcpRtt.measure(cfg);
            long[] udp = UdpRtt.measure(cfg);
            emit("tcp", tcp, cfg.iterations());
            emit("udp", udp, cfg.iterations());
        } catch (Exception e) {
            System.err.println("network-rtt: benchmark failed: " + e);
            System.exit(1);
        }
    }

    private static void emit(String transport, long[] samples, int iterations) {
        Arrays.sort(samples);
        long p50 = Stats.percentile(samples, 50);
        long p99 = Stats.percentile(samples, 99);
        double mean = Stats.mean(samples);
        new Result(FOCUS_AREA, transport + "_rtt_p50", p50, UNIT, iterations, "").emit();
        new Result(FOCUS_AREA, transport + "_rtt_p99", p99, UNIT, iterations, "").emit();
        new Result(FOCUS_AREA, transport + "_rtt_mean", mean, UNIT, iterations, "").emit();
    }
}
