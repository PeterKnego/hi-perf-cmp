package net.knego.hiperf.networkrtt;

/**
 * Benchmark configuration read from environment variables, with the
 * cross-language default values. Non-positive or non-numeric values are a
 * hard error.
 *
 * <p>{@code RTT_MODE} selects the role:
 * <ul>
 *   <li>{@code loopback} (default) — in-process echo server + client over an
 *       ephemeral 127.0.0.1 port; emits the six result lines.</li>
 *   <li>{@code server} — bind TCP and UDP echo responders on {@code 0.0.0.0}
 *       at the configured ports and serve until killed; emits nothing.</li>
 *   <li>{@code client} — connect to {@code RTT_HOST} on both ports, measure,
 *       and emit the six result lines.</li>
 * </ul>
 */
record Config(
        Mode mode,
        String host,
        int tcpPort,
        int udpPort,
        int payloadBytes,
        int warmup,
        int iterations) {

    /** Operating mode selected by {@code RTT_MODE}. */
    enum Mode {
        LOOPBACK,
        SERVER,
        CLIENT
    }

    static Config fromEnv() {
        Mode mode = readMode();
        String host = trimmedOrNull(System.getenv("RTT_HOST"));
        if (mode == Mode.CLIENT && host == null) {
            throw new IllegalArgumentException("RTT_HOST is required in client mode");
        }
        return new Config(
                mode,
                host,
                readPositiveInt("RTT_TCP_PORT", 9100),
                readPositiveInt("RTT_UDP_PORT", 9101),
                readPositiveInt("RTT_PAYLOAD_BYTES", 64),
                readPositiveInt("RTT_WARMUP", 10000),
                readPositiveInt("RTT_ITERATIONS", 100000));
    }

    private static Mode readMode() {
        String raw = trimmedOrNull(System.getenv("RTT_MODE"));
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

    private static String trimmedOrNull(String raw) {
        if (raw == null) {
            return null;
        }
        String t = raw.trim();
        return t.isEmpty() ? null : t;
    }

    private static int readPositiveInt(String name, int def) {
        String raw = System.getenv(name);
        if (raw == null || raw.isEmpty()) {
            return def;
        }
        int value;
        try {
            value = Integer.parseInt(raw.trim());
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException(
                    name + " must be a positive integer, got: " + raw);
        }
        if (value <= 0) {
            throw new IllegalArgumentException(
                    name + " must be a positive integer, got: " + raw);
        }
        return value;
    }
}
