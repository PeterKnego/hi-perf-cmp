package net.knego.hiperf.networkrtt;

import java.net.InetAddress;
import java.util.Arrays;
import net.knego.hiperf.common.Result;

/**
 * network-rtt benchmark (Java).
 *
 * <p>Measures synchronous ping-pong round-trip latency for both TCP and UDP
 * (one request outstanding at a time). The role is selected by {@code RTT_MODE}:
 * <ul>
 *   <li>{@code loopback} (default) — in-process echo server on an ephemeral
 *       127.0.0.1 port + client; emits six result-contract JSON lines.</li>
 *   <li>{@code server} — bind TCP+UDP echo responders on {@code 0.0.0.0} at the
 *       configured ports and serve until killed; emits nothing to stdout.</li>
 *   <li>{@code client} — connect to {@code RTT_HOST} on both ports, measure, and
 *       emit six result-contract JSON lines.</li>
 * </ul>
 * All diagnostics go to stderr. See docs/result-contract.md and the network-rtt
 * design spec.
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
            switch (cfg.mode()) {
                case LOOPBACK -> runLoopback(cfg);
                case SERVER -> runServer(cfg);
                case CLIENT -> runClient(cfg);
            }
        } catch (Exception e) {
            System.err.println("network-rtt: benchmark failed: " + e);
            System.exit(1);
        }
    }

    private static void runLoopback(Config cfg) throws Exception {
        long[] tcp = TcpRtt.loopback(cfg);
        long[] udp = UdpRtt.loopback(cfg);
        emit("tcp", tcp, cfg.iterations());
        emit("udp", udp, cfg.iterations());
    }

    /** Runs TCP and UDP responders on {@code 0.0.0.0} forever; emits nothing to stdout. */
    private static void runServer(Config cfg) throws Exception {
        InetAddress bind = InetAddress.getByName("0.0.0.0");
        Thread udpThread = new Thread(() -> {
            try {
                UdpRtt.serve(bind, cfg.udpPort(), cfg.payloadBytes());
            } catch (Exception e) {
                System.err.println("network-rtt: UDP responder failed: " + e);
                System.exit(1);
            }
        }, "udp-responder");
        udpThread.start();
        // TCP serve blocks forever (until the process is killed).
        TcpRtt.serve(bind, cfg.tcpPort(), cfg.payloadBytes());
    }

    private static void runClient(Config cfg) throws Exception {
        long[] tcp = TcpRtt.client(cfg);
        long[] udp = UdpRtt.client(cfg);
        emit("tcp", tcp, cfg.iterations());
        emit("udp", udp, cfg.iterations());
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
