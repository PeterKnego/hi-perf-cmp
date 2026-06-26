//! `record`: copy a bench-out run into the journal and write a pre-filled
//! `entry.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::index;
use crate::model::{Manifest, ResultLine, parse_manifest, parse_results};

/// Format a metric value: integer when whole, else one decimal (tolerates both
/// the integer `42000` and Java's `34151.0` renderings without noise).
fn fmt_val(v: f64) -> String {
    if v.fract().abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

/// Per (focus_area, experiment) group accumulator for the results digest.
#[derive(Default)]
struct Group {
    units: BTreeMap<String, String>,       // metric -> unit
    langs: BTreeSet<String>,               // languages seen
    vals: BTreeMap<(String, String), f64>, // (language, metric) -> value
}

/// Render the `## Results` digest: per (focus_area, experiment), a
/// language × metric table of the measured values. Placeholder / stub cells are
/// omitted. Returns an empty string when there are no real result lines, so the
/// section is dropped entirely for an all-stub run.
pub fn render_results(results: &[ResultLine]) -> String {
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for r in results.iter().filter(|r| !r.is_placeholder()) {
        let g = groups
            .entry((r.focus_area.clone(), r.experiment.clone()))
            .or_default();
        g.units.entry(r.metric.clone()).or_insert(r.unit.clone());
        g.langs.insert(r.language.clone());
        g.vals
            .insert((r.language.clone(), r.metric.clone()), r.value);
    }
    if groups.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "## Results\n\nPer-cell values from this run (placeholder/stub cells omitted).\n\n",
    );
    for ((focus_area, experiment), g) in &groups {
        out.push_str(&format!("### {focus_area} / {experiment}\n\n"));
        let metrics: Vec<&String> = g.units.keys().collect();
        out.push_str("| language |");
        for m in &metrics {
            out.push_str(&format!(" {m} ({}) |", g.units[*m]));
        }
        out.push('\n');
        out.push_str("|---|");
        for _ in &metrics {
            out.push_str("---|");
        }
        out.push('\n');
        for lang in &g.langs {
            out.push_str(&format!("| {lang} |"));
            for m in &metrics {
                match g.vals.get(&(lang.clone(), (*m).clone())) {
                    Some(v) => out.push_str(&format!(" {} |", fmt_val(*v))),
                    None => out.push_str(" — |"),
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// Render the pre-filled `entry.md` from the manifest, the run's results, and an
/// optional `--desc`.
///
/// Commit / instance / params come from the manifest; an auto-generated
/// `## Results` digest carries the numbers; the prose sections (`Hypothesis` /
/// `Observations`) are left as headers for the author. When `desc` is given it
/// becomes the first line of `## What changed`; otherwise that section keeps an
/// author prompt.
pub fn render_entry(
    run_id: &str,
    manifest: &Manifest,
    results: &[ResultLine],
    desc: Option<&str>,
) -> String {
    let sha = manifest.git_sha();
    let sha = if sha.is_empty() { "(unknown)" } else { sha };
    let instance = manifest.get("instance_type").unwrap_or("unknown");
    let vcpus = manifest.get("vcpus").unwrap_or("?");
    let kernel = manifest.get("kernel").unwrap_or("?");
    let payload = manifest.get("rtt_payload_bytes").unwrap_or("?");
    let warmup = manifest.get("rtt_warmup").unwrap_or("?");
    let iterations = manifest.get("rtt_iterations").unwrap_or("?");

    let what_changed = match desc {
        Some(d) if !d.trim().is_empty() => d.trim().to_string(),
        _ => "<one-paragraph description of what was added/changed in this version>".to_string(),
    };

    let results_section = render_results(results);

    format!(
        "# {run_id}\n\
\n\
- commit: {sha}\n\
- instance: {instance}, {vcpus} vCPU, kernel {kernel}\n\
- params: payload={payload}B warmup={warmup} iterations={iterations}\n\
\n\
## What changed\n\
{what_changed}\n\
\n\
{results_section}\
## Hypothesis\n\
<what we expected to happen>\n\
\n\
## Observations\n\
<what actually happened; reference compare output / notable deltas>\n"
    )
}

/// Outcome of a successful `record`.
#[derive(Debug)]
pub struct RecordOutcome {
    pub run_id: String,
    pub run_dir: std::path::PathBuf,
}

/// Record the run found in `from_dir` into `journal_dir`.
///
/// Reads `<from_dir>/manifest.txt` for the run id, creates
/// `<journal_dir>/runs/<run-id>/`, copies `results.jsonl` + `manifest.txt`,
/// writes `entry.md`, and regenerates `INDEX.md`. Refuses an existing run dir
/// unless `force`.
pub fn record(
    from_dir: &Path,
    journal_dir: &Path,
    desc: Option<&str>,
    force: bool,
) -> Result<RecordOutcome, String> {
    let manifest_path = from_dir.join("manifest.txt");
    let results_path = from_dir.join("results.jsonl");

    let manifest_body = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("reading {}: {e}", manifest_path.display()))?;
    if !results_path.exists() {
        return Err(format!("missing {}", results_path.display()));
    }
    let results_body = fs::read_to_string(&results_path)
        .map_err(|e| format!("reading {}: {e}", results_path.display()))?;
    let results = parse_results(&results_body)?;
    let manifest = parse_manifest(&manifest_body);
    let run_id = manifest.run_id();

    let run_dir = journal_dir.join("runs").join(&run_id);
    if run_dir.exists() {
        if !force {
            return Err(format!(
                "run dir already exists: {} (use --force to overwrite)",
                run_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(&run_dir).map_err(|e| format!("creating {}: {e}", run_dir.display()))?;
    }

    fs::copy(&results_path, run_dir.join("results.jsonl"))
        .map_err(|e| format!("copying results.jsonl: {e}"))?;
    fs::copy(&manifest_path, run_dir.join("manifest.txt"))
        .map_err(|e| format!("copying manifest.txt: {e}"))?;
    fs::write(
        run_dir.join("entry.md"),
        render_entry(&run_id, &manifest, &results, desc),
    )
    .map_err(|e| format!("writing entry.md: {e}"))?;

    index::regenerate(journal_dir).map_err(|e| format!("regenerating INDEX.md: {e}"))?;

    Ok(RecordOutcome { run_id, run_dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        parse_manifest(
            "timestamp=20260625T053000Z\n\
             git_sha=abcdef0123456789\n\
             instance_type=c7g.large\n\
             vcpus=2\n\
             kernel=6.8.0-aws\n\
             rtt_payload_bytes=64\n\
             rtt_warmup=1000\n\
             rtt_iterations=100000\n",
        )
    }

    fn results() -> Vec<ResultLine> {
        parse_results(
            "{\"language\":\"rust\",\"focus_area\":\"network-rtt\",\"experiment\":\"tcp\",\"metric\":\"rtt_p50\",\"value\":36025,\"unit\":\"ns\",\"samples\":100000}\n\
             {\"language\":\"java\",\"focus_area\":\"network-rtt\",\"experiment\":\"tcp\",\"metric\":\"rtt_p50\",\"value\":35156.0,\"unit\":\"ns\",\"samples\":100000}\n\
             {\"language\":\"go\",\"focus_area\":\"filesystem-write\",\"experiment\":\"placeholder\",\"metric\":\"placeholder\",\"value\":0,\"unit\":\"ns\",\"samples\":0}\n",
        )
        .unwrap()
    }

    #[test]
    fn results_digest_tabulates_real_cells_and_omits_placeholders() {
        let md = render_results(&results());
        assert!(md.contains("## Results"));
        assert!(md.contains("### network-rtt / tcp"));
        assert!(md.contains("rtt_p50 (ns)"));
        assert!(md.contains("| rust | 36025 |")); // integer rendering
        assert!(md.contains("| java | 35156 |")); // float .0 rendered as integer
        assert!(!md.contains("filesystem-write")); // stub cell's group omitted
    }

    #[test]
    fn results_digest_empty_for_all_stub_run() {
        let stubs = parse_results(
            "{\"language\":\"go\",\"focus_area\":\"filesystem-write\",\"experiment\":\"placeholder\",\"metric\":\"placeholder\",\"value\":0,\"unit\":\"ns\",\"samples\":0}\n",
        )
        .unwrap();
        assert_eq!(render_results(&stubs), "");
    }

    #[test]
    fn entry_embeds_results_digest() {
        let md = render_entry("RUNID", &manifest(), &results(), None);
        assert!(md.contains("## Results"));
        assert!(md.contains("| rust | 36025 |"));
        // ordering: What changed -> Results -> Hypothesis
        let wc = md.find("## What changed").unwrap();
        let res = md.find("## Results").unwrap();
        let hyp = md.find("## Hypothesis").unwrap();
        assert!(wc < res && res < hyp);
    }

    #[test]
    fn entry_fills_provenance_from_manifest() {
        let md = render_entry("RUNID", &manifest(), &[], None);
        assert!(md.starts_with("# RUNID\n"));
        assert!(md.contains("- commit: abcdef0123456789"));
        assert!(md.contains("- instance: c7g.large, 2 vCPU, kernel 6.8.0-aws"));
        assert!(md.contains("- params: payload=64B warmup=1000 iterations=100000"));
        // prose sections present as headers
        assert!(md.contains("## What changed"));
        assert!(md.contains("## Hypothesis"));
        assert!(md.contains("## Observations"));
    }

    #[test]
    fn desc_becomes_what_changed_first_line() {
        let md = render_entry("R", &manifest(), &[], Some("udp: sendmmsg batching"));
        let headline = crate::index::extract_headline(&md);
        assert_eq!(headline, "udp: sendmmsg batching");
    }

    #[test]
    fn no_desc_leaves_author_prompt() {
        let md = render_entry("R", &manifest(), &[], None);
        assert!(md.contains("<one-paragraph description"));
        assert_eq!(crate::index::extract_headline(&md), "");
    }

    #[test]
    fn missing_fields_use_placeholders() {
        let m = parse_manifest("timestamp=T\ngit_sha=\n");
        let md = render_entry("R", &m, &[], None);
        assert!(md.contains("- commit: (unknown)"));
        assert!(md.contains("- instance: unknown, ? vCPU, kernel ?"));
        assert!(md.contains("payload=?B warmup=? iterations=?"));
    }
}
