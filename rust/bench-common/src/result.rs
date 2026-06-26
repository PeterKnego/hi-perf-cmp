//! Result-contract JSON line emission.
//!
//! Hand-rendered one-object-per-line JSON on stdout, matching
//! `docs/result-contract.md`. stdout is results-only; every line carries the
//! `experiment` dimension. Two emitters: an integer-value and a float-value
//! variant (percentiles are integers, the mean is fractional).

const LANGUAGE: &str = "rust";

/// Emit a result line with an integer `value`.
pub fn emit(
    focus_area: &str,
    experiment: &str,
    metric: &str,
    value: u64,
    unit: &str,
    samples: usize,
) {
    println!(
        r#"{{"language":"{LANGUAGE}","focus_area":"{focus_area}","experiment":"{experiment}","metric":"{metric}","value":{value},"unit":"{unit}","samples":{samples}}}"#
    );
}

/// Emit a result line with a (possibly fractional) numeric `value`.
pub fn emit_float(
    focus_area: &str,
    experiment: &str,
    metric: &str,
    value: f64,
    unit: &str,
    samples: usize,
) {
    println!(
        r#"{{"language":"{LANGUAGE}","focus_area":"{focus_area}","experiment":"{experiment}","metric":"{metric}","value":{value},"unit":"{unit}","samples":{samples}}}"#
    );
}

/// Emit a stub placeholder line for a not-yet-implemented focus area.
pub fn emit_placeholder(focus_area: &str) {
    println!(
        r#"{{"language":"{LANGUAGE}","focus_area":"{focus_area}","experiment":"placeholder","metric":"placeholder","value":0,"unit":"ns","samples":0,"notes":"stub"}}"#
    );
}
