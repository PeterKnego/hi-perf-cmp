# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A comparison of high-performance code artifacts across **Rust**, **Java**, and **Go**. The focus areas are the
performance-critical paths of **state-machine-replication (SMR)** systems (Raft/Paxos-style replicated logs);
the goal is to choose and optimize the code for each path. Each focus area has one or more **experiments**
(variants) compared on a grid of **experiment × language**:

- **network-rtt** — minimize RTT for leader→follower→leader communication when replicating log entries.
- **filesystem-write** — fast, durable command-log persistence.
- **thread-handoff** — thread-to-thread data passing, including thread sleep/wakeup.
- **serialization** — encode/decode cost (latency + memory) of a command-log record; SBE vs bincode.
- **smr-collections** — insert/update/snapshot cost of the in-memory state store (a fixed-capacity
  limit-order-book) that SMR replays commands into.
- **rpc-roundtrip** — mutating serialize→send→deserialize+mutate→reserialize→send→deserialize round-trip
  across transport+codec stacks.
- **shared-memory-ipc** — shared-memory inter-process communication _(planned focus area)_.

**Status:** `network-rtt` is implemented for the `tcp`, `udp`, and `quic` experiments (cross-host capable).
`filesystem-write` is implemented for the `fsync`, `fdatasync`, `prealloc`, and `batch` experiments
(single-host, local NVMe). `thread-handoff` is implemented for the `spin`, `condvar`, `channel`, and
`ring` experiments (single-host). `serialization` is implemented in Rust (`sbe_gen` zerocopy SBE,
`aeron_sbe` real-logic SBE-tool Rust output, `bincode` serde+bincode) and Go (`bebop` via the
200sc/bebop safe API, `protobuf` via the canonical google.golang.org/protobuf runtime), single-host,
measuring encode/decode latency + decode allocation. Go now also has SBE: a zero-copy flyweight cell
reusing experiment `aeron_sbe` (the Go twin of the Rust `aeron_sbe` flyweight, byte-identical wire, 0
decode-alloc) and an owned-decode `sbe_struct` cell (same wire, materializes an owned struct); both are
generated from the shared `journal.xml` by the vendored real-logic sbe-tool
(`-Dsbe.go.generate.generate.flyweights` toggles the two modes). Go also has a `flatbuffers` cell using
Google FlatBuffers' zero-copy read path (default accessors, not the object API): 0 decode allocation,
with a larger wire than SBE (~608 B vs SBE's 502 B). Java is not planned for this focus area.
`smr-collections` is implemented for the `insert`, `update`, and `snapshot` experiments
across all three languages (single-host, fixed-capacity limit-order-book state store): Java uses
Agrona (`Long2ObjectHashMap` + pooled orders), Rust/Go use a hand-rolled open-addressing id-map;
the snapshot format is SBE (`book_snapshot.xml`), byte-identical across languages and verified by a
golden test. `rpc-roundtrip` is implemented for `sbe_udp` (Rust, UDP + zero-copy SBE), `grpc` (Go, gRPC),
and `bebop_tcp` (Go, TCP + bebop), cross-host, measuring full mutating round-trip latency + encoded size;
Java is not planned for this focus area. `shared-memory-ipc` is not yet scaffolded.

## Architecture: the result contract is the only coupling

The design is deliberately decoupled. Each benchmark is an independently runnable executable that prints
**one JSON object per line to stdout** in a shared schema (see `docs/result-contract.md`). That contract is
the *only* thing tying benchmarks to the downstream tooling:

- Benchmarks stay plain executables with no tooling dependency.
- The `tools/journal` CLI is a plain stdout-line reader with no per-language build knowledge — it parses the
  lines and aligns on the cell key `(focus_area, experiment, language, metric)`.

**Consequence for any change:** stdout is for result lines only — send logs/progress/diagnostics to stderr.
A benchmark reporting multiple metrics prints one line per metric. Each language has one shared bench library
that owns Stats, env-config parsing, the timed loop, and result emission (including `experiment`); reuse it
rather than hand-rolling JSON:
- Rust — `bench-common` crate (`result::emit`).
- Go — `internal/bench` package (`bench.Emit`).
- Java — `net.knego.hiperf.common` (`Result#emit`) in the `:common` subproject.

## Layout is language-first

Top-level dirs are the languages, each a self-contained idiomatic workspace. Each experiment is its own
runnable artifact named `<focus_area>-<experiment>` (e.g. `network-rtt-tcp`) built over a shared per-language
bench library; a stub focus area would have a single artifact named just `<focus_area>` (none at present). This keeps each toolchain
(Cargo workspace / single Go module / single Gradle build) intact — do not fragment a language's build across
dirs. Cross-language/experiment comparison is the `tools/journal` CLI's job, not the directory layout's.

## Build & run

Artifact names: `network-rtt-{tcp,udp,quic}`, `filesystem-write-{fsync,fdatasync,prealloc,batch}`, `thread-handoff-{spin,condvar,channel,ring}`, `serialization-{sbe_gen,aeron_sbe,bincode}` (Rust; `aeron_sbe` also Go) and `serialization-{aeron_sbe,sbe_struct,bebop,protobuf,flatbuffers}` (Go), `smr-collections-{insert,update,snapshot}`, `rpc-roundtrip-{sbe_udp}` (Rust) and `rpc-roundtrip-{grpc,bebop_tcp}` (Go).

```sh
# Rust — Cargo workspace: bench-common + network-rtt + filesystem-write + thread-handoff experiments
cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check
cargo run --release -p network-rtt-tcp        # -p network-rtt-udp | -p filesystem-write-fsync | -p filesystem-write-batch | ...
cargo run --release -p serialization-bincode  # -p serialization-sbe_gen | -p serialization-aeron_sbe
cargo run --release -p smr-collections-insert # -p smr-collections-update | -p smr-collections-snapshot
cargo run --release -p rpc-roundtrip-sbe_udp

# Go — single module: internal/bench + cmd/network-rtt-* + filesystem-write-* + thread-handoff-* + serialization-*
cd go && go build ./... && go vet ./... && go test ./...
go run ./cmd/network-rtt-tcp
go run ./cmd/serialization-protobuf  # or ./cmd/serialization-bebop
go run ./cmd/serialization-aeron_sbe # or ./cmd/serialization-sbe_struct
go run ./cmd/serialization-flatbuffers
go run ./cmd/smr-collections-insert
go run ./cmd/rpc-roundtrip-grpc      # or ./cmd/rpc-roundtrip-bebop_tcp

# Java — single Gradle build: :common + :network-rtt-* + :filesystem-write-* + :thread-handoff-*, JDK 21 toolchain
cd java && ./gradlew build        # runs tests too (StatsTest under :common)
./gradlew :network-rtt-tcp:run -q
./gradlew :smr-collections-insert:run -q

# Journal CLI (separate Rust crate; not in the rust/ workspace)
cd tools/journal && cargo build --release && cargo test
```

The Gradle **wrapper is checked in** (`java/gradlew`, `java/gradle/wrapper/`); always invoke Gradle via
`./gradlew`, not a system `gradle`. Tests exist for the comparability-critical `Stats` (each language) and the
journal CLI; keep them green. Java emits `value` as a double (e.g. `34151.0`) and always includes `notes` — both
are valid contract JSON.

## Toolchain versions

Rust 1.96 · Go 1.22 · Java 21 · Gradle 8.10.2 (via wrapper).

## Remote benchmarking (`bench-infra/`)

`bench-infra/` provisions a 2-node AWS fleet (Terraform) and runs the benchmarks on it (Ansible),
pulling result-contract lines to `bench-out/dist/<ts>/results.jsonl`. node0 = client/driver +
single-host benchmarks; node1 = the `network-rtt` and `rpc-roundtrip` cross-host responder. Instances are
NVMe-bearing `c6id` (local NVMe mounted at the bench home for `filesystem-write`). Workflow: `make init` →
`make up` → `make bench` → `make destroy` (creds in a gitignored `.env`). Real runs cost money and are
**user-initiated** — never `terraform apply` automatically. See `bench-infra/README.md` and the spec.

`network-rtt` has an `RTT_MODE` env contract — `loopback` (default, local dev), `server`, `client` —
plus `RTT_HOST`/`RTT_TCP_PORT`/`RTT_UDP_PORT`. `rpc-roundtrip` mirrors this with an `RPC_MODE` env
contract plus `RPC_HOST`/`RPC_UDP_PORT`/`RPC_TCP_PORT`/`RPC_GRPC_PORT`. Real network RTT/RPC is
**cross-host only**; loopback is a local-dev convenience, never a reported result.

## Tracking performance over time

Runs are recorded in `journal/` (committed) and compared with the `tools/journal` CLI: `record` ingests a
`bench-out/dist/<ts>/` run into `journal/runs/<ts>-<sha>/`, `compare` prints per-cell deltas and flags
regressions (direction-aware by unit; default 10%), `set-baseline` sets the reference. Confirmed regressions
go in `journal/REGRESSIONS.md`. Each run is committed, so results are correlated with the producing commit.

**Only journal a real benchmark run.** `record` ingests the `results.jsonl` an AWS `bench-infra` run
produced — there is nothing to record without one, so journaling is the tail of a real run, not a separate
step. Local-dev/loopback smoke runs are fitness checks only and are **never** journaled. The first journal
entries therefore come from genuine AWS runs — cross-host for `network-rtt`, single-host on local NVMe for
`filesystem-write` — not from loopback or a dev box.

## Adding an experiment or a real benchmark

To add an experiment to a focus area (e.g. a new transport for `network-rtt`): add an artifact
`<focus_area>-<experiment>` in each language over the shared bench library (Rust crate under
`rust/<focus_area>/<exp>/`; Go `go/cmd/<focus_area>-<exp>/`; Java `:<focus_area>-<exp>` subproject), emit
`experiment: "<exp>"`, and add a row to `bench-infra/ansible/group_vars/all.yml`'s `experiments` matrix.
Keep experiment-specific dependencies in that artifact only (e.g. QUIC's quinn/quic-go/Kwik).

To turn a stub focus area real: replace its placeholder emit (`experiment: "placeholder"`, `metric:
"placeholder"`, `notes: "stub"`) with real measurement. Keep focus-area names exact (`network-rtt`,
`filesystem-write`, `thread-handoff`, `serialization`, `smr-collections`, `rpc-roundtrip`, and the planned
`shared-memory-ipc`), `language` matching the directory,
and always emit the `experiment` field — the `tools/journal` CLI aligns on `(focus_area, experiment, language,
metric)`. For the Rust release profile and workspace conventions, see below.

## Rust workspace conventions

Mirrors the sibling `../ultima_cluster` project: `rust/rust-toolchain.toml` pins
stable + `rustfmt`/`clippy`; the workspace uses **edition 2024** and a
`[workspace.package]` block that member crates inherit via `field.workspace =
true`; shared deps go in `[workspace.dependencies]` (the tcp/udp experiments and
stubs are std-only; the quic experiment is the only consumer of the
quinn/rustls/rcgen/tokio stack declared there). Release profile: `lto = "thin"`,
`codegen-units = 1`, `debug = 1`. Keep the workspace **clippy- and rustfmt-clean**
(`cargo clippy --all-targets`, `cargo fmt --check`).

Design rationale lives in `docs/superpowers/specs/`.
