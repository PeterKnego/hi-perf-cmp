# Project Structure Design — hi-perf-cmp

**Date:** 2026-06-25
**Status:** Approved (layout + tooling depth confirmed)

## Purpose

Establish the repository structure for exploring and comparing high-performance
code artifacts across **Rust**, **Java**, and **Go**. Three focus areas are
explored in each language:

- **network-rtt** — round-trip time of network request/response
- **filesystem-write** — filesystem write throughput / latency
- **thread-handoff** — latency of handing work off between threads

A cross-language **perf-testing harness** will be added later. The structure
must anticipate it: every benchmark is an independently runnable artifact that
emits results in one shared format the harness can collect and compare.

## Design Decisions

### 1. Language-first layout

Top-level directories are the three languages; each owns an idiomatic,
self-contained build workspace with one buildable unit per focus area.

**Why language-first over focus-area-first:** each language has a strong build
convention (Cargo workspace, single Go module, single Gradle build) that wants
to own one directory tree. Focus-area-first would fragment each toolchain across
three top-level dirs and fight every build tool. The side-by-side comparison
that focus-area-first offers is the *harness's* job, not the directory tree's —
the harness invokes each artifact and aligns results by the shared contract.

### 2. Buildable skeleton stubs (not full implementations)

Each focus area ships with working build config plus a minimal runnable stub
(`main` that prints one placeholder result line in the shared format). The whole
tree compiles/runs from day one. Real perf code is designed and added per area
in later work — we do not guess implementations now.

### 3. Full per-language workspaces

Each language builds with a single command:
- Rust: `cargo build` (Cargo workspace, members = focus areas)
- Go: `go build ./...` (single module, package per focus area)
- Java: `./gradlew build` (single Gradle build, subproject per focus area)

### 4. Shared result contract

A small documented schema (`docs/result-contract.md`) that every stub emits as a
single JSON line on stdout. Gives the future harness a stable thing to parse.

Fields:
| field        | type   | meaning                                          |
|--------------|--------|--------------------------------------------------|
| `language`   | string | `rust` \| `java` \| `go`                         |
| `focus_area` | string | `network-rtt` \| `filesystem-write` \| `thread-handoff` |
| `metric`     | string | what was measured, e.g. `rtt_p50`                |
| `value`      | number | measured value                                   |
| `unit`       | string | e.g. `ns`, `us`, `ms`, `ops_per_sec`             |
| `samples`    | number | number of samples behind the value              |
| `notes`      | string | free-form (optional)                             |

Example stub output:
```json
{"language":"rust","focus_area":"network-rtt","metric":"placeholder","value":0,"unit":"ns","samples":0,"notes":"stub"}
```

## Directory Structure

```
hi-perf-cmp/
├── README.md
├── LICENSE
├── docs/
│   ├── result-contract.md           # the shared output schema
│   └── superpowers/specs/           # design specs (this file)
├── rust/                            # Cargo workspace
│   ├── Cargo.toml                   # [workspace] members = the three crates
│   ├── network-rtt/        { Cargo.toml, src/main.rs }
│   ├── filesystem-write/   { Cargo.toml, src/main.rs }
│   └── thread-handoff/     { Cargo.toml, src/main.rs }
├── go/                              # single Go module
│   ├── go.mod
│   ├── internal/result/result.go    # shared result-contract emitter
│   └── cmd/{network-rtt,filesystem-write,thread-handoff}/main.go
├── java/                            # single Gradle build
│   ├── settings.gradle.kts          # includes the three subprojects
│   ├── build.gradle.kts             # shared config (Java 21)
│   ├── gradlew / gradlew.bat / gradle/wrapper/   # wrapper
│   └── {network-rtt,filesystem-write,thread-handoff}/
│         ├── build.gradle.kts
│         └── src/main/java/.../Main.java
├── harness/                         # placeholder for future orchestrator
│   └── README.md
└── results/                         # gitignored — benchmark output lands here
    └── .gitkeep
```

## Toolchain Reality (2026-06-25 environment)

| tool   | status            | implication                                            |
|--------|-------------------|--------------------------------------------------------|
| Rust   | 1.96 installed    | `cargo build` verifiable in this session               |
| Java   | JDK 21 installed  | source compiles; Gradle itself not installed           |
| Gradle | NOT installed     | ship Gradle **wrapper**; fetch wrapper jar if network allows, else document the one-step bootstrap |
| Go     | NOT installed     | scaffold `go.mod` + sources correctly; build not verifiable here — document it |

Verification scope for this task: Rust workspace builds clean. Java sources are
valid and Gradle config is correct (wrapper-bootstrapped). Go module + sources
are syntactically correct and conventionally laid out; build to be confirmed
once Go is available.

## Out of Scope (YAGNI)

- The harness implementation itself (placeholder dir + README only).
- Any real benchmark logic — stubs only.
- CI configuration.
