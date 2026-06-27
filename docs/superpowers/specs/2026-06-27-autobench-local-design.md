# autobench `Local`-kind support + thread-handoff cells — Design

**Date:** 2026-06-27
**Status:** Proposed — awaiting review

## Purpose

Extend the autobench autoresearch loop (`autobench/`) to optimize **single-host
(`Local`) benchmark cells**, then wire up two of them — `rust-thread-handoff-spin`
and `rust-thread-handoff-ring` — and run the loop to improve the Rust
`thread-handoff` `spin` and `ring` experiments.

Today autobench only supports `Network` cells (two-process `127.0.0.1` fitness over
the `RTT_*` contract). `thread-handoff` is a `Local` focus area: one process emits
the contract lines directly. `Kind::Local` exists in the enum but is unimplemented —
`run-iter` bails with `"kind Local correctness not implemented"`, the fitness driver
is two-process only, the correctness floor demands *exactly 3* contract lines, and
metric keys are hardcoded `<metric>_ns`. `ring` additionally emits a single
`handoff_throughput` (`ops_per_sec`) line and must **maximize** it.

This is a **harness extension**, not a rewrite: the change is data-driven and
**backward-compatible** — the nine existing `Network` cells (including the already-run
`rust-network-rtt-tcp`, whose `results.tsv` carries a real `rtt_p50_ns` history) keep
working unchanged.

## Scope

- Extend the autobench harness so `Kind::Local` cells run end-to-end (build →
  correctness → median-of-N microbench → Gate A).
- Add two `Local` cells: `rust-thread-handoff-spin`, `rust-thread-handoff-ring`.
- Run the capped optimization loop on each (spin first, then ring).

Out of scope (YAGNI): the other thread-handoff cells (`condvar`, `channel`), the
Go/Java thread-handoff cells, `filesystem-write` cells, and any change to the
graduation/AWS/journal flow. Those become data-only follow-ups once `Local` works.

## Harness extension (the `autobench` crate)

### 1. `TaskSpec` gains data fields (`src/task_spec.rs`)

So `run-iter` stops hardcoding `RTT_*` env and an "exactly 3 lines" floor:

| field | type | meaning | network value | spin | ring |
|---|---|---|---|---|---|
| `warmup_env` | `&str` | env var for the warmup count | `RTT_WARMUP` | `TH_WARMUP` | `TH_WARMUP` |
| `iters_env` | `&str` | env var for the iteration count | `RTT_ITERATIONS` | `TH_ITERATIONS` | `TH_ITERATIONS` |
| `extra_env` | `&[(&str,&str)]` | fixed per-cell env | `[]` | `[]` | `[("TH_RING_CAP","1024")]` |
| `expected_metrics` | `&[&str]` | suffixed metric keys the cell must emit | `["rtt_p50_ns","rtt_p99_ns","rtt_mean_ns"]` | `["handoff_rtt_p50_ns","handoff_rtt_p99_ns","handoff_rtt_mean_ns"]` | `["handoff_throughput_ops_per_sec"]` |
| `primary_key` | `&str` | suffixed key of the champion value | `rtt_p50_ns` | `handoff_rtt_p50_ns` | `handoff_throughput_ops_per_sec` |

The existing `primary_metric` (bare name) stays for labels. The nine network rows get
these fields filled in mechanically; their `primary_key` is `rtt_p50_ns` — identical
to today's computed `format!("{}_ns", primary_metric)`, so **no behavior change**.

### 2. Unit-aware metric keying (`src/sampling.rs::parse_contract_metrics`)

Key the metrics map by `<metric>_<unit>` (reading the always-present `unit` field)
instead of the hardcoded `<metric>_ns`:

- `rtt_p50` + `ns` → `rtt_p50_ns` — **unchanged** (network history preserved).
- `handoff_rtt_p50` + `ns` → `handoff_rtt_p50_ns`.
- `handoff_throughput` + `ops_per_sec` → `handoff_throughput_ops_per_sec`.

`Direction::Maximize` already exists and is tested, so `ring` (higher-is-better) needs
no new comparison logic — the cell's row just sets `direction: Maximize`.

### 3. A `Local` single-run driver (`src/sampling.rs`)

Add `run_local_once(run, run_dir, env) -> io::Result<LocalRun { ok, stdout, stderr }>`:
spawn the artifact once with the given env, capture stdout/stderr, return the captured
output and exit status. No port, no readiness probe, no server child — much simpler
than `run_network_once`. (A `LocalRun` struct mirrors `NetworkRun`'s `{ok, stdout,
stderr}` shape.)

### 4. `run-iter` `Local` branch (`src/bin/run-iter.rs`)

Replace the `kind != Network → correctness_fail` stub with a real branch in both
`correctness` and `microbench`:

- **correctness (Local):** `run_local_once` with `{warmup_env: SMOKE_WARMUP,
  iters_env: SMOKE_ITERATIONS}` + `extra_env`; require exit 0, then require every key
  in `expected_metrics` present in the parsed map and `> 0`. (Replaces the hardcoded
  count check; `spin` has 3 expected, `ring` has 1.)
- **microbench (Local):** `samples` runs of `run_local_once` with `{warmup_env:
  BENCH_WARMUP, iters_env: BENCH_ITERATIONS}` + `extra_env`; per run require
  `expected_metrics` present; take the median per key; set `primary` from
  `primary_key`. Reuses the existing `SMOKE_*`/`BENCH_*` constants (mapped through
  `warmup_env`/`iters_env`), so no new count config.

The `Network` path is refactored minimally so `correctness`/`microbench` dispatch on
`spec.kind` — the two-process logic is unchanged. The frozen `bench-common::measure`
loop still owns all timing; the optimizer can only edit the cell's transport.

## The two `Local` cells

Each is a `TaskSpec` row + a `tasks/<id>/` overlay (a `program.md` modeled on the
network overlay + a header-only `results.tsv`).

| | `rust-thread-handoff-spin` | `rust-thread-handoff-ring` |
|---|---|---|
| kind | `Local` | `Local` |
| build (`rust/`) | `cargo build --release -p thread-handoff-spin` | `cargo build --release -p thread-handoff-ring` |
| run (`rust/`) | `cargo run --release -q -p thread-handoff-spin` | `cargo run --release -q -p thread-handoff-ring` |
| gate_a (`rust/`) | `cargo test` | `cargo test` |
| primary / direction | `handoff_rtt_p50_ns` / **Minimize** | `handoff_throughput_ops_per_sec` / **Maximize** |
| expected_metrics | 3 × `handoff_rtt_*_ns` | `handoff_throughput_ops_per_sec` |
| extra_env | `[]` | `[("TH_RING_CAP","1024")]` |
| mutable | `rust/thread-handoff/spin/src/**` | `rust/thread-handoff/ring/src/**` |

`results.tsv` headers:
- spin: `commit\thandoff_rtt_p50_ns\thandoff_rtt_p99_ns\thandoff_rtt_mean_ns\tstatus\tdescription`
- ring: `commit\thandoff_throughput_ops_per_sec\tstatus\tdescription`

Frozen for both (per the autobench TEMPLATE): `rust/bench-common/**` (incl. the
`measure`/emit path), the result contract, every other cell, `autobench/**`, all
docs. No new dependency in the cell's `Cargo.toml`.

## Anti-Goodhart (the correctness floor)

`spin`'s mutable `main.rs` holds both the responder loop and the timing closure, so a
naive "improvement" could lower latency by not actually synchronizing. Defenses:

1. **Structural:** the responder loops exactly `warmup+iterations` and the timer drives
   the same count — removing the synchronization tends to deadlock → the stage exceeds
   its timeout → `Timeout`/reject.
2. **Floor:** every `expected_metric` present and `> 0`, exit 0.
3. **Gate A:** `cargo test` over the workspace. For `ring` this includes the SPSC
   `spsc_preserves_order_and_count` test — breaking ordering fails Gate A. (`spin` has
   no equivalent unit test.)
4. **Orchestrator review:** because the run is **capped** (≤10 iterations/cell), the
   loop orchestrator reads each **KEPT** diff and confirms it is a genuine mechanism
   change, not a correctness shortcut, before accepting the champion. Each cell's
   `program.md` names its specific Goodhart trap (spin: "must still wait for the real
   cross-thread echo each round trip; do not remove the round-trip wait").

This is the agreed level; a fully self-policing alternative (move the ping-pong driver
+ a sequence/echo assertion into frozen `bench-common`) is deferred.

## The run (orchestration)

Run on a branch `autoresearch/thread-handoff-<tag>`, **spin first, then ring**. Per
cell, the capped plateau loop (mirrors `autobench/program.md`):

1. read the cell's `results.tsv`; champion = best `primary` among `status=keep`
   (first iteration: baseline the unmodified cell).
2. **opus** hypothesis subagent → one-line hypothesis + file sketch (champion
   description + last ~10 TSV rows).
3. **sonnet** implementer subagent edits **only** the cell's mutable `src/**`
   (escalate to opus for unsafe/lock-free/ordering work or after 2 failed builds).
4. `run-iter --task <id> --json --baseline-primary <champion> --samples 5` → parse the
   verdict.
5. decide:
   - `pass` & `primary` improves beyond noise (direction-aware) & Gate A green →
     **KEEP**: orchestrator reads the diff (anti-Goodhart), appends a TSV row, commits.
   - `pass` & no improvement → **DISCARD**: `git checkout -- <mutable>`, append a
     discard row, commit the TSV.
   - `*_failed`/`timeout` → **haiku** triage → ≤2 sonnet fix attempts → else **crash**
     row + revert.
6. **stop** after 3 consecutive non-KEEP iterations OR 10 iterations, whichever first.

Values are median-of-5; `Local` latency cells are noisy on a dev box (scheduler), so
treat within-noise deltas as washes and re-run when ambiguous — same discipline the
network overlays use. These are **local fast-loop** numbers (fully meaningful for a
single-host focus area); a graduated champion can later go through an AWS run +
`tools/journal` (the existing flow), which is **not** part of this spec.

## Testing

TDD the pure logic in the autobench crate (existing tests must stay green):

- `parse_contract_metrics` unit-suffixed keying: assert `rtt_p50_ns` is preserved
  (backward-compat), and that `handoff_throughput`/`ops_per_sec` →
  `handoff_throughput_ops_per_sec` and `handoff_rtt_p50`/`ns` → `handoff_rtt_p50_ns`.
- `task_spec` resolution for both new cells: `kind == Local`, correct `direction`,
  `primary_key`, `expected_metrics`, `warmup_env`/`iters_env`/`extra_env`.
- The nine network `task_spec` tests keep passing (new fields are additive).

Process-spawning paths (`run_local_once`, the `Local` correctness/microbench branches)
are validated by a **real harness smoke**, by hand, like the network pilot:
`cargo run -p run-iter -- --task rust-thread-handoff-spin --json` and
`--task rust-thread-handoff-ring --json` each emit a verdict with `status:"pass"` and a
populated `primary`/`metrics`. The crate stays clippy- and rustfmt-clean.

## Files touched

- `autobench/src/task_spec.rs` — new `TaskSpec` fields; fill the 9 network rows; add 2
  Local rows; add resolution tests.
- `autobench/src/sampling.rs` — unit-aware `parse_contract_metrics`; add
  `run_local_once` + `LocalRun`; add keying tests.
- `autobench/src/bin/run-iter.rs` — dispatch `correctness`/`microbench` on `kind`; add
  the `Local` branches using `expected_metrics`/`primary_key`.
- `autobench/tasks/rust-thread-handoff-spin/{program.md,results.tsv}` (new).
- `autobench/tasks/rust-thread-handoff-ring/{program.md,results.tsv}` (new).
- `autobench/CLAUDE.md` — note `Local` is now supported + the two new task ids (the
  "known tasks" string in `run-iter`'s unknown-task message also updated).
