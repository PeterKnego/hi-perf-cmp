package net.knego.hiperf.common;

/**
 * Benchmark configuration read from environment variables, with the
 * cross-language default values. Non-positive or non-numeric values are a
 * hard error.
 *
 * <p>{@code RTT_MODE} selects the role of each network-rtt experiment artifact:
 * <ul>
 *   <li>{@code loopback} (default) — in-process echo server + client over an
 *       ephemeral 127.0.0.1 port; emits the three result lines.</li>
 *   <li>{@code server} — bind this experiment's echo responder on
 *       {@code 0.0.0.0} at the configured port and serve until killed; emits
 *       nothing to stdout.</li>
 *   <li>{@code client} — connect to {@code RTT_HOST} on this experiment's port,
 *       measure, and emit the three result lines.</li>
 * </ul>
 */
public record Config(
        Mode mode,
        String host,
        int tcpPort,
        int udpPort,
        int quicPort,
        int payloadBytes,
        int warmup,
        int iterations) {

    /** Operating mode selected by {@code RTT_MODE}. */
    public enum Mode {
        LOOPBACK,
        SERVER,
        CLIENT
    }

    public static Config fromEnv() {
        Mode mode = readMode();
        String host = Env.trimmedOrNull(System.getenv("RTT_HOST"));
        if (mode == Mode.CLIENT && host == null) {
            throw new IllegalArgumentException("RTT_HOST is required in client mode");
        }
        return new Config(
                mode,
                host,
                Env.readPositiveInt("RTT_TCP_PORT", 9100),
                Env.readPositiveInt("RTT_UDP_PORT", 9101),
                Env.readPositiveInt("RTT_QUIC_PORT", 9102),
                Env.readPositiveInt("RTT_PAYLOAD_BYTES", 64),
                Env.readPositiveInt("RTT_WARMUP", 10000),
                Env.readPositiveInt("RTT_ITERATIONS", 100000));
    }

    private static Mode readMode() {
        String raw = Env.trimmedOrNull(System.getenv("RTT_MODE"));
        if (raw == null) {
            return Mode.LOOPBACK;
        }
        switch (raw.toLowerCase()) {
            case "loopback":
                return Mode.LOOPBACK;
            case "server":
                return Mode.SERVER;
            case "client":
                return Mode.CLIENT;
            default:
                throw new IllegalArgumentException(
                        "RTT_MODE must be one of loopback|server|client, got: " + raw);
        }
    }
}
