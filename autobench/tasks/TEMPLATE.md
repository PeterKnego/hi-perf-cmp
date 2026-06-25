# Task template

Every optimization task is a benchmark matrix cell
`(focus_area, experiment, language)`. It lives in `autobench/tasks/<task>/` and
is registered by adding one `TaskSpec` row in `autobench/src/task_spec.rs`.

## Files per task

- `autobench/tasks/<task>/program.md` — the task overlay: objective, primary
  and secondary metrics (name + direction), mutable paths, frozen paths, the
  cell `kind`, the noise note, the gates, and the TSV schema. Modeled on
  `autobench/tasks/rust-network-rtt-tcp/program.md`.
- `autobench/tasks/<task>/results.tsv` — the committed run log, tab-separated.
  First column `commit`, last two columns `status` (keep|discard|crash) and
  `description`. Metric columns in between. Numeric values are median-of-N
  (note N in the `description` column).

## Mutable vs frozen (always)

- **Mutable:** only the cell's own source (e.g. `rust/network-rtt/tcp/src/**`).
  That is the code being optimized.
- **Frozen, always:** `rust/bench-common/**` (the shared emitter / measurement
  loop), the result contract (`docs/result-contract.md`), every other cell,
  `autobench/**` itself, and all docs/specs. Never add a dependency to the
  cell's `Cargo.toml`. Never weaken the artifact's built-in correctness checks
  (the Goodhart trap).

## Metrics, direction, noise

- Primary metric drives KEEP/DISCARD; secondaries are recorded and must not
  regress. State each metric's direction (minimize latency, maximize
  throughput).
- Integer ns for latency; float entries/sec for throughput.
- **Network cells are noisy** even over loopback — always use the harness's
  median-of-N (`--samples`, default 5); never decide on a single sample. When a
  delta is within run-to-run noise, re-run `run-iter` for fresh samples.

## Kind

- `Network` — two-process localhost fitness: `run-iter` spawns the artifact as a
  `server` child, waits for it to bind, runs it as a `client` against
  `127.0.0.1`, and parses the client's contract lines. The AWS cross-host run is
  the graduation gate (see `program.md`), not a per-iteration step.
- `Local` — single-host cells (filesystem-write, thread-handoff,
  shared-memory-ipc): the artifact is run once and emits the contract lines
  directly; local fitness is fully meaningful (no cross-host tension).

## Gates

- **build** — the cell's build command.
- **correctness** — the anti-Goodhart floor: a tiny run that must exit 0 and
  emit well-formed contract lines for the cell's `(focus_area, experiment)`.
- **microbench** — the fitness: median-of-N at standard counts → metrics +
  primary.
- **Gate A (tests)** — the cell's test suite, so an optimization can't pass by
  breaking shared code.

## Registering a new task (3 steps)

1. **TaskSpec row** — add to `autobench/src/task_spec.rs`:
   ```rust
   "<id>" => Some(TaskSpec {
       task: "<id>",
       language: "rust",            // | "go" | "java"
       focus_area: "<focus-area>",
       experiment: "<experiment>",
       kind: Kind::Network,         // | Kind::Local
       build: &["cargo", "build", "--release", "-p", "<crate>"],
       build_dir: "rust",
       run: &["cargo", "run", "--release", "-q", "-p", "<crate>"],
       run_dir: "rust",
       gate_a: &["cargo", "test"],
       gate_a_dir: "rust",
       primary_metric: "<metric>",  // contract metric, e.g. "rtt_p50"
       direction: Direction::Minimize, // or Maximize
   }),
   ```
   A Go or Java cell differs only in the `build`/`run`/`gate_a` argv (e.g.
   `go build` / `go test`, or `gradlew :…:installDist` / the launcher /
   `gradlew test`). The harness dispatches on the data — no per-language fork.

2. **Task overlay** — create `autobench/tasks/<id>/program.md` following
   `autobench/tasks/rust-network-rtt-tcp/program.md`: objective, primary /
   secondary metrics + direction, mutable / frozen paths, kind, noise note,
   gates, and the TSV schema block.

3. **TSV header** — create `autobench/tasks/<id>/results.tsv` containing only
   the header row from the overlay's TSV schema block (tab-separated).
