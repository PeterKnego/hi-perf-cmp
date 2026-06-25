# Experiment Dimension + QUIC — Design

**Date:** 2026-06-25
**Status:** Proposed — awaiting review
**Supersedes:** metric naming in `2026-06-25-network-rtt-design.md` (the `tcp_`/`udp_`
prefix on metrics is replaced by a first-class `experiment` field).

## Purpose

Introduce **experiment** as a first-class benchmark dimension nested under each
focus area, making the comparison grid **experiments × languages**. For
`network-rtt` the experiments are **tcp**, **udp**, **quic**. Each experiment is
its own runnable artifact built over a shared per-language bench library. QUIC is
implemented now alongside the restructured TCP/UDP.

```
focus area        experiments              languages
network-rtt   →   { tcp, udp, quic }   ×   { rust, go, java }
filesystem-write  (experiments later)  ×   { rust, go, java }
thread-handoff    (experiments later)  ×   { rust, go, java }
```

## 1. Result contract change

Add an `experiment` field; move the transport out of the metric name.

| field        | example         | change                         |
|--------------|-----------------|--------------------------------|
| `language`   | `rust`          | unchanged                      |
| `focus_area` | `network-rtt`   | unchanged                      |
| `experiment` | `tcp`           | **NEW** — the variant under the focus area |
| `metric`     | `rtt_p50`       | simplified (was `tcp_rtt_p50`) |
| `value` / `unit` / `samples` / `notes` | … | unchanged    |

`docs/result-contract.md` is updated: `experiment` is required; for the
still-stubbed `filesystem-write` / `thread-handoff` the stub emits
`experiment: "placeholder"`. Each `network-rtt` experiment artifact emits three
lines: `rtt_p50`, `rtt_p99`, `rtt_mean` (unit `ns`).

No consumers parse results yet (the harness is a placeholder; bench-infra only
concatenates lines), so the field addition is non-breaking.

## 2. Artifact-per-experiment + shared library (per language)

A shared **bench-common** library per language holds the comparability-critical
and boilerplate code so every experiment artifact is thin and identical in
methodology:
- **Stats** — `percentile` (nearest-rank `sorted[floor(p/100·(n-1))]`) + `mean`.
- **Config** — parse the `RTT_*` env contract (`RTT_MODE`, `RTT_HOST`,
  `RTT_TCP_PORT`/`RTT_UDP_PORT`/`RTT_QUIC_PORT`, `RTT_PAYLOAD_BYTES`,
  `RTT_WARMUP`, `RTT_ITERATIONS`).
- **Result** — emit a contract line including `experiment`.
- **Measure** — the warmup + timed ping-pong loop driving a per-experiment
  "one round trip" callback into a pre-allocated sample buffer (keeps allocation
  out of the timed path uniformly).

Each experiment artifact provides only: a transport **server/responder**, a
**client connect**, and a **round-trip** operation; it calls into bench-common
for everything else. The `loopback`/`server`/`client` `RTT_MODE` semantics carry
over per experiment.

### Naming convention (consistent across languages)

Artifact/binary name = `<focus_area>-<experiment>`, e.g. `network-rtt-tcp`,
`network-rtt-udp`, `network-rtt-quic`. This lets the bench-infra `run_bench.sh`
resolve binaries uniformly.

### Directory layout

**Rust** (`rust/`): new workspace members.
```
rust/
  Cargo.toml                 # members += bench-common, network-rtt/{tcp,udp,quic}; remove old network-rtt
  bench-common/              # lib crate: stats, config, result, measure (std-only)
  network-rtt/
    tcp/   (pkg network-rtt-tcp,  std-only)        # serve/client moved from old tcp.rs
    udp/   (pkg network-rtt-udp,  std-only)        # moved from old udp.rs
    quic/  (pkg network-rtt-quic, quinn deps)
  filesystem-write/  thread-handoff/   # unchanged stubs; emit experiment="placeholder"
```
The old single `rust/network-rtt` crate is removed; its tcp/udp/stats/config
logic is split into bench-common + the tcp/udp experiment crates. QUIC deps live
**only** in the quic crate (workspace.dependencies entries: quinn, rustls, rcgen).

**Go** (`go/`): one module; per-binary linking already isolates deps (the
tcp/udp binaries never import quic-go, so they don't link it).
```
go/
  internal/bench/            # shared: stats, config, result, measure
  cmd/network-rtt-tcp/   network-rtt-udp/   network-rtt-quic/   (quic imports github.com/quic-go/quic-go)
  cmd/filesystem-write/  thread-handoff/    # unchanged stubs
```
`go build -o bin/ ./cmd/...` produces `go/bin/network-rtt-{tcp,udp,quic}` etc.

**Java** (`java/`): `:common` gains Stats + Config + Measure (moved from
`:network-rtt`); the old `:network-rtt` subproject is replaced by three
application subprojects.
```
java/
  settings.gradle.kts        # include common, network-rtt-tcp/udp/quic, filesystem-write, thread-handoff
  common/                    # Result(+experiment), Stats, Config, Measure
  network-rtt-tcp/  network-rtt-udp/  network-rtt-quic/   (quic depends on a QUIC lib)
  filesystem-write/  thread-handoff/  # unchanged stubs
```

## 3. QUIC experiment

QUIC needs TLS; for a loopback/private-network benchmark the server generates an
**in-memory self-signed certificate** at startup and the client **skips
verification** (insecure is acceptable — we measure latency, not security). A
fixed **ALPN** (`hperf-rtt`) is used on both ends.

**Methodology (mirrors TCP for comparability):** one connection, one long-lived
**bidirectional stream**, strict ping-pong (write `payload_bytes`, read the full
echo back), one outstanding request at a time, warmup discarded, then
`RTT_ITERATIONS` timed round trips. Server echoes stream bytes back. Same
`RTT_MODE` server/client/loopback split. Datagram-mode QUIC is out of scope (a
possible future `quic-dgram` experiment).

**Libraries:**
- Rust — `quinn` (+ `rustls` with the `ring` backend, `rcgen` for the cert).
- Go — `github.com/quic-go/quic-go` (+ stdlib `crypto/tls`, self-signed cert).
- Java — a JVM QUIC library. **Prefer a pure-Java implementation (Kwik —
  `tech.kwik:kwik`) to avoid native-library deps on the bench hosts**; fall back
  to Netty's incubator QUIC codec only if Kwik can't cover both the echo server
  and client. The chosen lib is pinned in `network-rtt-quic/build.gradle.kts`.
  (Java QUIC is the highest-risk piece; if neither library cleanly supports a
  raw bidi-stream echo server, flag it before sinking time — do not invent a
  half-working transport.)

The QUIC port is `RTT_QUIC_PORT` (default `9102`), distinct from TCP/UDP so a
single `server`-mode process per experiment is unambiguous.

## 4. bench-infra matrix update

`group_vars/all.yml` gains an experiment-aware matrix. Replace the
`focus_areas` list with experiment rows:
```yaml
experiments:
  - { focus_area: network-rtt,      experiment: tcp,         kind: cross_host }
  - { focus_area: network-rtt,      experiment: udp,         kind: cross_host }
  - { focus_area: network-rtt,      experiment: quic,        kind: cross_host }
  - { focus_area: filesystem-write, experiment: placeholder, kind: local }
  - { focus_area: thread-handoff,   experiment: placeholder, kind: local }
rtt_quic_port: 9102   # add alongside rtt_tcp_port / rtt_udp_port
```
- `run_bench.sh` signature becomes `<language> <focus_area> <experiment> <mode>`;
  it resolves the binary by the `<focus_area>-<experiment>` convention
  (rust `rust/target/release/<fa>-<exp>`, go `go/bin/<fa>-<exp>`, java
  `./gradlew :<fa>-<exp>:run -q`).
- The `run` role loops `languages × (experiments where kind==cross_host)` for the
  node1-server / node0-client orchestration, and `languages × (experiments where
  kind==local)` on node0. All result lines still append to one `results.jsonl`.
- `local` focus areas with `experiment: placeholder` just run the stub binary
  (single experiment per area until real ones exist).
- Build role unchanged in shape (workspace/module/gradle builds now also compile
  the QUIC artifacts → quinn/quic-go/Kwik fetched at build time; network is
  available on the hosts).

## 5. Testing / verification

- **bench-common Stats** unit-tested in each language (unchanged formula).
- **tcp/udp** experiments: tests green; loopback emits 3 lines each; two-process
  127.0.0.1 server↔client run yields sane numbers (`p99 ≥ p50`), per language.
- **quic** experiment: loopback emits 3 lines; two-process 127.0.0.1 run works,
  per language. QUIC values will be higher than TCP (handshake amortized over the
  connection, but stream/crypto overhead per round trip) — sanity, not a target.
- All emitted lines carry the new `experiment` field; `filesystem-write` /
  `thread-handoff` emit `experiment: "placeholder"`.
- bench-infra: `terraform validate` still clean (unchanged); `run_bench.sh`
  shellcheck-clean with the new arg; `ansible --syntax-check` passes with the new
  matrix.
- Workspace stays clippy/fmt-clean (Rust); `go vet` clean; Java builds clean.

## 6. Implementation phases (one spec, staged execution)

1. **Contract + shared libs + tcp/udp restructure** — add `experiment`; extract
   bench-common (Stats/Config/Result/Measure) per language; port TCP and UDP into
   experiment artifacts; update `docs/result-contract.md`. Verify locally.
2. **QUIC experiment** — implement `network-rtt-quic` in all three languages.
   Verify locally (loopback + two-process).
3. **bench-infra matrix** — experiment-aware `group_vars`, `run_bench.sh`, and
   `run` role; re-verify terraform/shellcheck/ansible-syntax.

## Out of scope (YAGNI)

- Experiments for `filesystem-write` / `thread-handoff` (defined when those areas
  are implemented).
- QUIC datagram mode; HTTP/3 framing; real TLS verification.
- Result aggregation across the matrix — the `harness/` placeholder's future job
  (it will pivot `results.jsonl` on `focus_area × experiment × language`).
