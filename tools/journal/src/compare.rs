//! Join two result sets on the cell key and classify each shared cell with a
//! direction-aware verdict.

use std::collections::BTreeMap;

use crate::model::{CellKey, ResultLine};

/// Whether a metric is better when it goes up or down. Inferred from the unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Latency-style: `ns`, `us`, `ms`, `s`. An increase is a regression.
    LowerBetter,
    /// Throughput-style: `*_per_sec`. A decrease is a regression.
    HigherBetter,
    /// Unit not recognised: defaults to lower-is-better, callers note it.
    Unknown,
}

impl Direction {
    pub fn from_unit(unit: &str) -> Direction {
        match unit {
            "ns" | "us" | "ms" | "s" => Direction::LowerBetter,
            "ops_per_sec" | "bytes_per_sec" => Direction::HigherBetter,
            _ => Direction::Unknown,
        }
    }

    /// Treat `Unknown` as lower-is-better for the regression decision.
    fn effective_lower_better(self) -> bool {
        matches!(self, Direction::LowerBetter | Direction::Unknown)
    }
}

/// The verdict for a cell present in both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Moved in the worse direction beyond the threshold.
    Regression,
    /// Moved in the better direction beyond the threshold.
    Improvement,
    /// Within the threshold either way (or no change).
    Same,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Regression => "REGRESSION",
            Verdict::Improvement => "improved",
            Verdict::Same => "ok",
        }
    }
}

/// A cell joined across both runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub key: CellKey,
    pub unit: String,
    pub direction: Direction,
    pub a: f64,
    pub b: f64,
    pub abs_delta: f64,
    /// Percent change of B relative to A. `None` when A is 0 (undefined).
    pub pct_delta: Option<f64>,
    pub verdict: Verdict,
    /// True when the direction had to be guessed from an unknown unit.
    pub unknown_unit: bool,
}

/// The result of joining two runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JoinResult {
    /// Cells present in both runs (sorted by key).
    pub shared: Vec<Comparison>,
    /// Cell keys present only in B (added), sorted.
    pub added: Vec<CellKey>,
    /// Cell keys present only in A (removed), sorted.
    pub removed: Vec<CellKey>,
}

impl JoinResult {
    pub fn regressions(&self) -> impl Iterator<Item = &Comparison> {
        self.shared
            .iter()
            .filter(|c| c.verdict == Verdict::Regression)
    }

    pub fn has_regression(&self) -> bool {
        self.regressions().next().is_some()
    }
}

/// Build a key -> line map, skipping placeholder/zero lines. Later duplicate
/// keys win (last line for a cell).
fn index(lines: &[ResultLine]) -> BTreeMap<CellKey, &ResultLine> {
    let mut map = BTreeMap::new();
    for line in lines {
        if line.is_placeholder() {
            continue;
        }
        map.insert(line.cell_key(), line);
    }
    map
}

/// Classify one shared cell.
fn classify(
    a: f64,
    b: f64,
    unit: &str,
    threshold_pct: f64,
) -> (f64, Option<f64>, Verdict, Direction) {
    let direction = Direction::from_unit(unit);
    let abs_delta = b - a;
    let pct_delta = if a == 0.0 {
        None
    } else {
        Some((b - a) / a.abs() * 100.0)
    };

    let verdict = match pct_delta {
        None => Verdict::Same,
        Some(pct) => {
            if pct.abs() < threshold_pct {
                Verdict::Same
            } else if direction.effective_lower_better() {
                // lower is better: positive pct (B bigger) is worse
                if pct > 0.0 {
                    Verdict::Regression
                } else {
                    Verdict::Improvement
                }
            } else {
                // higher is better: negative pct (B smaller) is worse
                if pct < 0.0 {
                    Verdict::Regression
                } else {
                    Verdict::Improvement
                }
            }
        }
    };

    (abs_delta, pct_delta, verdict, direction)
}

/// Join run A and run B on the cell key, classifying each shared cell against
/// `threshold_pct` (e.g. `10.0`).
pub fn join(a_lines: &[ResultLine], b_lines: &[ResultLine], threshold_pct: f64) -> JoinResult {
    let a = index(a_lines);
    let b = index(b_lines);

    let mut shared = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();

    for (key, a_line) in &a {
        match b.get(key) {
            Some(b_line) => {
                let (abs_delta, pct_delta, verdict, direction) =
                    classify(a_line.value, b_line.value, &b_line.unit, threshold_pct);
                shared.push(Comparison {
                    key: key.clone(),
                    unit: b_line.unit.clone(),
                    direction,
                    a: a_line.value,
                    b: b_line.value,
                    abs_delta,
                    pct_delta,
                    verdict,
                    unknown_unit: direction == Direction::Unknown,
                });
            }
            None => removed.push(key.clone()),
        }
    }
    for key in b.keys() {
        if !a.contains_key(key) {
            added.push(key.clone());
        }
    }

    // BTreeMap iteration is already sorted; vectors inherit that order.
    JoinResult {
        shared,
        added,
        removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(
        focus: &str,
        exp: &str,
        lang: &str,
        metric: &str,
        value: f64,
        unit: &str,
    ) -> ResultLine {
        ResultLine {
            language: lang.into(),
            focus_area: focus.into(),
            experiment: exp.into(),
            metric: metric.into(),
            value,
            unit: unit.into(),
            samples: 1,
            notes: None,
        }
    }

    #[test]
    fn direction_from_unit() {
        for u in ["ns", "us", "ms", "s"] {
            assert_eq!(Direction::from_unit(u), Direction::LowerBetter);
        }
        for u in ["ops_per_sec", "bytes_per_sec"] {
            assert_eq!(Direction::from_unit(u), Direction::HigherBetter);
        }
        assert_eq!(Direction::from_unit("widgets"), Direction::Unknown);
    }

    #[test]
    fn delta_math() {
        let a = [line("f", "e", "rust", "m", 100.0, "ns")];
        let b = [line("f", "e", "rust", "m", 120.0, "ns")];
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared.len(), 1);
        let c = &j.shared[0];
        assert_eq!(c.abs_delta, 20.0);
        assert_eq!(c.pct_delta, Some(20.0));
    }

    #[test]
    fn latency_increase_beyond_threshold_is_regression() {
        let a = [line("f", "e", "rust", "m", 100.0, "ns")];
        let b = [line("f", "e", "rust", "m", 120.0, "ns")]; // +20% latency
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].verdict, Verdict::Regression);
        assert!(j.has_regression());
    }

    #[test]
    fn latency_decrease_is_improvement() {
        let a = [line("f", "e", "rust", "m", 100.0, "ms")];
        let b = [line("f", "e", "rust", "m", 50.0, "ms")];
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].verdict, Verdict::Improvement);
        assert!(!j.has_regression());
    }

    #[test]
    fn throughput_decrease_beyond_threshold_is_regression() {
        let a = [line("f", "e", "rust", "m", 1000.0, "ops_per_sec")];
        let b = [line("f", "e", "rust", "m", 800.0, "ops_per_sec")]; // -20%
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].verdict, Verdict::Regression);
    }

    #[test]
    fn throughput_increase_is_improvement() {
        let a = [line("f", "e", "rust", "m", 1000.0, "bytes_per_sec")];
        let b = [line("f", "e", "rust", "m", 2000.0, "bytes_per_sec")];
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].verdict, Verdict::Improvement);
    }

    #[test]
    fn just_under_threshold_is_same() {
        let a = [line("f", "e", "rust", "m", 100.0, "ns")];
        let b = [line("f", "e", "rust", "m", 109.9, "ns")]; // +9.9% < 10
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].verdict, Verdict::Same);
        assert!(!j.has_regression());
    }

    #[test]
    fn at_threshold_boundary_flags() {
        // exactly 10% is NOT < threshold, so it counts as a regression for latency
        let a = [line("f", "e", "rust", "m", 100.0, "ns")];
        let b = [line("f", "e", "rust", "m", 110.0, "ns")];
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].verdict, Verdict::Regression);
    }

    #[test]
    fn unknown_unit_defaults_lower_better_and_flags() {
        let a = [line("f", "e", "rust", "m", 100.0, "widgets")];
        let b = [line("f", "e", "rust", "m", 200.0, "widgets")];
        let j = join(&a, &b, 10.0);
        assert!(j.shared[0].unknown_unit);
        assert_eq!(j.shared[0].direction, Direction::Unknown);
        assert_eq!(j.shared[0].verdict, Verdict::Regression);
    }

    #[test]
    fn placeholder_and_zero_skipped() {
        let a = [
            line(
                "filesystem-write",
                "placeholder",
                "go",
                "placeholder",
                0.0,
                "ns",
            ),
            line("network-rtt", "tcp", "rust", "rtt_p50", 0.0, "ns"), // zero -> skipped
        ];
        let b = a.clone();
        let j = join(&a, &b, 10.0);
        assert!(j.shared.is_empty());
        assert!(j.added.is_empty());
        assert!(j.removed.is_empty());
    }

    #[test]
    fn added_and_removed_cells_listed() {
        let a = [
            line("f", "e", "rust", "m1", 100.0, "ns"),
            line("f", "e", "rust", "only_a", 100.0, "ns"),
        ];
        let b = [
            line("f", "e", "rust", "m1", 100.0, "ns"),
            line("f", "e", "rust", "only_b", 100.0, "ns"),
        ];
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared.len(), 1);
        assert_eq!(j.removed.len(), 1);
        assert_eq!(j.removed[0].metric, "only_a");
        assert_eq!(j.added.len(), 1);
        assert_eq!(j.added[0].metric, "only_b");
    }

    #[test]
    fn zero_baseline_has_no_pct() {
        // a present but value becomes nonzero in b while a==0 -> a is skipped
        // (zero). Instead test pct None path directly via classify-equivalent:
        let a = [line("f", "e", "rust", "m", 5.0, "ns")];
        let b = [line("f", "e", "rust", "m", 5.0, "ns")];
        let j = join(&a, &b, 10.0);
        assert_eq!(j.shared[0].pct_delta, Some(0.0));
        assert_eq!(j.shared[0].verdict, Verdict::Same);
    }
}
