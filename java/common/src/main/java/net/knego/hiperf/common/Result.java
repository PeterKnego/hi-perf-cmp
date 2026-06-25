package net.knego.hiperf.common;

/**
 * One benchmark measurement in the shared cross-language format, plus a helper
 * to emit it as a single JSON line on stdout. See docs/result-contract.md.
 *
 * <p>{@code language} is fixed to {@code "java"}. {@code experiment} is the
 * variant under the focus area (e.g. {@code tcp}, {@code udp}, {@code quic}, or
 * {@code placeholder} for stubs). JSON is hand-rendered to keep the skeleton
 * dependency-free.
 */
public record Result(
        String focusArea,
        String experiment,
        String metric,
        double value,
        String unit,
        long samples,
        String notes) {

    /** Writes this result as a single JSON line to stdout. */
    public void emit() {
        StringBuilder sb = new StringBuilder(160);
        sb.append('{')
                .append("\"language\":\"java\",")
                .append("\"focus_area\":").append(quote(focusArea)).append(',')
                .append("\"experiment\":").append(quote(experiment)).append(',')
                .append("\"metric\":").append(quote(metric)).append(',')
                .append("\"value\":").append(value).append(',')
                .append("\"unit\":").append(quote(unit)).append(',')
                .append("\"samples\":").append(samples).append(',')
                .append("\"notes\":").append(quote(notes))
                .append('}');
        System.out.println(sb);
    }

    private static String quote(String s) {
        if (s == null) {
            return "\"\"";
        }
        StringBuilder sb = new StringBuilder(s.length() + 2);
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> sb.append(c);
            }
        }
        sb.append('"');
        return sb.toString();
    }
}
