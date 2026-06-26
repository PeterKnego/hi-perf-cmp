//! filesystem-write durable-append harness, shared by the fsync/fdatasync/
//! prealloc/batch experiments. Each artifact supplies (SyncKind, batch_size,
//! prealloc); the harness owns config, file setup, warmup, the timed loop, and
//! result emission. stdout stays results-only; std-only.

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Instant;

use crate::result;
use crate::stats;

/// Focus area for every filesystem-write experiment.
pub const FOCUS_AREA: &str = "filesystem-write";

/// Which durability barrier to issue per commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// Full fsync (`File::sync_all`) — flushes data + all metadata.
    Full,
    /// fdatasync (`File::sync_data`) — flushes data + size, skips timestamps.
    Data,
}

/// Parsed, validated filesystem-write configuration (`FSW_*`).
#[derive(Debug, Clone)]
pub struct FsConfig {
    /// Directory for the bench file. Required — no default (guards against tmpfs).
    pub dir: PathBuf,
    /// Entry size in bytes.
    pub entry_bytes: usize,
    /// Discarded warmup operations.
    pub warmup: usize,
    /// Measured entries.
    pub iterations: usize,
    /// Entries per group-commit (the `batch` experiment only).
    pub batch: usize,
}

impl FsConfig {
    /// Read configuration from `FSW_*`, applying defaults and validating.
    pub fn from_env() -> Result<FsConfig, String> {
        let dir = match env::var("FSW_DIR") {
            Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim()),
            _ => {
                return Err(
                    "FSW_DIR: required (set FSW_DIR=<dir on a real disk, not tmpfs>)".to_string(),
                );
            }
        };
        Ok(FsConfig {
            dir,
            entry_bytes: parse_positive("FSW_ENTRY_BYTES", 256)?,
            warmup: parse_positive("FSW_WARMUP", 5_000)?,
            iterations: parse_positive("FSW_ITERATIONS", 50_000)?,
            batch: parse_positive("FSW_BATCH", 32)?,
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

/// Build config from env, run the experiment, and emit its four result lines.
pub fn run_and_emit(
    experiment: &str,
    sync_kind: SyncKind,
    prealloc: bool,
    batched: bool,
) -> Result<(), String> {
    let cfg = FsConfig::from_env()?;
    let batch_size = if batched { cfg.batch } else { 1 };
    let (samples, throughput) =
        run_durable_append(&cfg, sync_kind, batch_size, prealloc, experiment)
            .map_err(|e| format!("{e}"))?;
    emit_fs(experiment, &samples, throughput, cfg.iterations);
    Ok(())
}

/// Run the durable-append loop. Returns (per-sync latencies in ns, throughput
/// in entries/sec). `batch_size` entries per sync; `prealloc` pre-writes the
/// file so a size-extending sync is avoided.
fn run_durable_append(
    cfg: &FsConfig,
    sync_kind: SyncKind,
    batch_size: usize,
    prealloc: bool,
    experiment: &str,
) -> io::Result<(Vec<u64>, f64)> {
    let path = cfg.dir.join(format!("filesystem-write-{experiment}.log"));
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    // Make the file's existence durable (file + parent dir), outside timing.
    file.sync_all()?;
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }

    let entry = vec![0xABu8; cfg.entry_bytes];

    if prealloc {
        let total = (cfg.warmup + cfg.iterations) * cfg.entry_bytes;
        let zeros = vec![0u8; 1024 * 1024];
        let mut remaining = total;
        while remaining > 0 {
            let n = remaining.min(zeros.len());
            file.write_all(&zeros[..n])?;
            remaining -= n;
        }
        file.sync_all()?;
        file.seek(SeekFrom::Start(0))?;
    }

    // Warmup — timings discarded.
    run_entries(&mut file, &entry, cfg.warmup, batch_size, sync_kind, None)?;

    // Measured — record each sync's latency; time the whole span for throughput.
    let mut samples = Vec::with_capacity(cfg.iterations.div_ceil(batch_size));
    let t_start = Instant::now();
    run_entries(
        &mut file,
        &entry,
        cfg.iterations,
        batch_size,
        sync_kind,
        Some(&mut samples),
    )?;
    let throughput = cfg.iterations as f64 / t_start.elapsed().as_secs_f64();

    Ok((samples, throughput))
}

/// Write `entries` entries in chunks of `batch_size`, issuing one sync per chunk
/// (trailing short chunk allowed). When `samples` is `Some`, push each sync's
/// elapsed ns into it.
fn run_entries(
    file: &mut File,
    entry: &[u8],
    entries: usize,
    batch_size: usize,
    sync_kind: SyncKind,
    mut samples: Option<&mut Vec<u64>>,
) -> io::Result<()> {
    let mut remaining = entries;
    while remaining > 0 {
        let count = remaining.min(batch_size);
        for _ in 0..count {
            file.write_all(entry)?;
        }
        let start = Instant::now();
        match sync_kind {
            SyncKind::Full => file.sync_all()?,
            SyncKind::Data => file.sync_data()?,
        }
        if let Some(s) = samples.as_deref_mut() {
            s.push(start.elapsed().as_nanos() as u64);
        }
        remaining -= count;
    }
    Ok(())
}

/// Sort the sync samples and emit the four filesystem-write result lines.
fn emit_fs(experiment: &str, sync_samples: &[u64], throughput: f64, iterations: usize) {
    let mut sorted = sync_samples.to_vec();
    sorted.sort_unstable();
    let n_sync = sorted.len();

    result::emit_float(
        FOCUS_AREA,
        experiment,
        "durable_append_throughput",
        throughput,
        "ops_per_sec",
        iterations,
    );
    result::emit(
        FOCUS_AREA,
        experiment,
        "sync_p50",
        stats::percentile(&sorted, 50.0),
        "ns",
        n_sync,
    );
    result::emit(
        FOCUS_AREA,
        experiment,
        "sync_p99",
        stats::percentile(&sorted, 99.0),
        "ns",
        n_sync,
    );
    result::emit_float(
        FOCUS_AREA,
        experiment,
        "sync_mean",
        stats::mean(sync_samples),
        "ns",
        n_sync,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fswrite-test-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn batch_chunking_and_prealloc_sync_counts() {
        let dir = scratch_dir("counts");
        let cfg = FsConfig {
            dir: dir.clone(),
            entry_bytes: 64,
            warmup: 5,
            iterations: 20,
            batch: 4,
        };

        // batch 4 over 20 iterations → 5 syncs; throughput positive.
        let (samples, tput) =
            run_durable_append(&cfg, SyncKind::Data, 4, false, "test-batch").unwrap();
        assert_eq!(samples.len(), 5, "20 entries / batch 4 = 5 syncs");
        assert!(tput > 0.0);

        // non-divisible: 21 iterations, batch 4 → ceil = 6 syncs.
        let cfg2 = FsConfig {
            iterations: 21,
            ..cfg.clone()
        };
        let (s2, _) = run_durable_append(&cfg2, SyncKind::Data, 4, false, "test-batch2").unwrap();
        assert_eq!(s2.len(), 6, "ceil(21/4) = 6 syncs");

        // prealloc, batch 1 → 20 syncs; file at least the preallocated size.
        let (s3, _) = run_durable_append(&cfg, SyncKind::Data, 1, true, "test-prealloc").unwrap();
        assert_eq!(s3.len(), 20);
        let len = std::fs::metadata(dir.join("filesystem-write-test-prealloc.log"))
            .unwrap()
            .len();
        assert!(len >= ((cfg.warmup + cfg.iterations) * cfg.entry_bytes) as u64);

        std::fs::remove_dir_all(&dir).ok();
    }
}
