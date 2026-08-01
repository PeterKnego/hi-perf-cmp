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
    /// Commands per write-txn in the ultima batched-apply cells.
    pub apply_batch: usize,
    /// When true, the ultima batched cells open their tables once per batch
    /// via `open_tables3`/`open_tables2` (issue #20) instead of re-opening per
    /// command. Per-command work is identical; this isolates the re-open cost.
    pub multi_table: bool,
    /// Timed writer ops in the live_* experiments.
    pub live_iters: usize,
    /// Ops between snapshot triggers in the live_* experiments.
    pub snap_every: usize,
    /// Order-to-trade ratio in basis points: the share of departures that are
    /// fills rather than cancels. 100 = 1 %, the real-exchange figure.
    pub otr_bps: u64,
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
        let apply_batch = parse_usize("SMRC_APPLY_BATCH", 64)?;
        let multi_table = std::env::var("SMRC_MULTI_TABLE")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        let live_iters = parse_usize("SMRC_LIVE_ITERS", 200_000)?;
        let snap_every = parse_usize("SMRC_SNAP_EVERY", 20_000)?;
        let otr_bps = parse_usize_allow_zero("SMRC_OTR_BPS", 100)? as u64;
        if tick <= 0 {
            return Err("SMRC_TICK must be > 0".into());
        }
        if levels == 0 || levels > 65_535 {
            return Err("SMRC_LEVELS must be in 1..=65535".into());
        }
        if steady > cap || steady > 65_535 {
            return Err("SMRC_STEADY must be <= SMRC_CAP and <= 65535".into());
        }
        if otr_bps > 10_000 {
            return Err(format!("SMRC_OTR_BPS must be in 0..=10000 (got {otr_bps})"));
        }
        if chunk > cap {
            return Err("SMRC_CHUNK must be <= SMRC_CAP".into());
        }
        if apply_batch == 0 || apply_batch > iters {
            return Err(format!(
                "SMRC_APPLY_BATCH must be in 1..={iters} (got {apply_batch})"
            ));
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
            apply_batch,
            multi_table,
            live_iters,
            snap_every,
            otr_bps,
        })
    }

    /// Cells that bump-allocate (no free list) need a pool slot for every op
    /// they will ever run. Churn cells recycle slots and must NOT call this.
    pub fn require_bump_capacity(&self) -> Result<(), String> {
        if self.warmup + self.iters > self.cap {
            return Err("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP".into());
        }
        Ok(())
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

/// Like `parse_usize`, but `0` is a legal value. Only `SMRC_OTR_BPS` uses
/// this: a pure-cancel run (`SMRC_OTR_BPS=0`) is a legitimate experiment, and
/// the other `SMRC_*` knobs' zero-rejection stays as-is.
fn parse_usize_allow_zero(key: &str, default: usize) -> Result<usize, String> {
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("{key}: not an integer: {s:?}")),
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

/// Emit the live-experiment metric set: writer latency (p50/p99/mean + max),
/// snapshot latency over completed snapshots, counts, and image size.
pub fn emit_live(
    experiment: &str,
    writer_ns: &[u64],
    snap_ns: &[u64],
    skipped: u64,
    snap_len: usize,
) {
    emit_latency(experiment, "writer", writer_ns);
    emit_int(
        experiment,
        "writer_max",
        writer_ns.iter().copied().max().unwrap_or(0),
        "ns",
        writer_ns.len(),
    );
    emit_latency(experiment, "snapshot", snap_ns);
    emit_int(experiment, "snap_count", snap_ns.len() as u64, "count", 1);
    emit_int(experiment, "snap_skipped", skipped, "count", 1);
    emit_int(experiment, "snapshot_bytes", snap_len as u64, "bytes", 1);
}

/// Resident set size in bytes, from Linux `/proc/self/statm` field 2
/// (resident pages). Returns 0 where unreadable — the bench hosts are
/// x86-64 Linux with 4 KiB pages, which is the only case that must be right.
pub fn rss_bytes() -> u64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    match s
        .split_whitespace()
        .nth(1)
        .and_then(|f| f.parse::<u64>().ok())
    {
        Some(pages) => pages * 4096,
        None => 0,
    }
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

    #[test]
    fn smrc_otr_bps_defaults_to_100() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("SMRC_OTR_BPS") };
        let c = SmrConfig::from_env().expect("defaults parse");
        assert_eq!(c.otr_bps, 100, "default OTR is 1% = 100 bps");
    }

    #[test]
    fn smrc_otr_bps_zero_is_legal() {
        let _g = ENV_LOCK.lock().unwrap();
        // A pure-cancel run (no fills) is a legitimate experiment; unlike the
        // other SMRC_* knobs, zero must not be rejected here.
        unsafe { std::env::set_var("SMRC_OTR_BPS", "0") };
        let c = SmrConfig::from_env();
        unsafe { std::env::remove_var("SMRC_OTR_BPS") };
        assert_eq!(c.expect("OTR=0 must parse").otr_bps, 0);
    }

    #[test]
    fn smrc_otr_bps_rejects_over_10000() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("SMRC_OTR_BPS", "10001") };
        let r = SmrConfig::from_env();
        unsafe { std::env::remove_var("SMRC_OTR_BPS") };
        assert!(r.is_err(), "OTR above 100% must be rejected");
    }

    #[test]
    fn churn_sized_run_parses_but_fails_bump_capacity() {
        let _g = ENV_LOCK.lock().unwrap();
        // warmup + iters > cap is legal for a slot-recycling churn cell and
        // illegal for a bump-allocating insert cell.
        unsafe {
            std::env::set_var("SMRC_CAP", "1024");
            std::env::set_var("SMRC_STEADY", "512");
            std::env::set_var("SMRC_WARMUP", "1000");
            std::env::set_var("SMRC_ITERS", "10000");
            std::env::set_var("SMRC_CHUNK", "256");
        }
        let c = SmrConfig::from_env().expect("churn-sized config must parse");
        let bump = c.require_bump_capacity();
        unsafe {
            for k in [
                "SMRC_CAP",
                "SMRC_STEADY",
                "SMRC_WARMUP",
                "SMRC_ITERS",
                "SMRC_CHUNK",
            ] {
                std::env::remove_var(k);
            }
        }
        assert!(bump.is_err(), "bump-allocating cells must reject it");
    }

    #[test]
    fn rss_bytes_is_nonzero_on_linux() {
        assert!(
            rss_bytes() > 0,
            "RSS must be readable from /proc/self/statm"
        );
    }
}
