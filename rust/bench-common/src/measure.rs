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

/// Run `cfg.warmup` discarded round trips, then time `cfg.iterations` round
/// trips into a pre-allocated buffer, returning one elapsed-ns sample each.
///
/// `round_trip` performs exactly one ping-pong; its timing is what we measure.
/// All allocation happens before the timed loop.
pub fn run<F>(cfg: &Config, mut round_trip: F) -> io::Result<Vec<u64>>
where
    F: FnMut() -> io::Result<()>,
{
    // Warmup — timings discarded.
    for _ in 0..cfg.warmup {
        round_trip()?;
    }

    // Pre-allocate the sample buffer so allocation never enters the timed path.
    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip()?;
        *slot = start.elapsed().as_nanos() as u64;
    }
    Ok(samples)
}

/// Sort `samples`, compute p50/p99/mean, and emit the three result lines for an
/// experiment under the `network-rtt` focus area.
pub fn emit_rtt(experiment: &str, samples: &[u64]) {
    let n = samples.len();
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();

    let p50 = stats::percentile(&sorted, 50.0);
    let p99 = stats::percentile(&sorted, 99.0);
    let mean = stats::mean(samples);

    result::emit(FOCUS_AREA, experiment, "rtt_p50", p50, "ns", n);
    result::emit(FOCUS_AREA, experiment, "rtt_p99", p99, "ns", n);
    result::emit_float(FOCUS_AREA, experiment, "rtt_mean", mean, "ns", n);
}
