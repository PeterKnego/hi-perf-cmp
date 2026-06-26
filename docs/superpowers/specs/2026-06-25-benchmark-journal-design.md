# Benchmark Journal — Design

**Date:** 2026-06-25 (implemented; spec reconciled to as-built 2026-06-26)
**Status:** Implemented

> As-built note: this spec matches the shipped `tools/journal` CLI and the
> `journal/` tree. Two refinements landed after the initial proposal and are
> reflected below: `record` now auto-embeds a `## Results` digest in each
> `entry.md`, and `INDEX.md` leads with a human-readable timestamp + headline
> (the run-id, linked to its entry, and sha moved to the end).

## Purpose

Track benchmark performance over time so we can (a) see what impact each change
to an experiment had, correlated with the exact code version, and (b) catch and
remember regressions so they are not reintroduced. The mechanism: a **committed,
version-linked journal** of curated benchmark runs plus a **Rust CLI** to record
runs and compare them.

Git becomes the time machine — each journal entry is committed alongside (or
referencing) the code that produced it. Results are small (JSON lines), so
committing them is cheap and keeps history self-contained.

This **realized the `harness/` placeholder**: the "aggregate + compare results
across the matrix" role stubbed there is exactly `journal compare`. The
`harness/` directory was retired and its README pointer removed; the journal is
its evolution.

## Inputs (produced by bench-infra)

`bench-infra`'s `collect` role pulls each run to `bench-out/dist/<ts>/`:
- `results.jsonl` — one result-contract line per `(language, focus_area,
  experiment, metric)`.
- `manifest.txt` — provenance: `git_sha`, `instance_type`, `vcpus`, `kernel`,
  the `rtt_*` params, languages, experiments, node roles, timestamp.

`bench-out/` is gitignored (transient). The journal is the curated layer on top.

## Journal layout (committed, repo root)

```
journal/
├── README.md             # how the journal works + the workflow
├── runs/
│   └── <UTC-ts>-<short-sha>/
│       ├── results.jsonl # copied verbatim from the run
│       ├── manifest.txt  # copied verbatim
│       └── entry.md      # provenance + auto Results digest + author narrative
├── baselines.json        # reference value per cell, + which run it came from
├── REGRESSIONS.md        # registry of confirmed regressions: cause, fix, guard
└── INDEX.md              # GENERATED chronological table of runs
```

A **run id** is `<UTC-ts>-<short-sha>` (timestamp + first 12 chars of the commit
from the manifest; falls back to `notime` / `nogit` when absent).

### `entry.md`

`record` pre-fills the provenance, **auto-generates the `## Results` digest from
the run's `results.jsonl`**, and leaves the prose sections (`Hypothesis`,
`Observations`) as headers for the author. `--desc` fills the first line of
`## What changed` (and becomes the INDEX headline).

```
# <run-id>

- commit: <sha>            (link to the experiment version)
- instance: <type>, <vcpus> vCPU, kernel <kver>
- params: payload=<n>B warmup=<n> iterations=<n>

## What changed
<one-paragraph description; --desc fills the first line>

## Results
Per-cell values from this run (placeholder/stub cells omitted).

### <focus_area> / <experiment>

| language | <metric> (<unit>) | … |
|---|---|---|
| <lang>   | <value>           | … |

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
```

The **`## Results` digest** is generated, not hand-written: lines are grouped by
`(focus_area, experiment)`; within each group a `language × metric` table lists
the values (unit in the column header; integer-vs-`.0` float rendered cleanly;
missing cells show `—`). Placeholder/stub cells (`experiment == "placeholder"`
or value `0`) are omitted, and the whole section is dropped for an all-stub run.
This keeps the numbers in the entry — the journal is readable without parsing
`results.jsonl`.

## The `journal` CLI (Rust)

A standalone Rust crate at `tools/journal/` (a binary named `journal`, **not** a
member of the `rust/` benchmark workspace — keeps benchmark builds and the
bench-infra rsync/build untouched). Edition 2024, clippy/fmt-clean per repo
convention. Dependencies (tool-only, isolated from benchmarks): `clap` (args),
`serde` + `serde_json` (parse `results.jsonl` / `baselines.json`); table output
is hand-rendered markdown to keep deps minimal.

It locates the `journal/` dir relative to the repo root (walk up to the git root,
or accept `--journal-dir`).

### Verbs

- **`record --from <bench-out/dist/<ts>> [--desc "<headline>"] [--force]`**
  Reads the manifest for sha+timestamp, parses `results.jsonl`, creates
  `journal/runs/<run-id>/`, copies `results.jsonl` + `manifest.txt`, writes
  `entry.md` (provenance + auto `## Results` digest + prose headers), and
  regenerates `INDEX.md`. Refuses to overwrite an existing run id unless
  `--force`.

- **`compare <runA> <runB>` | `compare <run> --baseline` [`--threshold <pct>`]
  [`--strict`]**
  Joins the two result sets on the cell key `(focus_area, experiment, language,
  metric)`, prints an aligned table of `A`, `B`, absolute delta, % delta, and a
  direction-aware verdict. **Flags regressions** beyond the threshold (default
  10%). Cells present in only one side are listed as added/removed. Advisory by
  default (exit 0); `--strict` exits non-zero if any regression is flagged (for
  optional CI use).

- **`set-baseline <run>`**
  Writes `baselines.json` from that run's cells (value + unit + originating run
  id per cell).

- **`index`** — regenerate `INDEX.md` (also done implicitly by `record`).

### `INDEX.md` (generated)

A newest-first table (run-id is timestamp-prefixed, so lexical sort is
chronological). Columns, in order:

| timestamp | what changed | run-id | sha |
|-----------|--------------|--------|-----|

- **timestamp** — human-readable `YYYY-MM-DD HH:MM:SS UTC` (the manifest's compact
  `YYYYMMDDThhmmssZ` is reformatted; other shapes pass through unchanged).
- **what changed** — the `--desc` headline (pipes escaped), so the index is
  scannable without opening each entry.
- **run-id** — a markdown link to `runs/<run-id>/entry.md`.
- **sha** — short commit.

The human-facing columns (timestamp + headline) lead; the run-id/sha identifiers
follow at the end.

### Metric direction (regression semantics)

Regression = a change in the *worse* direction beyond the threshold. Direction is
inferred from `unit`:
- **lower-is-better:** `ns`, `us`, `ms`, `s` (latency) → an increase is a
  regression.
- **higher-is-better:** `ops_per_sec`, `bytes_per_sec` (throughput) → a decrease
  is a regression.
Unknown units default to lower-is-better with a printed note. (`placeholder`
stub lines — value 0 — are skipped in comparisons.)

### Regression handling (advisory + registry)

`compare` only *flags* regressions; by default it does not block (`--strict` is
opt-in). Confirmed regressions are recorded by hand in `REGRESSIONS.md`, one
entry each: date, affected cell(s), magnitude, root cause, the fix/commit, and a
short "guard" note describing how to avoid reintroducing it. This registry is the
institutional memory; `compare` is the detector.

## Workflow

```
# after a bench-infra run lands in bench-out/dist/<ts>/
journal record --from bench-out/dist/<ts> --desc "udp: batch syscalls via sendmmsg"
$EDITOR journal/runs/<run-id>/entry.md      # fill hypothesis/observations
                                            # (provenance + Results are pre-filled)
journal compare <run-id> --baseline          # see deltas vs the reference
# if it's the new reference:
journal set-baseline <run-id>
git add journal/ && git commit               # the entry is now version-linked
```

## Testing

- **Unit tests (in `src/`):** results.jsonl + manifest parsing; cell-key join;
  delta & %-delta math; direction-aware regression detection (latency up =
  regression, throughput down = regression, threshold boundary, unknown unit);
  `INDEX.md` rendering (column order, human-readable timestamp, entry link);
  `## Results` digest (tabulation, placeholder omission, empty for all-stub).
- **Integration fixtures:** small synthetic `bench-out` dirs under
  `tools/journal/tests/fixtures/` (`baseline-run`, `regressed-run`) drive an
  end-to-end `record` → `compare` test (`tests/integration.rs`) asserting the
  regression is flagged.
- Tool stays clippy/fmt-clean.

## Docs

- `journal/README.md` — the workflow above + layout.
- Root `README.md` and `CLAUDE.md` document the `journal record/compare/set-baseline`
  workflow and that results are committed under `journal/runs/`; the `harness/`
  references were replaced by the journal.
- `docs/result-contract.md` notes the journal consumes these lines.

## Out of scope (YAGNI)

- Auto-recording every run (curated milestones only).
- Plots/dashboards/web UI (the CLI prints tables; INDEX.md is the overview, the
  per-entry `## Results` digest is the per-run view).
- Statistical significance testing across repeated runs (threshold on point
  estimates is enough for now; cloud variance is noted in entry observations).
- A hard CI gate (advisory by default; `--strict` exists for opt-in use).
- A `make record` convenience wrapper in `bench-infra/` (the `journal record`
  call is short enough; not added).
