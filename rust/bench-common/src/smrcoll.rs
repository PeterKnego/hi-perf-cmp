//! smr-collections shared harness: env config, a generic timed op loop, and
//! latency emit helpers. The LOB itself lives in `smr-collections-common`.

use crate::{result, stats};
use std::time::Instant;

const FOCUS: &str = "smr-collections";

/// Fixed-capacity LOB benchmark configuration, sourced from `SMRC_*` env vars.
#[derive(Debug, Clone, Copy)]
pub struct SmrConfig {
    pub cap: usize,
    pub levels: u32,
    pub tick: i64,
    pub price_min: i64,
    pub steady: usize,
    pub warmup: usize,
    pub iters: usize,
}

impl SmrConfig {
    pub fn from_env() -> Result<SmrConfig, String> {
        let cap = parse_usize("SMRC_CAP", 262_144)?;
        let levels = parse_usize("SMRC_LEVELS", 1_024)? as u32;
        let tick = parse_i64("SMRC_TICK", 1)?;
        let price_min = parse_i64("SMRC_PRICE_MIN", 0)?; // signed: 0/negative allowed
        let steady = parse_usize("SMRC_STEADY", 60_000)?;
        let warmup = parse_usize("SMRC_WARMUP", 10_000)?;
        let iters = parse_usize("SMRC_ITERS", 100_000)?;
        if tick <= 0 {
            return Err("SMRC_TICK must be > 0".into());
        }
        if levels == 0 || levels > 65_535 {
            return Err("SMRC_LEVELS must be in 1..=65535".into());
        }
        if steady > cap || steady > 65_535 {
            return Err("SMRC_STEADY must be <= SMRC_CAP and <= 65535".into());
        }
        if warmup + iters > cap {
            return Err("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP".into());
        }
        Ok(SmrConfig {
            cap,
            levels,
            tick,
            price_min,
            steady,
            warmup,
            iters,
        })
    }
}

fn parse_usize(key: &str, default: usize) -> Result<usize, String> {
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(s) => {
            let v: usize = s
                .trim()
                .parse()
                .map_err(|_| format!("{key}: not an integer: {s:?}"))?;
            if v == 0 {
                return Err(format!("{key}: must be positive"));
            }
            Ok(v)
        }
    }
}

fn parse_i64(key: &str, default: i64) -> Result<i64, String> {
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("{key}: not an integer: {s:?}")),
    }
}

/// Run `warmup` discarded ops, then time `iters` ops into a preallocated Vec (ns).
pub fn measure<F: FnMut()>(warmup: usize, iters: usize, mut op: F) -> Vec<u64> {
    for _ in 0..warmup {
        op();
    }
    let mut samples = vec![0u64; iters];
    for s in samples.iter_mut() {
        let start = Instant::now();
        op();
        *s = start.elapsed().as_nanos() as u64;
    }
    samples
}

/// Sort a copy, emit `{prefix}_p50`/`_p99` (u64 ns) and `{prefix}_mean` (f64 ns).
pub fn emit_latency(experiment: &str, prefix: &str, samples: &[u64]) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = samples.len();
    result::emit(
        FOCUS,
        experiment,
        &format!("{prefix}_p50"),
        stats::percentile(&sorted, 50.0),
        "ns",
        n,
    );
    result::emit(
        FOCUS,
        experiment,
        &format!("{prefix}_p99"),
        stats::percentile(&sorted, 99.0),
        "ns",
        n,
    );
    result::emit_float(
        FOCUS,
        experiment,
        &format!("{prefix}_mean"),
        stats::mean(samples),
        "ns",
        n,
    );
}

/// Emit one integer metric line (e.g. `snapshot_bytes`).
pub fn emit_int(experiment: &str, metric: &str, value: u64, unit: &str, samples: usize) {
    result::emit(FOCUS, experiment, metric, value, unit, samples);
}

/// Emit one fractional metric line (e.g. `snapshot_throughput`).
pub fn emit_float(experiment: &str, metric: &str, value: f64, unit: &str, samples: usize) {
    result::emit_float(FOCUS, experiment, metric, value, unit, samples);
}
