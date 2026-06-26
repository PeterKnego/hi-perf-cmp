package net.knego.hiperf.common;

/** Shared env-var parsing helpers used by the per-focus-area config types. */
final class Env {

    private Env() {}

    /** The trimmed value of an env var, or {@code null} if unset/blank. */
    static String trimmedOrNull(String raw) {
        if (raw == null) {
            return null;
        }
        String t = raw.trim();
        return t.isEmpty() ? null : t;
    }

    /**
     * Parse env var {@code name} as a positive integer, returning {@code def}
     * when unset/empty. Non-numeric or non-positive values are a hard error.
     */
    static int readPositiveInt(String name, int def) {
        String raw = System.getenv(name);
        if (raw == null || raw.isEmpty()) {
            return def;
        }
        int value;
        try {
            value = Integer.parseInt(raw.trim());
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException(name + " must be a positive integer, got: " + raw);
        }
        if (value <= 0) {
            throw new IllegalArgumentException(name + " must be a positive integer, got: " + raw);
        }
        return value;
    }
}
