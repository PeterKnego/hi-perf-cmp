//! The shared warmup + timed measurement loop.
//!
//! Every experiment drives the same loop: discard `warmup` round trips, then
//! time `iterations` round trips into a pre-allocated `Vec<u64>` so allocation
//! never enters the timed path. The per-experiment round-trip operation is
//! supplied as a `FnMut() -> io::Result<()>` closure.

use std::io;
use std::time::Instant;

use crate::config::Config;
use crate::result;
use crate::stats;

/// The focus area for every network-rtt experiment.
pub const FOCUS_AREA: &str = "network-rtt";

/// Warmup + timed loop decoupled from `Config` (used by focus areas whose
/// config type is not `network-rtt`'s `Config`). Allocation happens before the
/// timed loop.
pub fn run_n<F>(warmup: usize, iterations: usize, mut round_trip: F) -> io::Result<Vec<u64>>
where
    F: FnMut() -> io::Result<()>,
{
    for _ in 0..warmup {
        round_trip()?;
    }
    let mut samples = vec![0u64; iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip()?;
        *slot = start.elapsed().as_nanos() as u64;
    }
    Ok(samples)
}

/// Run the measure loop driven by a `network-rtt` `Config`.
pub fn run<F>(cfg: &Config, round_trip: F) -> io::Result<Vec<u64>>
where
    F: FnMut() -> io::Result<()>,
{
    run_n(cfg.warmup, cfg.iterations, round_trip)
}

/// Sort, compute p50/p99/mean, emit the three `rtt_*` lines under `focus_area`.
pub fn emit_rtt_with_focus(focus_area: &str, experiment: &str, samples: &[u64]) {
    let n = samples.len();
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p50 = stats::percentile(&sorted, 50.0);
    let p99 = stats::percentile(&sorted, 99.0);
    let mean = stats::mean(samples);
    result::emit(focus_area, experiment, "rtt_p50", p50, "ns", n);
    result::emit(focus_area, experiment, "rtt_p99", p99, "ns", n);
    result::emit_float(focus_area, experiment, "rtt_mean", mean, "ns", n);
}

/// Emit the three `rtt_*` lines under the `network-rtt` focus area.
pub fn emit_rtt(experiment: &str, samples: &[u64]) {
    emit_rtt_with_focus(FOCUS_AREA, experiment, samples);
}
