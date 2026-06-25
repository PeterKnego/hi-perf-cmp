//! `record`: copy a bench-out run into the journal and write a pre-filled
//! `entry.md`.

use std::fs;
use std::path::Path;

use crate::index;
use crate::model::{Manifest, parse_manifest};

/// Render the pre-filled `entry.md` from the manifest and optional `--desc`.
///
/// Commit / instance / params come from the manifest; prose sections are left
/// as headers for the author. When `desc` is given it becomes the first line of
/// `## What changed`; otherwise that section keeps an author prompt.
pub fn render_entry(run_id: &str, manifest: &Manifest, desc: Option<&str>) -> String {
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
        render_entry(&run_id, &manifest, desc),
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

    #[test]
    fn entry_fills_provenance_from_manifest() {
        let md = render_entry("RUNID", &manifest(), None);
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
        let md = render_entry("R", &manifest(), Some("udp: sendmmsg batching"));
        let headline = crate::index::extract_headline(&md);
        assert_eq!(headline, "udp: sendmmsg batching");
    }

    #[test]
    fn no_desc_leaves_author_prompt() {
        let md = render_entry("R", &manifest(), None);
        assert!(md.contains("<one-paragraph description"));
        assert_eq!(crate::index::extract_headline(&md), "");
    }

    #[test]
    fn missing_fields_use_placeholders() {
        let m = parse_manifest("timestamp=T\ngit_sha=\n");
        let md = render_entry("R", &m, None);
        assert!(md.contains("- commit: (unknown)"));
        assert!(md.contains("- instance: unknown, ? vCPU, kernel ?"));
        assert!(md.contains("payload=?B warmup=? iterations=?"));
    }
}
