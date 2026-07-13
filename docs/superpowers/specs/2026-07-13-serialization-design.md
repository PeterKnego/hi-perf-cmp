# serialization — SBE vs bincode Codec Comparison — Design

**Date:** 2026-07-13
**Status:** Proposed — awaiting review

## Purpose

A new focus area, **`serialization`**, that measures the cost of encoding and
decoding one realistic ~500-byte state-machine-replication **journal record**
across three Rust codecs driven from a shared message schema. It is the codec
analog of `filesystem-write`: the same command-log record, but measuring the
**serialization** rather than the disk.

The comparison answers two questions for the SMR hot path — a command journal
that encodes on append and decodes on replay:

1. **Performance:** encode/decode latency (ns/op) per codec.
2. **Memory:** heap allocated to decode-and-materialize a record — the axis where
   zero-copy SBE and eager owned bincode diverge most, and the one that dominates
   a replay of millions of journal records.

Scope: **Rust only.** SBE is a cross-language wire format, but the two other cells
(`bincode`) are Rust-native and there is no Go/Java analog worth pairing here. A
cross-language SBE cell can be added later; it is out of scope for this spec.

## Experiments (three Rust codecs)

The comparison grid is `experiment × language` within the focus area; here
`language` is fixed to `rust` and the three experiments are the codecs:

| experiment  | generator                                             | runtime dep         | style                              |
|-------------|-------------------------------------------------------|---------------------|------------------------------------|
| `sbe_gen`   | [`sbe_gen`](https://docs.rs/sbe_gen) crate, at build time | `zerocopy`          | zero-copy views + encode builders  |
| `aeron_sbe` | real-logic `sbe-all.jar` `RustGenerator`, at build time | none (self-contained)| reference SBE flyweight codecs     |
| `bincode`   | `serde` derive + `bincode` v2 (`config::standard()`)  | `serde`, `bincode`  | eager owned encode/decode          |

- **`sbe_gen`** is a pure-Rust SBE compiler that emits `zerocopy`-derived structs
  (`FromBytes`/`IntoBytes`/`KnownLayout`/`Immutable`/`Unaligned`) with `parse_prefix`
  zero-copy decode and per-message builders. Full support for repeating groups and
  variable-length `data` fields (confirmed against 0.7.3 source).
- **`aeron_sbe`** is the **reference** real-logic SBE toolchain — the exact codegen
  Aeron itself uses — emitting Rust via `sbe.target.language=Rust`. `aeron_sbe` is
  **not a crate**; it is the Java tool that generates self-contained Rust ser/deser
  code (no runtime crate dependency). The `sbe-all-1.38.1.jar` is already present in
  the local Gradle cache and carries a full `generation/rust/RustGenerator`; JDK 21
  is available to run it.
- **`bincode`** is the ergonomic baseline: derive `Serialize`/`Deserialize` on a
  mirror struct, encode/decode with `bincode::serde` + `config::standard()` — the
  same choice the sibling `../ultima_cluster` uses on its consensus wire path.

### The fairness anchor: byte-identical SBE output

`sbe_gen` and `aeron_sbe` consume the **same `schema.xml`**. Because SBE is a
deterministic wire spec (message header → fixed root block → repeating groups →
length-prefixed var-data, little-endian), both generators MUST produce
**byte-for-byte identical** encoded output for the same record. A test asserts
this equality (golden bytes). This is the correctness guarantee that makes the
three-way comparison honest: the two SBE cells differ only in generated-code
quality, not in what they put on the wire.

## The payload — `JournalRecord` (~500 bytes)

One realistic SMR journal-append record: a batch of replicated commands with a
mixed-type header. Expressed **once** as an SBE XML schema (consumed by both SBE
cells) and **mirrored** as a `#[derive(Serialize, Deserialize)]` Rust struct (for
the bincode cell). The two representations describe the same logical record; a
test asserts round-trip field equality across all three codecs.

Modeled on `aeron/aeron-cluster/.../aeron-cluster-codecs.xml` conventions.

### Layout

Fixed root block (mixed field types) — 50 bytes:

| field              | type          | bytes |
|--------------------|---------------|-------|
| `leadershipTermId` | int64         | 8     |
| `logPosition`      | int64         | 8     |
| `timestamp`        | int64 (time_t)| 8     |
| `clusterSessionId` | int64         | 8     |
| `correlationId`    | int64         | 8     |
| `leaderMemberId`   | int32         | 4     |
| `serviceId`        | int32         | 4     |
| `eventType`        | enum uint8    | 1     |
| `flags`            | bitset uint8  | 1     |

Repeating group `entries` (default 4 entries), each:

| field            | type      | bytes            |
|------------------|-----------|------------------|
| `entryTermId`    | int64     | 8                |
| `entryIndex`     | int64     | 8                |
| `entryTimestamp` | int64     | 8                |
| `commandKey`     | int32     | 4                |
| `command`        | varData   | 4 (len) + N      |

### Hitting ~500 bytes

```
8  (SBE message header)
50 (fixed root block)
4  (group dimension header: blockLength + numInGroup)
4 × (28 entry block + 4 varData len + 78 command bytes)  = 440
---
≈ 502 bytes total
```

### Repeatable and deterministic

- The record is built by a **shared builder** seeded from the record index —
  **no RNG** (matches the project's no-`Math.random`/`Date.now` discipline), so a
  run is byte-reproducible.
- Size is a **knob, not a magic constant**: `SER_ENTRIES` (default 4) and
  `SER_CMD_BYTES` (default 78) are env config; defaults land at ~500 bytes.
  "Various fields" and "repeatable" (repeating group) are both exercised.

## Metrics

Each codec (experiment) emits four result-contract lines (see
`docs/result-contract.md`), aligned on `(focus_area, experiment, metric)`:

| metric               | unit    | emitter      | what it captures                                                                 |
|----------------------|---------|--------------|----------------------------------------------------------------------------------|
| `encode_ns`          | `ns`    | `emit_float` | serialize one record into a **reused, preallocated** buffer                      |
| `decode_ns`          | `ns`    | `emit_float` | decode + **fully materialize all fields** (see below)                            |
| `encoded_bytes`      | `bytes` | `emit`       | on-wire size of one encoded record (journal footprint per record)                |
| `decode_alloc_bytes` | `bytes` | `emit`       | heap **bytes allocated** to decode + materialize one record (counting allocator) |

### Decode fairness: full materialization

SBE decode is lazy (zero-copy field views); bincode decode is eager (owns the
struct). Timing "decode" alone would unfairly reward SBE for doing nothing. So
**every codec's decode reads all fields** — walks the `entries` group, reads every
scalar and every `command` byte, and folds them into a `u64` checksum that is fed
to `std::hint::black_box`. The compiler cannot elide the reads; all three codecs
do the same materialization work. bincode reaches full materialization by
constructing the owned struct; SBE reaches it by explicit field access.

### Memory: deterministic allocation counting

`decode_alloc_bytes` is measured with a counting `#[global_allocator]` (a thin
wrapper over `System` that atomically sums `alloc`/`realloc` request sizes). We
snapshot the counter before and after the decode+materialize region and report
the delta per record. This is **deterministic and repeatable** — unlike RSS
sampling — and isolates the headline result:

- **SBE (`sbe_gen`, `aeron_sbe`):** ~0 bytes — decode is a cast/view over the
  existing journal buffer; materialization reads in place.
- **`bincode`:** allocates the owned struct plus the `entries` `Vec` and each
  command `Vec<u8>` on every decode.

Over a journal replay of millions of records that gap is the story the focus area
exists to tell.

## Journal-simulation harness

The timed loop mirrors a command-log write/replay cycle rather than a bare
microbench:

1. **Encode phase:** append `N` records (each built from its index) into one
   contiguous in-memory **journal buffer**, timing per-record encode.
2. **Replay phase:** scan the journal, decode + materialize each record, timing
   per-record decode and accumulating the checksum.

`decode_alloc_bytes` is captured during the replay phase, so it reflects real
replay pressure. `bench-common` owns the timed-loop / Stats / emit machinery; the
per-cell binaries stay thin.

## Layout and build integration

New Cargo members under `rust/serialization/`:

```
rust/serialization/
  common/       # serialization-common: schema.xml, bincode mirror struct, record builder, JournalCfg
  sbe_gen/      # serialization-sbe_gen  binary (+ build.rs → sbe_gen::generate_to)
  aeron_sbe/    # serialization-aeron_sbe binary (+ build.rs → java -jar sbe-all.jar; vendored jar)
  bincode/      # serialization-bincode  binary
```

- **`serialization-common`** is a small shared crate holding the single-source
  `schema.xml`, the bincode mirror `JournalRecord` struct, the deterministic
  record builder, and the `JournalCfg` env parser (`SER_ENTRIES`, `SER_CMD_BYTES`,
  warmup/iterations). Keeps the schema and the record shape single-source across
  all three cells.
- **`sbe_gen` cell:** `build.rs` calls `sbe_gen::generate_to(schema.xml → OUT_DIR)`.
  `sbe_gen` in `[build-dependencies]`, `zerocopy` in `[dependencies]`. Generated
  code stays out of git (OUT_DIR); the XML is the single source of truth (the
  prost-build / tonic-build idiom).
- **`aeron_sbe` cell:** `build.rs` shells `java -jar <sbe-all.jar> ... -Dsbe.target.language=Rust`
  against the same `schema.xml`, output to `OUT_DIR`. The jar is **vendored** in the
  repo (`rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar`) so the build is
  hermetic and CI-reproducible; `SBE_JAR` env overrides the path. `build.rs` fails
  with a clear message if `java` is absent. The generated flyweights have no runtime
  crate dependency.
- **`bincode` cell:** no codegen; derives on the mirror struct from
  `serialization-common`.

All three are workspace members inheriting `[workspace.package]`; shared deps
(`serde`, `bincode`, `zerocopy`, `sbe_gen`) go in `[workspace.dependencies]`, kept
experiment-scoped per the workspace conventions. Workspace stays clippy- and
rustfmt-clean.

### Toolchain note

`aeron_sbe` adds a **JDK build-time dependency** (to run `sbe-all.jar`) to what has
been a pure-Cargo workspace. This is contained to that one cell's `build.rs` and
documented; the other two cells build with Cargo alone.

## Tests

- **Cross-codec round-trip:** build a record, encode+decode through each of the
  three codecs, assert materialized field equality against the source record.
- **SBE byte-identity:** assert `sbe_gen` and `aeron_sbe` produce byte-for-byte
  identical output for the same record (golden bytes).
- **Size sanity:** default config encodes to within a tolerance band of ~500 bytes,
  guarding the payload-tuning knobs.
- **Stats** reuse from `bench-common` (already tested); no new Stats code.

## bench-infra

Add three rows to `bench-infra/ansible/group_vars/all.yml`'s `experiments` matrix:
`serialization-sbe_gen`, `serialization-aeron_sbe`, `serialization-bincode` — all
single-host on node0 (no responder, no NVMe requirement). The AWS image must have
JDK 21 available for the `aeron_sbe` build (already true — the Java benchmarks need
it); note this in the bench-infra provisioning so the build is not a surprise.

## Result-contract compliance

- stdout carries **only** result lines; the `java`/codegen chatter from `build.rs`
  is compile-time, not runtime, and never touches benchmark stdout.
- One line per metric; every line carries `experiment` and `language:"rust"`.
- Journaling follows the project rule: only **real AWS single-host runs** are
  recorded via `tools/journal`; local runs are fitness checks only.

## Non-goals

- No Go or Java benchmark cell.
- No cross-language SBE comparison (deferred).
- No on-disk journal (that is `filesystem-write`'s job); the journal here is an
  in-memory buffer so the measurement isolates the codec.
- No protobuf/capnp/rkyv baseline in this spec (bincode is the chosen baseline);
  additional codecs are future experiments, added as new rows.
