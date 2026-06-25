//! `journal` CLI — thin clap wiring over the library modules.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use journal::baseline;
use journal::compare;
use journal::index;
use journal::model::parse_results;
use journal::record;

#[derive(Parser)]
#[command(
    name = "journal",
    about = "Record and compare curated benchmark runs (the benchmark journal)."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Record a bench-out run into the journal.
    Record {
        /// The bench-out/dist/<ts> directory to record from.
        #[arg(long)]
        from: PathBuf,
        /// One-line headline; becomes the first line of "## What changed".
        #[arg(long)]
        desc: Option<String>,
        /// Overwrite an existing run dir.
        #[arg(long)]
        force: bool,
        /// Journal dir (default: <git-root>/journal).
        #[arg(long)]
        journal_dir: Option<PathBuf>,
    },
    /// Compare two runs (or a run against the recorded baseline).
    Compare {
        /// First run id (run A).
        run_a: String,
        /// Second run id (run B). Omit and pass --baseline to compare vs baseline.
        run_b: Option<String>,
        /// Compare run_a against baselines.json instead of a second run.
        #[arg(long)]
        baseline: bool,
        /// Regression threshold in percent.
        #[arg(long, default_value_t = 10.0)]
        threshold: f64,
        /// Exit non-zero if any regression is flagged.
        #[arg(long)]
        strict: bool,
        /// Journal dir (default: <git-root>/journal).
        #[arg(long)]
        journal_dir: Option<PathBuf>,
    },
    /// Write baselines.json from a run.
    SetBaseline {
        /// Run id to take the baseline from.
        run: String,
        /// Journal dir (default: <git-root>/journal).
        #[arg(long)]
        journal_dir: Option<PathBuf>,
    },
    /// Regenerate INDEX.md.
    Index {
        /// Journal dir (default: <git-root>/journal).
        #[arg(long)]
        journal_dir: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("journal: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Command::Record {
            from,
            desc,
            force,
            journal_dir,
        } => {
            let journal = resolve_journal_dir(journal_dir)?;
            let out = record::record(&from, &journal, desc.as_deref(), force)?;
            println!("recorded run {} -> {}", out.run_id, out.run_dir.display());
            println!("regenerated {}", journal.join("INDEX.md").display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Compare {
            run_a,
            run_b,
            baseline,
            threshold,
            strict,
            journal_dir,
        } => {
            let journal = resolve_journal_dir(journal_dir)?;
            cmd_compare(
                &journal,
                &run_a,
                run_b.as_deref(),
                baseline,
                threshold,
                strict,
            )
        }
        Command::SetBaseline { run, journal_dir } => {
            let journal = resolve_journal_dir(journal_dir)?;
            let lines = read_run_results(&journal, &run)?;
            let baselines = baseline::build(&run, &lines);
            let path = journal.join("baselines.json");
            std::fs::write(&path, baseline::to_json(&baselines))
                .map_err(|e| format!("writing {}: {e}", path.display()))?;
            println!(
                "wrote {} baseline cells from {run} -> {}",
                baselines.len(),
                path.display()
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Index { journal_dir } => {
            let journal = resolve_journal_dir(journal_dir)?;
            index::regenerate(&journal).map_err(|e| format!("regenerating INDEX.md: {e}"))?;
            println!("regenerated {}", journal.join("INDEX.md").display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn cmd_compare(
    journal: &Path,
    run_a: &str,
    run_b: Option<&str>,
    baseline: bool,
    threshold: f64,
    strict: bool,
) -> Result<ExitCode, String> {
    let run_lines = read_run_results(journal, run_a)?;

    if baseline {
        // A = the recorded baseline (reference), B = the run under test.
        let path = journal.join("baselines.json");
        let body = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let base: baseline::Baselines =
            serde_json::from_str(&body).map_err(|e| format!("parsing baselines.json: {e}"))?;
        let base_lines = baseline_to_lines(&base);
        return render_compare(
            &base_lines,
            &run_lines,
            "baseline",
            run_a,
            threshold,
            strict,
        );
    }

    // A = run_a, B = run_b.
    let run_b = run_b.ok_or("compare needs a second run id, or pass --baseline")?;
    let b_lines = read_run_results(journal, run_b)?;
    render_compare(&run_lines, &b_lines, run_a, run_b, threshold, strict)
}

/// Reconstruct result lines from a baselines map (enough fields for the join).
fn baseline_to_lines(base: &baseline::Baselines) -> Vec<journal::model::ResultLine> {
    base.iter()
        .filter_map(|(key, cell)| {
            let parts: Vec<&str> = key.split('/').collect();
            if parts.len() != 4 {
                return None;
            }
            Some(journal::model::ResultLine {
                focus_area: parts[0].to_string(),
                experiment: parts[1].to_string(),
                language: parts[2].to_string(),
                metric: parts[3].to_string(),
                value: cell.value,
                unit: cell.unit.clone(),
                samples: 0,
                notes: None,
            })
        })
        .collect()
}

fn render_compare(
    a_lines: &[journal::model::ResultLine],
    b_lines: &[journal::model::ResultLine],
    label_a: &str,
    label_b: &str,
    threshold: f64,
    strict: bool,
) -> Result<ExitCode, String> {
    let result = compare::join(a_lines, b_lines, threshold);
    print_table(&result, label_a, label_b);

    for key in &result.added {
        println!("added (only in {label_b}): {key}");
    }
    for key in &result.removed {
        println!("removed (only in {label_a}): {key}");
    }

    let n_reg = result.regressions().count();
    println!("\n{n_reg} regression(s) flagged at threshold {threshold:.1}%.");

    if strict && result.has_regression() {
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Hand-rendered aligned table (no table crate).
fn print_table(result: &compare::JoinResult, label_a: &str, label_b: &str) {
    let header = [
        "cell".to_string(),
        format!("A ({label_a})"),
        format!("B ({label_b})"),
        "abs delta".to_string(),
        "% delta".to_string(),
        "verdict".to_string(),
    ];

    let mut rows: Vec<[String; 6]> = Vec::new();
    for c in &result.shared {
        let pct = match c.pct_delta {
            Some(p) => format!("{p:+.2}%"),
            None => "n/a".to_string(),
        };
        let mut verdict = c.verdict.label().to_string();
        if c.unknown_unit {
            verdict.push_str(" (unknown unit: assumed lower-better)");
        }
        rows.push([
            c.key.to_string(),
            fmt_num(c.a),
            fmt_num(c.b),
            fmt_signed(c.abs_delta),
            pct,
            verdict,
        ]);
    }

    // column widths
    let mut widths = [0usize; 6];
    for (i, h) in header.iter().enumerate() {
        widths[i] = h.len();
    }
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    print_row(&header, &widths);
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    let sep_arr: [String; 6] = sep.try_into().expect("6 columns");
    print_row(&sep_arr, &widths);
    for row in &rows {
        print_row(row, &widths);
    }
    if rows.is_empty() {
        println!("(no shared cells)");
    }
}

fn print_row(cells: &[String; 6], widths: &[usize; 6]) {
    let parts: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{c:<width$}", width = widths[i]))
        .collect();
    println!("{}", parts.join("  "));
}

/// Format a number without a trailing `.0` for integers.
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Like `fmt_num` but with an explicit leading sign (for deltas).
fn fmt_signed(v: f64) -> String {
    let sign = if v >= 0.0 { "+" } else { "-" };
    format!("{sign}{}", fmt_num(v.abs()))
}

/// Read and parse `<journal>/runs/<run>/results.jsonl`.
fn read_run_results(journal: &Path, run: &str) -> Result<Vec<journal::model::ResultLine>, String> {
    let path = journal.join("runs").join(run).join("results.jsonl");
    let body =
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    parse_results(&body)
}

/// Resolve the journal dir: explicit flag, else `<git-root>/journal`.
fn resolve_journal_dir(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let root = find_git_root(&cwd)
        .ok_or("not inside a git repo; pass --journal-dir explicitly".to_string())?;
    Ok(root.join("journal"))
}

/// Walk up from `start` looking for a `.git` entry.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}
