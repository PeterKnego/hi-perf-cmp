//! Core data model: the result-contract line, the cell key used to join runs,
//! and parsers for `results.jsonl` and `manifest.txt`.

use std::collections::BTreeMap;

use serde::Deserialize;

/// One result-contract line (see `docs/result-contract.md`).
///
/// `value` is a JSON number; serde gives us an `f64` which tolerates both the
/// integer form (`42000`) and the Java float rendering (`34151.0`). `notes` is
/// optional.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ResultLine {
    pub language: String,
    pub focus_area: String,
    pub experiment: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub samples: i64,
    #[serde(default)]
    pub notes: Option<String>,
}

impl ResultLine {
    /// The join key: `(focus_area, experiment, language, metric)`.
    pub fn cell_key(&self) -> CellKey {
        CellKey {
            focus_area: self.focus_area.clone(),
            experiment: self.experiment.clone(),
            language: self.language.clone(),
            metric: self.metric.clone(),
        }
    }

    /// Placeholder / stub lines are skipped in comparisons: either the spec
    /// marker `experiment == "placeholder"` or a zero value.
    pub fn is_placeholder(&self) -> bool {
        self.experiment == "placeholder" || self.value == 0.0
    }
}

/// The cell key benchmarks are joined on. Ordered so a `BTreeMap` keyed on it
/// yields a stable, human-friendly ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellKey {
    pub focus_area: String,
    pub experiment: String,
    pub language: String,
    pub metric: String,
}

impl std::fmt::Display for CellKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}/{}/{}",
            self.focus_area, self.experiment, self.language, self.metric
        )
    }
}

/// Parse a `results.jsonl` body into result lines.
///
/// Blank / whitespace-only lines are skipped. A malformed line is reported with
/// its 1-based line number.
pub fn parse_results(body: &str) -> Result<Vec<ResultLine>, String> {
    let mut out = Vec::new();
    for (i, raw) in body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: ResultLine =
            serde_json::from_str(line).map_err(|e| format!("results.jsonl line {}: {e}", i + 1))?;
        out.push(parsed);
    }
    Ok(out)
}

/// Parsed `manifest.txt` provenance: simple `key=value` lines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Manifest {
    pub fields: BTreeMap<String, String>,
}

impl Manifest {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    pub fn git_sha(&self) -> &str {
        self.get("git_sha").unwrap_or("")
    }

    pub fn timestamp(&self) -> &str {
        self.get("timestamp").unwrap_or("")
    }

    /// Short sha used in the run id: first 12 chars of `git_sha`. Falls back to
    /// `nogit` for a missing sha or the sentinel `no-git` / `n/a`.
    pub fn short_sha(&self) -> String {
        let sha = self.git_sha().trim();
        if sha.is_empty() || sha == "no-git" || sha == "n/a" {
            return "nogit".to_string();
        }
        sha.chars().take(12).collect()
    }

    /// `<UTC-ts>-<short-sha>`; ts falls back to `notime` if absent.
    pub fn run_id(&self) -> String {
        let ts = self.timestamp().trim();
        let ts = if ts.is_empty() { "notime" } else { ts };
        format!("{ts}-{}", self.short_sha())
    }
}

/// Parse `manifest.txt`: `key=value` lines. Blank lines and `#` comments are
/// ignored. The value keeps everything after the first `=` (verbatim, trimmed
/// of surrounding whitespace) so values may themselves contain `=`.
pub fn parse_manifest(body: &str) -> Manifest {
    let mut fields = BTreeMap::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            fields.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Manifest { fields }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_integer_and_float_values() {
        let body = r#"
{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":42000,"unit":"ns","samples":100000}
{"language":"java","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":34151.0,"unit":"ns","samples":100000,"notes":"warmed"}
"#;
        let rows = parse_results(body).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].value, 42000.0);
        assert_eq!(rows[1].value, 34151.0);
        assert_eq!(rows[1].notes.as_deref(), Some("warmed"));
        assert_eq!(rows[0].notes, None);
    }

    #[test]
    fn skips_blank_lines() {
        let body = "\n  \n{\"language\":\"go\",\"focus_area\":\"network-rtt\",\"experiment\":\"udp\",\"metric\":\"rtt_p99\",\"value\":1,\"unit\":\"us\",\"samples\":10}\n\n";
        let rows = parse_results(body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].language, "go");
    }

    #[test]
    fn malformed_line_reports_line_number() {
        let body = "{\"language\":\"rust\"}\nnot json";
        let err = parse_results(body).unwrap_err();
        // first line is missing required fields -> error on line 1
        assert!(err.contains("line 1"), "got: {err}");
    }

    #[test]
    fn cell_key_components_and_display() {
        let line: ResultLine = serde_json::from_str(
            r#"{"language":"rust","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":1,"unit":"ns","samples":1}"#,
        )
        .unwrap();
        let key = line.cell_key();
        assert_eq!(key.focus_area, "network-rtt");
        assert_eq!(key.experiment, "tcp");
        assert_eq!(key.language, "rust");
        assert_eq!(key.metric, "rtt_p50");
        assert_eq!(key.to_string(), "network-rtt/tcp/rust/rtt_p50");
    }

    #[test]
    fn placeholder_detection() {
        let stub: ResultLine = serde_json::from_str(
            r#"{"language":"go","focus_area":"filesystem-write","experiment":"placeholder","metric":"placeholder","value":0,"unit":"ns","samples":0,"notes":"stub"}"#,
        )
        .unwrap();
        assert!(stub.is_placeholder());

        let zero: ResultLine = serde_json::from_str(
            r#"{"language":"go","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":0,"unit":"ns","samples":1}"#,
        )
        .unwrap();
        assert!(zero.is_placeholder());

        let real: ResultLine = serde_json::from_str(
            r#"{"language":"go","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":5,"unit":"ns","samples":1}"#,
        )
        .unwrap();
        assert!(!real.is_placeholder());
    }

    #[test]
    fn manifest_parses_key_values() {
        let body = "\
timestamp=20260625T053000Z
instance_type=c7g.large
vcpus=2
kernel=6.8.0-aws
rtt_payload_bytes=64
rtt_warmup=1000
rtt_iterations=100000
git_sha=abcdef0123456789abcdef
# a comment line
node0_role=node0 (client/driver + single-host benches)
";
        let m = parse_manifest(body);
        assert_eq!(m.timestamp(), "20260625T053000Z");
        assert_eq!(m.get("instance_type"), Some("c7g.large"));
        assert_eq!(m.get("vcpus"), Some("2"));
        assert_eq!(m.get("kernel"), Some("6.8.0-aws"));
        assert_eq!(m.get("rtt_payload_bytes"), Some("64"));
        // value containing '=' / parens preserved
        assert_eq!(
            m.get("node0_role"),
            Some("node0 (client/driver + single-host benches)")
        );
        // comment ignored
        assert_eq!(m.get("# a comment line"), None);
    }

    #[test]
    fn short_sha_is_first_12() {
        let m = parse_manifest("git_sha=abcdef0123456789abcdef\ntimestamp=20260625T053000Z");
        assert_eq!(m.short_sha(), "abcdef012345");
        assert_eq!(m.run_id(), "20260625T053000Z-abcdef012345");
    }

    #[test]
    fn missing_or_nogit_sha_falls_back() {
        for sentinel in ["", "no-git", "n/a"] {
            let m = parse_manifest(&format!("git_sha={sentinel}\ntimestamp=T"));
            assert_eq!(m.short_sha(), "nogit", "for {sentinel:?}");
            assert_eq!(m.run_id(), "T-nogit");
        }
        // truly absent
        let m = parse_manifest("timestamp=T");
        assert_eq!(m.short_sha(), "nogit");
    }
}
