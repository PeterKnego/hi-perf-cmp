# autobench — generic optimization loop

You (Claude Code) are the loop orchestrator. The human supplies a task name;
everything else runs without questions, pauses, or approval. The only stop
signal is Ctrl-C. If the setup itself is broken, fix it and continue.

## Autonomy — DO NOT ASK QUESTIONS, DO NOT PAUSE

**The human is not at the keyboard.** There is nobody to answer a clarifying
question, approve a step, or read a mid-run summary. Once a run is set up, you
execute the loop continuously and silently until the human presses Ctrl-C. This
is the single most-violated rule of this framework, so internalize it:

- **Never ask the user anything** — not "should I continue?", not "which
  hypothesis next?", not "is this setup right?". Resolve every ambiguity
  yourself by picking the most promising untried hypothesis and proceeding.
- **Never stop to summarize or wait for approval** between iterations. After
  you KEEP or DISCARD an iteration and append the TSV row, immediately start
  the next one. Treat every natural stopping point as "begin iteration N+1".
- **Never wait for confirmation to commit a win or revert a loss.** The
  decision rule below is unambiguous — just execute it.
- **The only stop signal is Ctrl-C.** Until then, GOTO the next iteration.
- If the *harness/setup itself* is broken (harness won't build, TSV missing,
  wrong branch, bad CLI args), fix it and keep going — still without asking.

## Ground rules

- **Run from the PRIMARY checkout root on a dedicated branch
  `autoresearch/<task>-<tag>`** — never from a git worktree. `run-iter` resolves
  the cell artifact via `cargo run` from `rust/`; running elsewhere measures the
  wrong tree.
- **Never edit frozen paths.** For the pilot the only mutable path is the cell's
  source (`rust/network-rtt/tcp/src/**`). These are always frozen:
  `rust/bench-common/**`, the result contract (`docs/result-contract.md`), every
  other benchmark cell, `autobench/**` itself, and all docs/specs. See the task
  overlay (`tasks/<task>/program.md`) for the exact mutable/frozen lists.
- **Never add a dependency to the cell's `Cargo.toml`.** The cells are
  intentionally std-only (beyond `bench-common`); optimizing the *code* is the
  game, not pulling in a library.
- **Never modify `run-iter`, the result contract, or `bench-common`** to win a
  number — that is the Goodhart trap and it invalidates the comparison grid.

## Setup (run once at the start of a new run)

1. Read `autobench/tasks/<task>/program.md` for the task's constraints (mutable
   paths, primary/secondary metrics, kind, noise note, TSV schema).
2. Confirm the working tree is clean (`git status`). If not, revert or stash
   strays yourself and continue — never stop to ask.
3. Pick a run tag from today's date (e.g. `jun25`). The branch
   `autoresearch/<task>-<tag>` must not already exist.
4. Create the branch: `git checkout -b autoresearch/<task>-<tag>` from `main`.
5. If `autobench/tasks/<task>/results.tsv` is missing, create it with the header
   row declared in the task overlay.
6. Begin the loop immediately. Do NOT wait for confirmation.

## State

- **Branch:** `autoresearch/<task>-<tag>` (created from `main` at setup).
- **`autobench/tasks/<task>/results.tsv`** — one row per iteration, committed
  every iteration. **Champion = the row with the best `primary` value among
  `status=keep` rows** (direction per the task overlay; pilot minimizes
  `rtt_p50_ns`). The `commit` column is the git SHA — check it out to inspect
  the winning code.
- Git is the only other state.

## Subagent dispatch (MANDATORY)

The orchestrator context must stay small: **it never reads source files or raw
bench output itself.** Dispatch heavy steps to subagents via the Agent tool and
keep only their summaries. This is what lets a run go for hours without context
exhaustion.

| Step | Agent model | Notes |
|------|-------------|-------|
| Hypothesis generation | opus | Input: champion description, last ~10 TSV rows, any hotspot summary. Output: one-line hypothesis + file-level sketch. |
| Implementation | sonnet | Prompt includes the hypothesis, the task's mutable paths, and constraints. **Escalate to opus** for unsafe / lock-free / syscall-ordering work, or after a sonnet attempt fails to build twice. Edits ONLY the cell's mutable paths. |
| Failure triage | haiku | Summarize `stderr_tail` to ≤5 lines. If the fix looks trivial (typo, import), dispatch a sonnet fix attempt; **max 2 attempts**, then log as crash. |

Rules: escalate-on-failure (haiku → sonnet → opus; never start at the top for
mechanical work); subagents absorb file dumps, only summaries return to the
orchestrator.

## Per-iteration sequence

1. Read state: `tail -20 autobench/tasks/<task>/results.tsv`; identify the
   champion (best primary among `status=keep`).
2. **Hypothesis subagent (opus)** → one-line hypothesis + file sketch.
3. **Implementation subagent (sonnet; escalate to opus per the table)** edits
   ONLY the task's mutable paths.
4. Run the harness:
   ```
   cargo run --manifest-path autobench/Cargo.toml --bin run-iter --release -- \
     --task <task> --json --baseline-primary <champion_primary> \
     > /tmp/run-iter.json 2>/dev/null
   ```
   (Run from the repo root. Omit `--baseline-primary` on the first iteration.)
5. Parse: `jq '.status, .primary, .metrics, .stderr_tail' /tmp/run-iter.json`.
6. Decide (comparisons use the median-of-N the harness already took; when a
   delta is within run-to-run noise, re-run `run-iter` for fresh samples before
   deciding):
   - `status=pass` AND `primary` improved beyond noise AND `metrics` did not
     regress the secondaries → **KEEP**: append a TSV row with `status=keep` and
     a one-line `description` (note N), then
     `git add -A && git commit -m "<task>: <description>"`.
   - `status=pass` but no improvement (or a secondary regressed) → **DISCARD**:
     `git checkout -- <mutable_paths>`, append a TSV row with `status=discard`,
     `git add` the TSV and commit `discard: <description>`.
   - `*_failed` / `timeout` → **haiku triage**, ≤2 fix attempts, else append a
     `status=crash` row, revert mutable paths, commit the TSV.
   - Use `git checkout -- <paths>` for reverts, not `git reset --hard` (which
     would drop the TSV row you just added).
7. GOTO 1.

## Graduation (the real, reportable number)

The per-iteration fitness is a two-process run over `127.0.0.1` — fast and
meaningful for *relative* optimization, but loopback is not a cross-host number.
**It is NOT run every iteration.** When a champion **plateaus** (several
iterations with no improvement), or on an explicit instruction:

1. Trigger a **bench-infra AWS cross-host run** for this cell (server and client
   on separate hosts) — the real network measurement.
2. **Record that result in `tools/journal`** as the cell's reportable number,
   referencing the champion commit. The journal — not this loop — owns the
   curated cross-host, cross-language record and the regression-vs-baseline
   comparison.

The loop's `results.tsv` is the local fitness log; the journal is the truth.

## Rules

- NEVER stop on your own and NEVER ask the user a question (see "Autonomy").
  The human interrupts when satisfied. If you're stuck, think harder: re-read
  the in-scope cell source, combine prior near-misses, try more radical
  structural changes (syscall batching, buffer reuse, nodelay/cork tuning,
  read/write strategy).
- Simplicity wins: a small gain that adds ugly complexity is not worth it. A
  wash that deletes code is always a keep.
- Frozen correctness semantics (the echo-byte equality the artifact asserts) are
  never removed to win a number.
