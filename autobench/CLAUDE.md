# autobench

Claude-Code-driven autoresearch loop for hi-perf-cmp benchmark cells.
Karpathy/autoresearch shape: Claude Code proposes code changes to one cell, the
`run-iter` harness measures them over `127.0.0.1`, and the loop commits wins /
reverts losses indefinitely until the human presses Ctrl-C. Modeled on
`ultima_db/autobench`.

A **task is a matrix cell** `(focus_area, experiment, language)`. The pilot is
`rust-network-rtt-tcp`. Both `Network` (two-process `127.0.0.1`) and `Local`
(single-host, single-process — e.g. `rust-thread-handoff-spin`,
`rust-thread-handoff-ring`) cells are supported. Adding a cell is a data-only
change (a `TaskSpec` row + a task overlay) — see "Adding a task".

## Starting a run

**Start from the PRIMARY repo root — not a git worktree.** `run-iter` resolves
the cell artifact via `cargo run` from `rust/`; a worktree would measure the
wrong tree.

Open Claude Code in the primary `hi-perf-cmp/` checkout and prompt:

```
Run the autobench loop for task rust-network-rtt-tcp per autobench/program.md.
```

That's the entire invocation. The loop reads `program.md`, creates a branch
`autoresearch/<task>-<tag>`, and runs indefinitely. Interrupt with Ctrl-C when
satisfied.

## Reading results

Results live in `autobench/tasks/<task>/results.tsv` — one row per iteration,
committed every iteration.

**Columns (rust-network-rtt-tcp):**
`commit | rtt_p50_ns | rtt_p99_ns | rtt_mean_ns | status | description`

**Local cells** (`rust-thread-handoff-spin`: minimize `handoff_rtt_p50_ns`;
`rust-thread-handoff-ring`: maximize `handoff_throughput_ops_per_sec`) run the
artifact as a single process — no server/client — and key metrics `<metric>_<unit>`.

**Champion:** the row with the best `primary` value among `status=keep` rows.
The pilot **minimizes** `rtt_p50_ns`. The `commit` column is the git SHA —
check it out to inspect the winning code. Values are median-of-N (N noted in
`description`).

**Why was variant X rejected?** Find its `status=discard` or `status=crash`
row and read `description`; cross-reference the matching commit in `git log`.

## run-iter quick reference

```
cargo run --manifest-path autobench/Cargo.toml --bin run-iter --release -- \
  --task rust-network-rtt-tcp \
  --json \
  --baseline-primary <champion_primary> \
  --samples 5
```

Flags:
- `--task`: task id (pilot: `rust-network-rtt-tcp`).
- `--json`: required; emits one JSON `Verdict` on stdout (exit 0 even on failure).
- `--baseline-primary`: champion's primary value; recorded for context. Omit on
  the first iteration.
- `--samples`: median-of-N for the microbench fitness (default 5).

Run it from anywhere in the checkout; the harness resolves each stage's cwd
(relative to the git root) from the `TaskSpec`.

**JSON verdict fields:**

| Field | Type | Meaning |
|-------|------|---------|
| `status` | string | Overall result (see statuses below) |
| `stage` | string | Last stage that ran (`setup`/`build`/`correctness`/`microbench`/`tests`) |
| `duration_s` | object | Per-stage wall times: `build`, `correctness`, `microbench`, `tests` |
| `metrics` | object | Median-of-N metrics: `rtt_p50_ns`, `rtt_p99_ns`, `rtt_mean_ns` |
| `primary` | number? | The primary metric value (median-of-N) — `rtt_p50_ns` for the pilot |
| `stderr_tail` | string? | Last ~50 lines of stderr+stdout on failure |

**Statuses:**

| Status | TSV status | Meaning |
|--------|-----------|---------|
| `pass` | keep/discard | All stages ran; compare `primary`/`metrics` to decide |
| `build_failed` | crash | The cell's build command failed |
| `correctness_failed` | crash | The two-process smoke didn't exit 0 or didn't yield 3 positive contract lines |
| `microbench_failed` | crash | A fitness sample failed to run / parse |
| `tests_failed` | crash | Gate A (`cargo test`) failed |
| `timeout` | crash | A stage exceeded its hard wall-clock budget |
| `unknown_task` | crash | `--task` value not in `task_spec.rs` |

## Subagent / model dispatch

| Step | Model | Notes |
|------|-------|-------|
| Hypothesis generation | opus | Champion description + last ~10 TSV rows + hotspot summary → one-line hypothesis + file sketch |
| Implementation | sonnet | Escalate to opus for unsafe / lock-free / syscall-ordering work, or after two failed build attempts |
| Failure triage | haiku | Summarize `stderr_tail` to ≤5 lines; dispatch a sonnet fix for trivial errors (max 2 attempts) |

The orchestrator never reads source or raw bench output itself — only subagent
summaries (so a run survives hours without context exhaustion).

## Graduation

The fast loop fitness is a two-process `127.0.0.1` run — meaningful for
*relative* optimization, but not a cross-host number. When a champion plateaus,
trigger a **bench-infra AWS cross-host run** for the cell and record it in
`tools/journal` as the real, reportable number. This is a manual graduation
step, not a per-iteration gate (see `program.md`).

## Adding a task

See `autobench/tasks/TEMPLATE.md` for the full guide. Summary: add a `TaskSpec`
row in `src/task_spec.rs`, write `tasks/<id>/program.md` (the overlay), and
create `tasks/<id>/results.tsv` with the header row only. A Go or Java cell
differs only in the `build`/`run`/`gate_a` argv.
