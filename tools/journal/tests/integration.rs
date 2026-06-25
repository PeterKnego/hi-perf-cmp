//! End-to-end: record two synthetic runs into a temp journal, then compare and
//! assert the regression is flagged (and unchanged cells are not).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use journal::compare::{self, Verdict};
use journal::model::{CellKey, parse_results};
use journal::record;

/// A unique temp dir under the OS temp dir, cleaned up on drop.
struct TempJournal {
    path: PathBuf,
}

impl TempJournal {
    fn new() -> TempJournal {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("journal-it-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempJournal { path }
    }
}

impl Drop for TempJournal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read_run(journal: &Path, run_id: &str) -> Vec<journal::model::ResultLine> {
    let body =
        std::fs::read_to_string(journal.join("runs").join(run_id).join("results.jsonl")).unwrap();
    parse_results(&body).unwrap()
}

#[test]
fn record_then_compare_flags_regression() {
    let tj = TempJournal::new();
    let journal = &tj.path;

    let base = record::record(
        &fixture("baseline-run"),
        journal,
        Some("baseline run"),
        false,
    )
    .expect("record baseline");
    let regr = record::record(
        &fixture("regressed-run"),
        journal,
        Some("introduce slow tcp path"),
        false,
    )
    .expect("record regressed");

    // run ids are formed from manifest ts + short sha
    assert_eq!(base.run_id, "20260620T120000Z-aaaaaaaaaaaa");
    assert_eq!(regr.run_id, "20260624T120000Z-bbbbbbbbbbbb");

    // copied files exist + entry.md + INDEX.md regenerated
    assert!(base.run_dir.join("results.jsonl").exists());
    assert!(base.run_dir.join("manifest.txt").exists());
    assert!(base.run_dir.join("entry.md").exists());
    let index_md = std::fs::read_to_string(journal.join("INDEX.md")).unwrap();
    assert!(index_md.contains("introduce slow tcp path"));
    assert!(index_md.contains("baseline run"));
    // newest first: regressed run (later ts) listed above baseline
    let i_regr = index_md.find("20260624T120000Z").unwrap();
    let i_base = index_md.find("20260620T120000Z").unwrap();
    assert!(i_regr < i_base);

    // compare baseline (A) vs regressed (B)
    let a = read_run(journal, &base.run_id);
    let b = read_run(journal, &regr.run_id);
    let result = compare::join(&a, &b, 10.0);

    let regressed_cell = CellKey {
        focus_area: "network-rtt".into(),
        experiment: "tcp".into(),
        language: "rust".into(),
        metric: "rtt_p50".into(),
    };
    let unchanged_cell = CellKey {
        focus_area: "network-rtt".into(),
        experiment: "tcp".into(),
        language: "rust".into(),
        metric: "rtt_p99".into(),
    };

    let regr_cmp = result
        .shared
        .iter()
        .find(|c| c.key == regressed_cell)
        .expect("rtt_p50 cell present");
    assert_eq!(
        regr_cmp.verdict,
        Verdict::Regression,
        "p50 latency up = regression"
    );

    let same_cmp = result
        .shared
        .iter()
        .find(|c| c.key == unchanged_cell)
        .expect("rtt_p99 cell present");
    assert_eq!(same_cmp.verdict, Verdict::Same, "p99 barely moved");

    // placeholder cell skipped entirely
    assert!(
        !result
            .shared
            .iter()
            .any(|c| c.key.experiment == "placeholder")
    );
    assert!(result.added.is_empty());
    assert!(result.removed.is_empty());
    assert!(result.has_regression());
    assert_eq!(result.regressions().count(), 1);
}

#[test]
fn record_refuses_overwrite_without_force() {
    let tj = TempJournal::new();
    record::record(&fixture("baseline-run"), &tj.path, None, false).unwrap();
    let again = record::record(&fixture("baseline-run"), &tj.path, None, false);
    assert!(again.is_err(), "should refuse existing run dir");
    // with force it succeeds
    record::record(&fixture("baseline-run"), &tj.path, None, true).expect("force overwrite");
}
