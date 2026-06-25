//! `baselines.json`: the reference value per cell plus which run it came from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::ResultLine;

/// One baseline cell entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineCell {
    pub value: f64,
    pub unit: String,
    pub run_id: String,
}

/// The whole `baselines.json`: cell-key string -> entry. A `BTreeMap` gives a
/// stable, sorted key order in the pretty-printed JSON.
pub type Baselines = BTreeMap<String, BaselineCell>;

/// Build the baseline map from a run's result lines, skipping placeholder/zero
/// lines. The cell-key string matches `CellKey::Display`.
pub fn build(run_id: &str, lines: &[ResultLine]) -> Baselines {
    let mut map: Baselines = BTreeMap::new();
    for line in lines {
        if line.is_placeholder() {
            continue;
        }
        map.insert(
            line.cell_key().to_string(),
            BaselineCell {
                value: line.value,
                unit: line.unit.clone(),
                run_id: run_id.to_string(),
            },
        );
    }
    map
}

/// Serialize to pretty JSON with a trailing newline.
pub fn to_json(baselines: &Baselines) -> String {
    let mut s = serde_json::to_string_pretty(baselines).expect("baselines serialize");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(metric: &str, value: f64, unit: &str) -> ResultLine {
        ResultLine {
            language: "rust".into(),
            focus_area: "network-rtt".into(),
            experiment: "tcp".into(),
            metric: metric.into(),
            value,
            unit: unit.into(),
            samples: 1,
            notes: None,
        }
    }

    #[test]
    fn builds_cells_skipping_placeholders() {
        let lines = [
            line("rtt_p50", 100.0, "ns"),
            line("rtt_p99", 0.0, "ns"), // zero -> skipped
            ResultLine {
                experiment: "placeholder".into(),
                ..line("placeholder", 5.0, "ns")
            },
        ];
        let b = build("RUN1", &lines);
        assert_eq!(b.len(), 1);
        let cell = &b["network-rtt/tcp/rust/rtt_p50"];
        assert_eq!(cell.value, 100.0);
        assert_eq!(cell.unit, "ns");
        assert_eq!(cell.run_id, "RUN1");
    }

    #[test]
    fn json_is_pretty_stable_and_newline_terminated() {
        let lines = [line("b", 2.0, "ns"), line("a", 1.0, "ns")];
        let json = to_json(&build("R", &lines));
        // sorted key order: ".../a" appears before ".../b"
        let ia = json.find("rust/a").unwrap();
        let ib = json.find("rust/b").unwrap();
        assert!(ia < ib);
        assert!(json.ends_with("}\n"));
        assert!(json.contains("\"run_id\": \"R\""));
        // round-trips
        let back: Baselines = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
    }
}
