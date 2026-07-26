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
    /// Orders per CoW chunk (CowBook only).
    pub chunk: usize,
    /// Timed writer ops in the live_* experiments.
    pub live_iters: usize,
    /// Ops between snapshot triggers in the live_* experiments.
    pub snap_every: usize,
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
        let chunk = parse_usize("SMRC_CHUNK", 4_096)?;
        let live_iters = parse_usize("SMRC_LIVE_ITERS", 200_000)?;
        let snap_every = parse_usize("SMRC_SNAP_EVERY", 20_000)?;
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
        if chunk > cap {
            return Err("SMRC_CHUNK must be <= SMRC_CAP".into());
        }
        if snap_every > live_iters {
            return Err("SMRC_SNAP_EVERY must be <= SMRC_LIVE_ITERS".into());
        }
        Ok(SmrConfig {
            cap,
            levels,
            tick,
            price_min,
            steady,
            warmup,
            iters,
            chunk,
            live_iters,
            snap_every,
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

#[cfg(test)]
mod tests {
    use super::*;

    // Serialize env-var mutation: cargo runs tests in parallel within a binary.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn smrc_new_fields_default() {
        let _g = ENV_LOCK.lock().unwrap();
        for k in ["SMRC_CHUNK", "SMRC_LIVE_ITERS", "SMRC_SNAP_EVERY"] {
            unsafe { std::env::remove_var(k) };
        }
        let c = SmrConfig::from_env().expect("defaults parse");
        assert_eq!(c.chunk, 4096);
        assert_eq!(c.live_iters, 200_000);
        assert_eq!(c.snap_every, 20_000);
    }

    #[test]
    fn smrc_snap_every_must_not_exceed_live_iters() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("SMRC_LIVE_ITERS", "1000");
            std::env::set_var("SMRC_SNAP_EVERY", "2000");
        }
        assert!(SmrConfig::from_env().is_err());
        unsafe {
            std::env::remove_var("SMRC_LIVE_ITERS");
            std::env::remove_var("SMRC_SNAP_EVERY");
        }
    }

    #[test]
    fn smrc_chunk_must_not_exceed_cap() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SMRC_CHUNK", "999999999") };
        assert!(SmrConfig::from_env().is_err());
        unsafe { std::env::remove_var("SMRC_CHUNK") };
    }
}
