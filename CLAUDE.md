# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A comparison of high-performance code artifacts across **Rust**, **Java**, and **Go**, over three focus areas:
**network-rtt** (request/response round-trip time), **filesystem-write** (write throughput/latency), and
**thread-handoff** (latency of passing work between threads).

The repo is at the **skeleton stage**: every benchmark is a stub that builds, runs, and emits a placeholder
result line. Real measurement logic is added per focus area later.

## Architecture: the result contract is the only coupling

The design is deliberately decoupled. Each benchmark is an independently runnable executable that prints
**one JSON object per line to stdout** in a shared schema (see `docs/result-contract.md`). That contract is
the *only* thing tying benchmarks to the future comparison harness:

- Benchmarks stay plain executables with no harness dependency.
- The harness (placeholder in `harness/`, not yet implemented) stays a plain stdout-line reader with no
  per-language build knowledge — it runs an artifact, parses the lines, aligns by `focus_area` + `metric`.

**Consequence for any change:** stdout is for result lines only — send logs/progress/diagnostics to stderr.
A benchmark reporting multiple metrics prints one line per metric. Each language has one canonical emitter;
reuse it rather than hand-rolling JSON:
- Rust — hand-rendered `println!` in each `src/main.rs` (zero deps).
- Go — `internal/result` package (`result.Emit`).
- Java — `net.knego.hiperf.common.Result#emit` in the `:common` subproject.

## Layout is language-first

Top-level dirs are the languages, each a self-contained idiomatic workspace with one runnable unit per focus
area. This keeps each toolchain (Cargo workspace / single Go module / single Gradle build) intact — do not
fragment a language's build across focus-area dirs. Cross-language side-by-side comparison is the harness's
job, not the directory layout's.

## Build & run

```sh
# Rust — Cargo workspace, crate per focus area
cd rust && cargo build --release
cargo run --release -p network-rtt          # | filesystem-write | thread-handoff

# Go — single module, cmd/ binary per area
cd go && go build ./... && go vet ./...
go run ./cmd/network-rtt                     # | filesystem-write | thread-handoff

# Java — single Gradle build, app subproject per area + :common, JDK 21 toolchain
cd java && ./gradlew build
./gradlew :network-rtt:run -q                # | :filesystem-write:run | :thread-handoff:run
```

The Gradle **wrapper is checked in** (`java/gradlew`, `java/gradle/wrapper/`); always invoke Gradle via
`./gradlew`, not a system `gradle`. There are no tests yet — when adding them, use each language's standard
runner (`cargo test`, `go test ./...`, `./gradlew test`).

## Toolchain versions

Rust 1.96 · Go 1.22 · Java 21 · Gradle 8.10.2 (via wrapper).

## Remote benchmarking (`bench-infra/`)

`bench-infra/` provisions a 2-node AWS fleet (Terraform) and runs the benchmarks on it (Ansible),
pulling result-contract lines to `bench-out/dist/<ts>/results.jsonl`. node0 = client/driver +
single-host benchmarks; node1 = the `network-rtt` cross-host responder. Instances are NVMe-bearing
`c6id` (local NVMe mounted at the bench home for `filesystem-write`). Workflow: `make init` → `make up`
→ `make bench` → `make destroy` (creds in a gitignored `.env`). Real runs cost money and are
**user-initiated** — never `terraform apply` automatically. See `bench-infra/README.md` and the spec.

`network-rtt` has an `RTT_MODE` env contract — `loopback` (default, local dev), `server`, `client` —
plus `RTT_HOST`/`RTT_TCP_PORT`/`RTT_UDP_PORT`. Real network RTT is **cross-host only**; loopback is a
local-dev convenience, never a reported result.

## Adding a real benchmark

Replace a stub's placeholder emit (`metric: "placeholder"`, `notes: "stub"`) with real measurement that emits
the same contract. Keep the focus-area names exact (`network-rtt`, `filesystem-write`, `thread-handoff`) and
`language` matching the directory — the harness aligns results on these strings. For the Rust release profile
and workspace conventions, see below.

## Rust workspace conventions

Mirrors the sibling `../ultima_cluster` project: `rust/rust-toolchain.toml` pins
stable + `rustfmt`/`clippy`; the workspace uses **edition 2024** and a
`[workspace.package]` block that member crates inherit via `field.workspace =
true`; shared deps go in `[workspace.dependencies]` (empty for now — benchmarks
are std-only). Release profile: `lto = "thin"`, `codegen-units = 1`, `debug = 1`.
Keep the workspace **clippy- and rustfmt-clean** (`cargo clippy --all-targets`,
`cargo fmt --check`).

Design rationale lives in `docs/superpowers/specs/`.
