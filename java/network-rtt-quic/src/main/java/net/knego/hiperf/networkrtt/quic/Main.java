package net.knego.hiperf.networkrtt.quic;

import java.net.InetAddress;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * network-rtt / quic experiment (Java).
 *
 * <p>Measures synchronous ping-pong round-trip latency over a single long-lived
 * QUIC bidirectional stream (one request outstanding at a time), mirroring the
 * TCP methodology. The role is selected by {@code RTT_MODE}:
 * <ul>
 *   <li>{@code loopback} (default) — in-process QUIC echo server on an ephemeral
 *       127.0.0.1 port + client; emits three result-contract JSON lines.</li>
 *   <li>{@code server} — bind a QUIC echo responder on {@code 0.0.0.0} at
 *       {@code RTT_QUIC_PORT} and serve until killed; emits nothing to
 *       stdout.</li>
 *   <li>{@code client} — connect to {@code RTT_HOST:RTT_QUIC_PORT}, measure, and
 *       emit three result-contract JSON lines.</li>
 * </ul>
 * TLS uses an in-memory self-signed cert (server) + skipped verification
 * (client); ALPN is {@code hperf-rtt}. Pure-Java QUIC via Kwik. All diagnostics
 * go to stderr. See docs/result-contract.md.
 */
public final class Main {

    private static final String EXPERIMENT = "quic";

    public static void main(String[] args) {
        Config cfg;
        try {
            cfg = Config.fromEnv();
        } catch (IllegalArgumentException e) {
            System.err.println("network-rtt-quic: invalid configuration: " + e.getMessage());
            System.exit(1);
            return;
        }

        try {
            switch (cfg.mode()) {
                case LOOPBACK -> Measure.emit(EXPERIMENT, QuicRtt.loopback(cfg));
                case SERVER -> QuicRtt.serve(InetAddress.getByName("0.0.0.0"),
                        cfg.quicPort(), cfg.payloadBytes());
                case CLIENT -> Measure.emit(EXPERIMENT, QuicRtt.client(cfg));
            }
        } catch (Exception e) {
            System.err.println("network-rtt-quic: benchmark failed: " + e);
            System.exit(1);
        }
    }
}
