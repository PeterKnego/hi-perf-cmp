//! thread-handoff focus area: `TH_*` config, the warmup + timed round-trip
//! loop, and result emission (three `handoff_rtt_*` latency lines, or one
//! `handoff_throughput` line). Reuses `stats`. Std-only.

use std::env;
use std::time::Instant;

use crate::result;
use crate::stats;

/// Focus area for every thread-handoff experiment.
pub const FOCUS_AREA: &str = "thread-handoff";

/// Parsed, validated thread-handoff configuration (`TH_*`).
#[derive(Debug, Clone)]
pub struct HandoffConfig {
    /// Discarded warmup round-trips / handoffs.
    pub warmup: usize,
    /// Measured round-trips / handoffs (== sample count).
    pub iterations: usize,
    /// Ring capacity (the `ring` experiment only).
    pub ring_cap: usize,
    /// Pacing gap in ns (the `backoff` experiments only): the requester
    /// busy-waits this long between round trips so the responder's idle
    /// ladder ramps. Ignored by the unpaced cells.
    pub gap_ns: u64,
}

impl HandoffConfig {
    /// Read configuration from `TH_*`, applying defaults and validating.
    pub fn from_env() -> Result<HandoffConfig, String> {
        Ok(HandoffConfig {
            warmup: parse_positive("TH_WARMUP", 10_000)?,
            iterations: parse_positive("TH_ITERATIONS", 100_000)?,
            ring_cap: parse_positive("TH_RING_CAP", 1024)?,
            gap_ns: parse_positive("TH_GAP_NS", 100_000)? as u64,
        })
    }
}

fn parse_positive(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) => {
            let value: usize = raw.trim().parse().map_err(|_| {
                format!("{name}: invalid value {raw:?} (expected a positive integer)")
            })?;
            if value == 0 {
                return Err(format!("{name}: must be positive, got 0"));
            }
            Ok(value)
        }
    }
}

/// Run `cfg.warmup` discarded round trips, then time `cfg.iterations` round
/// trips into a pre-allocated buffer, returning one elapsed-ns sample each.
/// `round_trip` performs exactly one ping-pong (infallible — in-process).
pub fn measure<F>(cfg: &HandoffConfig, mut round_trip: F) -> Vec<u64>
where
    F: FnMut(),
{
    for _ in 0..cfg.warmup {
        round_trip();
    }
    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip();
        *slot = start.elapsed().as_nanos() as u64;
    }
    samples
}

/// [`measure`] with a busy-waited gap of `cfg.gap_ns` before every round
/// trip (warmup included), left OUTSIDE the timed window. The gap lets a
/// responder's idle ladder ramp between requests; it is a busy-wait, not a
/// sleep, so the requester's send timing does not inherit the timer
/// overshoot the backoff cells exist to measure.
pub fn measure_paced<F>(cfg: &HandoffConfig, mut round_trip: F) -> Vec<u64>
where
    F: FnMut(),
{
    let gap = std::time::Duration::from_nanos(cfg.gap_ns);
    let wait = |gap: std::time::Duration| {
        let deadline = Instant::now() + gap;
        while Instant::now() < deadline {
            std::hint::spin_loop();
        }
    };
    for _ in 0..cfg.warmup {
        wait(gap);
        round_trip();
    }
    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        wait(gap);
        let start = Instant::now();
        round_trip();
        *slot = start.elapsed().as_nanos() as u64;
    }
    samples
}

/// Sort the round-trip samples and emit the three `handoff_rtt_*` lines.
pub fn emit_handoff(experiment: &str, samples: &[u64]) {
    let n = samples.len();
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    result::emit(
        FOCUS_AREA,
        experiment,
        "handoff_rtt_p50",
        stats::percentile(&sorted, 50.0),
        "ns",
        n,
    );
    result::emit(
        FOCUS_AREA,
        experiment,
        "handoff_rtt_p99",
        stats::percentile(&sorted, 99.0),
        "ns",
        n,
    );
    result::emit_float(
        FOCUS_AREA,
        experiment,
        "handoff_rtt_mean",
        stats::mean(samples),
        "ns",
        n,
    );
}

/// Emit the single `handoff_throughput` line (the `ring` experiment).
pub fn emit_handoff_throughput(experiment: &str, ops_per_sec: f64, samples: usize) {
    result::emit_float(
        FOCUS_AREA,
        experiment,
        "handoff_throughput",
        ops_per_sec,
        "ops_per_sec",
        samples,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_runs_warmup_plus_iterations_and_returns_iterations_samples() {
        let cfg = HandoffConfig {
            warmup: 3,
            iterations: 5,
            ring_cap: 16,
            gap_ns: 0,
        };
        let mut calls = 0usize;
        let samples = measure(&cfg, || calls += 1);
        assert_eq!(samples.len(), 5, "one sample per measured iteration");
        assert_eq!(calls, 8, "warmup (3) + iterations (5) round-trip calls");
    }

    #[test]
    fn measure_paced_keeps_the_gap_between_round_trips_and_outside_samples() {
        let gap_ns = 1_000_000u64; // 1 ms: far above clock granularity
        let cfg = HandoffConfig {
            warmup: 1,
            iterations: 4,
            ring_cap: 16,
            gap_ns,
        };
        let mut starts: Vec<Instant> = Vec::new();
        let samples = measure_paced(&cfg, || starts.push(Instant::now()));
        assert_eq!(samples.len(), 4);
        assert_eq!(starts.len(), 5, "warmup (1) + iterations (4)");
        for pair in starts.windows(2) {
            let d = (pair[1] - pair[0]).as_nanos() as u64;
            assert!(
                d >= gap_ns,
                "round trips only {d}ns apart, want >= {gap_ns}"
            );
        }
        // The gap busy-wait sits OUTSIDE the timed window: each sample is the
        // near-instant round trip, never the millisecond gap.
        for (i, s) in samples.iter().enumerate() {
            assert!(*s < gap_ns, "sample {i} = {s}ns includes the gap");
        }
    }
}
