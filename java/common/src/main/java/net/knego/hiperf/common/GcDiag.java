package net.knego.hiperf.common;

import java.util.ArrayList;
import java.util.List;

/**
 * Env-gated ({@code SMRC_GC_DIAG}) wall-clock tracker for large timed ops —
 * the correlation hook for GC-pause attribution of {@code writer_max} in the
 * {@code live_*} cells. When enabled, {@link #record} stamps every op at or
 * above a threshold ({@code SMRC_GC_DIAG_THRESH_NS}, default 100 µs) with
 * {@link System#currentTimeMillis} and buffers it; nothing is printed inside
 * the loop. The stamps line up against a GC log written with {@code time}
 * decorations; a stamp is taken when the op <em>ends</em>, so a pause
 * containing the op lies in {@code [at_epoch_ms - stall_ns/1e6, at_epoch_ms]}.
 *
 * <p>Diagnostic only: off (a single dead branch in the hot loop) unless the
 * env var is set, and all output goes to <b>stderr</b> — stdout stays
 * result-contract only. Numbers from a diag-enabled run are labelled
 * diagnostics, never grid figures.
 */
public final class GcDiag {
    /** Bound on buffered events — a run that stalls more than this is broken anyway. */
    private static final int MAX_EVENTS = 10_000;

    private final boolean enabled;
    private final long thresholdNs;
    private final List<long[]> events = new ArrayList<>(); // {iter, ns, epochMs}
    private long maxNs = -1;
    private long atEpochMs;
    private int atIter;

    public GcDiag() {
        this(System.getenv("SMRC_GC_DIAG") != null, threshFromEnv());
    }

    GcDiag(boolean enabled, long thresholdNs) {
        this.enabled = enabled;
        this.thresholdNs = thresholdNs;
    }

    GcDiag(boolean enabled) {
        this(enabled, 100_000);
    }

    private static long threshFromEnv() {
        String v = System.getenv("SMRC_GC_DIAG_THRESH_NS");
        return v == null ? 100_000 : Long.parseLong(v);
    }

    /** Record one timed op. Hot-loop safe: wall clock read only on threshold/new-max ops. */
    public void record(int iter, long ns) {
        if (!enabled || (ns < thresholdNs && ns <= maxNs)) {
            return;
        }
        long now = System.currentTimeMillis();
        if (ns >= thresholdNs && events.size() < MAX_EVENTS) {
            events.add(new long[] {iter, ns, now});
        }
        if (ns > maxNs) {
            maxNs = ns;
            atEpochMs = now;
            atIter = iter;
        }
    }

    /** One line per buffered threshold event, in record order. */
    public List<String> eventLines(String experiment) {
        List<String> out = new ArrayList<>(events.size());
        for (long[] e : events) {
            out.add("gc-diag " + experiment + " stall_ns=" + e[1] + " at_epoch_ms=" + e[2]
                    + " iter=" + e[0]);
        }
        return out;
    }

    /** The max-op summary line, or null when disabled or nothing was recorded. */
    public String line(String experiment) {
        if (!enabled || maxNs < 0) {
            return null;
        }
        return "gc-diag " + experiment + " writer_max_ns=" + maxNs + " at_epoch_ms=" + atEpochMs
                + " iter=" + atIter;
    }

    /** Print every event line plus the max summary to stderr (no-op when disabled). */
    public void emit(String experiment) {
        for (String l : eventLines(experiment)) {
            System.err.println(l);
        }
        String l = line(experiment);
        if (l != null) {
            System.err.println(l);
        }
    }
}
