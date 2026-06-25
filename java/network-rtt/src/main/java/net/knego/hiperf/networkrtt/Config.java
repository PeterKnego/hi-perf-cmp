package net.knego.hiperf.networkrtt;

/**
 * Benchmark configuration read from environment variables, with the
 * cross-language default values. Non-positive or non-numeric values are a
 * hard error.
 */
record Config(int payloadBytes, int warmup, int iterations) {

    static Config fromEnv() {
        return new Config(
                readPositiveInt("RTT_PAYLOAD_BYTES", 64),
                readPositiveInt("RTT_WARMUP", 10000),
                readPositiveInt("RTT_ITERATIONS", 100000));
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
