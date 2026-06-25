# Benchmark Journal

A committed, version-linked record of benchmark runs — so we can see what each
change to an experiment did, correlated with the exact code that produced it,
and remember regressions so they aren't reintroduced.

Git is the time machine: each run is committed with the commit it was built from
(recorded in its `manifest.txt`), and the `journal compare` tool shows deltas
between runs and flags regressions.

## Layout

```
journal/
├── runs/<UTC-ts>-<short-sha>/    one curated run
│   ├── results.jsonl            raw result-contract lines (see docs/result-contract.md)
│   ├── manifest.txt             provenance: commit, instance, kernel, params
│   └── entry.md                 narrative: what changed, hypothesis, observations
├── baselines.json               reference value per cell (focus_area/experiment/language/metric)
├── REGRESSIONS.md               registry of confirmed regressions + root cause/fix
└── INDEX.md                     generated chronological table of runs
```

A **cell** is `(focus_area, experiment, language, metric)` — the unit of
comparison across the experiment × language matrix.

We journal **curated milestone runs** (tied to a change worth remembering), not
every run. Ad-hoc runs stay in the gitignored `bench-out/`.

## Tool

The `journal` CLI lives at `tools/journal/` (a standalone Rust crate). Build it
with `cargo build --release` there; the binary is `journal`.

## Workflow

```sh
# after a bench-infra run lands in bench-out/dist/<ts>/
journal record --from bench-out/dist/<ts> --desc "udp: batch syscalls via sendmmsg"
$EDITOR journal/runs/<run-id>/entry.md       # fill in hypothesis / observations
journal compare <run-id> --baseline           # deltas vs the reference; flags regressions
journal set-baseline <run-id>                  # if this becomes the new reference
git add journal/ && git commit                 # the entry is now version-linked
```

Regressions are **advisory**: `compare` flags them; confirmed ones get a line in
[REGRESSIONS.md](REGRESSIONS.md) with root cause and a guard note.

See the design: `docs/superpowers/specs/2026-06-25-benchmark-journal-design.md`.
