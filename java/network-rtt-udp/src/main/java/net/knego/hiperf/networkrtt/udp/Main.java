package net.knego.hiperf.networkrtt.udp;

import java.net.InetAddress;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * network-rtt / udp experiment (Java).
 *
 * <p>Measures synchronous ping-pong round-trip latency over UDP (one request
 * outstanding at a time, a receive timeout is a hard error). The role is
 * selected by {@code RTT_MODE}:
 * <ul>
 *   <li>{@code loopback} (default) — in-process echo server on an ephemeral
 *       127.0.0.1 port + client; emits three result-contract JSON lines.</li>
 *   <li>{@code server} — bind a UDP echo responder on {@code 0.0.0.0} at
 *       {@code RTT_UDP_PORT} and serve until killed; emits nothing to
 *       stdout.</li>
 *   <li>{@code client} — connect to {@code RTT_HOST:RTT_UDP_PORT}, measure, and
 *       emit three result-contract JSON lines.</li>
 * </ul>
 * All diagnostics go to stderr. See docs/result-contract.md.
 */
public final class Main {

    private static final String EXPERIMENT = "udp";

    public static void main(String[] args) {
        Config cfg;
        try {
            cfg = Config.fromEnv();
        } catch (IllegalArgumentException e) {
            System.err.println("network-rtt-udp: invalid configuration: " + e.getMessage());
            System.exit(1);
            return;
        }

        try {
            switch (cfg.mode()) {
                case LOOPBACK -> Measure.emit(EXPERIMENT, UdpRtt.loopback(cfg));
                case SERVER -> UdpRtt.serve(InetAddress.getByName("0.0.0.0"),
                        cfg.udpPort(), cfg.payloadBytes());
                case CLIENT -> Measure.emit(EXPERIMENT, UdpRtt.client(cfg));
            }
        } catch (Exception e) {
            System.err.println("network-rtt-udp: benchmark failed: " + e);
            System.exit(1);
        }
    }
}
