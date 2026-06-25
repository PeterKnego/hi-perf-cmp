# hi-perf-cmp

Comparing high-performance code artifacts across **Rust**, **Java** and **Go**.

This project is an exploration in high-performance code, with emphasis on
comparison between the three languages across three focus areas:

- **network-rtt** — request/response round-trip time
- **filesystem-write** — filesystem write throughput / latency
- **thread-handoff** — latency of handing work between threads

## Layout

The tree is **language-first**: each language is a self-contained, idiomatic
build workspace with one runnable benchmark per focus area.

```
hi-perf-cmp/
├── rust/        Cargo workspace      — crate per focus area     (cargo build)
├── go/          Go module            — cmd/ binary per focus area (go build ./...)
├── java/        Gradle build         — subproject per focus area (./gradlew build)
├── harness/     cross-language perf harness                     (placeholder — see README)
├── results/     benchmark output                                (gitignored)
└── docs/
    ├── result-contract.md            the shared output schema all benchmarks emit
    └── superpowers/specs/            design specs
```

Cross-language side-by-side comparison is the **harness's** job, not the
directory layout's: every benchmark emits results in one shared
[result contract](docs/result-contract.md), which the harness collects and
aligns. The harness is not yet implemented.

## Status

Skeleton stage. Every benchmark is a **stub** that builds, runs, and emits a
placeholder result line in the shared contract format. Real measurement logic
is added per focus area in later work.

## Building & running

Each language builds and runs independently — see the per-language READMEs:
[rust/](rust/README.md) · [go/](go/README.md) · [java/](java/README.md).

```sh
# Rust
cd rust && cargo run --release -p network-rtt

# Go
cd go && go run ./cmd/network-rtt

# Java
cd java && ./gradlew :network-rtt:run -q
```

Each prints one JSON line on stdout, e.g.:

```json
{"language":"go","focus_area":"network-rtt","metric":"placeholder","value":0,"unit":"ns","samples":0,"notes":"stub"}
```

## Toolchain versions

Rust 1.96 · Go 1.22 · Java 21 (Gradle 8.10.2 via the checked-in wrapper).
