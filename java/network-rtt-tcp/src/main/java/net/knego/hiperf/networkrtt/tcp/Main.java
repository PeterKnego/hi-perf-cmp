package net.knego.hiperf.networkrtt.tcp;

import java.net.InetAddress;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * network-rtt / tcp experiment (Java).
 *
 * <p>Measures synchronous ping-pong round-trip latency over TCP (one request
 * outstanding at a time). The role is selected by {@code RTT_MODE}:
 * <ul>
 *   <li>{@code loopback} (default) — in-process echo server on an ephemeral
 *       127.0.0.1 port + client; emits three result-contract JSON lines.</li>
 *   <li>{@code server} — bind a TCP echo responder on {@code 0.0.0.0} at
 *       {@code RTT_TCP_PORT} and serve until killed; emits nothing to
 *       stdout.</li>
 *   <li>{@code client} — connect to {@code RTT_HOST:RTT_TCP_PORT}, measure, and
 *       emit three result-contract JSON lines.</li>
 * </ul>
 * All diagnostics go to stderr. See docs/result-contract.md.
 */
public final class Main {

    private static final String EXPERIMENT = "tcp";

    public static void main(String[] args) {
        Config cfg;
        try {
            cfg = Config.fromEnv();
        } catch (IllegalArgumentException e) {
            System.err.println("network-rtt-tcp: invalid configuration: " + e.getMessage());
            System.exit(1);
            return;
        }

        try {
            switch (cfg.mode()) {
                case LOOPBACK -> Measure.emit(EXPERIMENT, TcpRtt.loopback(cfg));
                case SERVER -> TcpRtt.serve(InetAddress.getByName("0.0.0.0"),
                        cfg.tcpPort(), cfg.payloadBytes());
                case CLIENT -> Measure.emit(EXPERIMENT, TcpRtt.client(cfg));
            }
        } catch (Exception e) {
            System.err.println("network-rtt-tcp: benchmark failed: " + e);
            System.exit(1);
        }
    }
}
