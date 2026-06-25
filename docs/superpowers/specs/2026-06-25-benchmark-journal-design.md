# Benchmark Journal — Design

**Date:** 2026-06-25
**Status:** Proposed — awaiting review

## Purpose

Track benchmark performance over time so we can (a) see what impact each change
to an experiment had, correlated with the exact code version, and (b) catch and
remember regressions so they are not reintroduced. The mechanism: a **committed,
version-linked journal** of curated benchmark runs plus a **Rust CLI** to record
runs and compare them.

Git becomes the time machine — each journal entry is committed alongside (or
referencing) the code that produced it. Results are small (JSON lines), so
committing them is cheap and keeps history self-contained.

This **realizes the `harness/` placeholder**: the "aggregate + compare results
across the matrix" role stubbed there is exactly `journal compare`. The
`harness/` directory is retired and its README pointer removed; the journal is
its evolution.

## Inputs (already produced by bench-infra)

`bench-infra`'s `collect` role pulls each run to `bench-out/dist/<ts>/`:
- `results.jsonl` — one result-contract line per `(language, focus_area,
  experiment, metric)`.
- `manifest.txt` — provenance: `git_sha`, `instance_type`, `vcpus`, `kernel`,
  the `rtt_*` params, languages, experiments, timestamp.

`bench-out/` is gitignored (transient). The journal is the curated layer on top.

## Journal layout (committed, repo root)

```
journal/
├── README.md             # how the journal works + the workflow
├── runs/
│   └── <UTC-ts>-<short-sha>/
│       ├── results.jsonl # copied verbatim from the run
│       ├── manifest.txt  # copied verbatim
│       └── entry.md      # human narrative (see template below)
├── baselines.json        # reference value per cell, + which run it came from
├── REGRESSIONS.md        # registry of confirmed regressions: cause, fix, guard
└── INDEX.md              # GENERATED chronological table of runs
```

A **run id** is `<UTC-ts>-<short-sha>` (timestamp + commit from the manifest).

`entry.md` template (record pre-fills the commit/sha; the author fills the prose):
```
# <run-id>

- commit: <sha>            (link to the experiment version)
- instance: <type>, <vcpus> vCPU, kernel <kver>
- params: payload=<n>B warmup=<n> iterations=<n>

## What changed
<one-paragraph description of what was added/changed in this version>

## Hypothesis
<what we expected to happen>

## Observations
<what actually happened; reference compare output / notable deltas>
```

The `--desc` headline also lands in `INDEX.md` so the index is scannable without
opening each entry.

## The `journal` CLI (Rust)

A standalone Rust crate at `tools/journal/` (a binary named `journal`, **not** a
member of the `rust/` benchmark workspace — keeps benchmark builds and the
bench-infra rsync/build untouched). Edition 2024, clippy/fmt-clean per repo
convention. Dependencies (tool-only, isolated from benchmarks): `clap` (args),
`serde` + `serde_json` (parse `results.jsonl` / `baselines.json`); table output
is hand-rendered (aligned columns) to keep deps minimal.

It locates the `journal/` dir relative to the repo root (walk up to the git root,
or accept `--journal-dir`).

### Verbs

- **`record --from <bench-out/dist/<ts>> [--desc "<headline>"]`**
  Reads the manifest for sha+timestamp, creates `journal/runs/<run-id>/`, copies
  `results.jsonl` + `manifest.txt`, writes a pre-filled `entry.md` template, and
  regenerates `INDEX.md`. Refuses to overwrite an existing run id unless
  `--force`.

- **`compare <runA> <runB>` | `compare <run> --baseline`**
  Joins the two result sets on the cell key `(focus_area, experiment, language,
  metric)`, prints an aligned table of `A`, `B`, absolute delta, % delta, and a
  direction-aware verdict. **Flags regressions** beyond a threshold (default
  10%, `--threshold <pct>`). Cells present in only one side are listed as
  added/removed. Exit code is always 0 (advisory — see decision below); a
  `--strict` flag may flip it to non-zero on any flagged regression for optional
  CI use, but the default workflow is advisory.

- **`set-baseline <run>`**
  Writes `baselines.json` from that run's cells (value + unit + originating
  run id per cell).

- **`index`** — regenerate `INDEX.md` (also done implicitly by `record`).

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

`compare` only *flags* regressions; it does not block. Confirmed regressions are
recorded by hand in `REGRESSIONS.md`, one entry each: date, affected cell(s),
magnitude, root cause, the fix/commit, and a short "guard" note describing how to
avoid reintroducing it. This registry is the institutional memory; `compare` is
the detector.

## Workflow

```
# after a bench-infra run lands in bench-out/dist/<ts>/
journal record --from bench-out/dist/<ts> --desc "udp: batch syscalls via sendmmsg"
$EDITOR journal/runs/<run-id>/entry.md      # fill in hypothesis/observations
journal compare <run-id> --baseline          # see deltas vs the reference
# if it's the new reference:
journal set-baseline <run-id>
git add journal/ && git commit               # the entry is now version-linked
```

An optional `make record TS=<ts> DESC="..."` convenience target in `bench-infra/`
wraps the `record` call; not required.

## Testing

- **Unit tests (TDD):** results.jsonl + manifest parsing; cell-key join; delta &
  %-delta math; direction-aware regression detection (latency up = regression,
  throughput down = regression, threshold boundary, unknown unit); INDEX/table
  rendering of a known fixture.
- **Fixtures:** a couple of small synthetic `bench-out` dirs under
  `tools/journal/tests/` (e.g. a baseline and a regressed run) to drive an
  end-to-end `record` → `compare` test asserting the regression is flagged.
- Tool stays clippy/fmt-clean.

## Docs

- `journal/README.md` — the workflow above + layout.
- Update root `README.md` and `CLAUDE.md`: replace the `harness/` references with
  the journal; document the `journal record/compare/set-baseline` workflow and
  that results are committed under `journal/runs/`.
- `docs/result-contract.md` gets a back-reference noting the journal consumes
  these lines.

## Out of scope (YAGNI)

- Auto-recording every run (curated milestones only).
- Plots/dashboards/web UI (the CLI prints tables; INDEX.md is the overview).
- Statistical significance testing across repeated runs (threshold on point
  estimates is enough for now; cloud variance is noted in entry observations).
- A hard CI gate (advisory by default; `--strict` exists if wanted later).
