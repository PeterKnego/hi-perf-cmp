# hi-perf-cmp

Comparing high-performance code artifacts across **Rust**, **Java** and **Go**.

This project is an exploration in high-performance code, with emphasis on
comparison between the three languages across three focus areas, each with one or
more **experiments** (variants) compared on a grid of **experiment × language**:

- **network-rtt** — request/response round-trip time. Experiments: `tcp`, `udp`, `quic`.
- **filesystem-write** — filesystem write throughput / latency _(stub)_.
- **thread-handoff** — latency of handing work between threads _(stub)_.

## Layout

The tree is **language-first**: each language is a self-contained, idiomatic
build workspace. Experiments are separate runnable artifacts named
`<focus_area>-<experiment>` (e.g. `network-rtt-tcp`) over a shared per-language
bench library.

```
hi-perf-cmp/
├── rust/          Cargo workspace — bench-common + network-rtt/{tcp,udp,quic} + stubs
├── go/            Go module       — internal/bench + cmd/network-rtt-{tcp,udp,quic} + stubs
├── java/          Gradle build    — :common + :network-rtt-{tcp,udp,quic} + stubs
├── bench-infra/   AWS provisioning rig (Terraform + Ansible) to run on real VPSes
├── tools/journal/ the journal CLI (record / compare / set-baseline)
├── journal/       committed, version-linked record of benchmark runs over time
└── docs/
    ├── result-contract.md         the shared output schema all benchmarks emit
    └── superpowers/specs/         design specs
```

Cross-language/experiment comparison is **not** the directory layout's job: every
benchmark emits results in one shared [result contract](docs/result-contract.md)
(one JSON line per metric), and the [`journal`](journal/README.md) tool collects,
aligns on `(focus_area, experiment, language, metric)`, and tracks them over time.

## Status

`network-rtt` is implemented for `tcp`, `udp`, and `quic` (cross-host capable —
see below). `filesystem-write` and `thread-handoff` are still stubs that emit a
placeholder line.

## Building & running

Each language builds and runs independently — see the per-language READMEs:
[rust/](rust/README.md) · [go/](go/README.md) · [java/](java/README.md).

```sh
# Rust
cd rust && cargo run --release -p network-rtt-tcp

# Go
cd go && go run ./cmd/network-rtt-tcp

# Java
cd java && ./gradlew :network-rtt-tcp:run -q
```

Each prints one JSON line per metric on stdout, e.g.:

```json
{"language":"go","focus_area":"network-rtt","experiment":"tcp","metric":"rtt_p50","value":14380,"unit":"ns","samples":100000}
```

`network-rtt` runs in `loopback` mode by default (local dev). Real **cross-host**
RTT uses `RTT_MODE=server` on one host and `RTT_MODE=client RTT_HOST=<ip>` on
another — orchestrated across a 2-node AWS fleet by [`bench-infra/`](bench-infra/README.md).

## Tracking performance over time

Benchmark runs are recorded in [`journal/`](journal/README.md) and compared with
the `tools/journal` CLI, which flags regressions against a baseline. Results are
committed so every measurement is correlated with the commit that produced it.

## Toolchain versions

Rust 1.96 · Go 1.22 · Java 21 (Gradle 8.10.2 via the checked-in wrapper).
