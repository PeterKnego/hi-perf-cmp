# serialization — Go SBE cells (flyweight + struct) — Design

**Date:** 2026-07-22
**Status:** Approved

## Purpose

Add Go SBE codecs to the `serialization` focus area, generated from the shared
`journal.xml` by the same real-logic `sbe-tool` the Rust `aeron_sbe` cell uses.
The SBE-Golang generator has **two modes**, one codegen flag apart, and both are
benchmarked because their cost profiles differ in exactly the way this focus area
exists to measure:

- **flyweight** (`-Dsbe.go.generate.generate.flyweights=true`) — zero-copy: wraps
  a caller buffer, reads/writes fields in place, no owned message struct. The
  honest cross-language twin of Rust `aeron_sbe` (same tool, same flyweight
  approach, byte-identical wire, **~0 decode allocation**).
- **struct** (default) — owned: streams through `io.Writer`/`io.Reader` via a
  `SbeGoMarshaller` and materializes an owned `JournalRecord` (with its `Entries`
  slice and per-entry `Command []byte`) on decode, so **nonzero decode
  allocation**.

The flyweight-vs-struct contrast — same tool, same wire, opposite allocation
behavior — is the finding. It also corrects an initial assumption that "SBE-Go
isn't zero-copy": that is true only of the default struct mode.

## Experiments and the grid

| experiment   | language | mode      | decode alloc (expected) |
|--------------|----------|-----------|-------------------------|
| `aeron_sbe`  | rust     | flyweight | 0 B (existing cell)     |
| `aeron_sbe`  | **go**   | flyweight | ~0 B (new)              |
| `sbe_struct` | **go**   | struct    | > 0 B (new)             |

- The flyweight cell **reuses experiment `aeron_sbe`** with `language: go`. It is
  literally "aeron_sbe in Go" — the same real-logic tool and flyweight codegen —
  giving a clean `aeron_sbe × {rust, go}` cross-language comparison on the cell
  key `(serialization, aeron_sbe, go)`. The `tools/journal` CLI already aligns on
  `(focus_area, experiment, language, metric)`, so this is a distinct cell from
  the Rust one, not a collision.
- The struct cell is a **new experiment `sbe_struct`** (`language: go`) — a
  distinct codec cost profile, not a language port of `aeron_sbe`.

## Codegen — two committed packages (mirrors smr-collections booksnap)

Both packages are generated from the single shared schema
`rust/serialization/aeron_sbe/schema/journal.xml` by the vendored
`rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar`, and **committed** (bench
hosts need no JDK; JDK is required only to regenerate — the existing
`regen-booksnap.sh` / `regen.sh` precedent). Namespaces keep the Go package names
clean (`-Dsbe.target.namespace=…`, as aeron-go's own `generate.sh` does).

```
go/internal/serjournal/
  journalsbe/               # flyweight codec (committed), package journalsbe
  journalsbestruct/         # struct codec (committed), package journalsbestruct
  regen-journalsbe.sh       # jar + -Dsbe.go.generate.generate.flyweights=true -Dsbe.target.namespace=journalsbe
  regen-journalsbestruct.sh # jar (struct default) + -Dsbe.target.namespace=journalsbestruct
  sbe.go                    # flyweight adapter
  sbe_struct.go             # struct adapter
  sbe_test.go               # round-trip + byte-identity + size tests (both modes)
```

Both generated packages are pure Go (imports stdlib only — `fmt`, `io`,
`io/ioutil`, `math`, `strings`, `unicode/utf8` for struct; `fmt`, `strings` etc.
for flyweight); **no new module dependency**. The `io/ioutil` import in the
generator output is deprecated-but-harmless (same as the committed booksnap
codec). aeron-go was the reference for the output shape only; nothing depends on
it.

### Verified generator facts (scratch-confirmed against the vendored jar)

- Flyweight message API: `WrapAndApplyHeader(buffer []byte, offset, bufferLength uint64)`,
  chained setters (`SetLeadershipTermId(int64) *JournalRecord`, …,
  `SetEventType(EventType)`, `SetFlags(uint8)`), group builder
  `EntriesCount(uint16) *JournalRecordEntries` then `Next()` per entry with
  `SetEntryTermId`/`SetEntryIndex`/`SetEntryTimestamp`/`SetCommandKey` and
  `PutCommand(string)` for the var-data field; `EncodedLength() uint64` and the
  const `MessageHeaderEncodedLength uint64 = 8`. Decode: `MessageHeader.Wrap(...)`
  then `JournalRecord.WrapForDecode(buffer, offset, actingBlockLength,
  actingVersion, bufferLength)`, scalar getters (`LeadershipTermId() int64`, …),
  group `Entries() *JournalRecordEntries` + `HasNext()`/`Next()`, and
  `GetCommand(dst []byte) int` (copies var-data into a caller buffer — the
  zero-alloc read path; `Command() string` also exists but allocates). `EventType`
  is `EventType uint8` with `EventType_APPEND`/`EventType_SNAPSHOT`.
  A framed record at the default config (4 entries × 78-byte command) encodes to
  **502 bytes** — byte-for-byte the Rust SBE size.
- Struct message API: owned `type JournalRecord struct { …; EventType
  EventTypeEnum; Flags uint8; Entries []JournalRecordEntries }`, `type
  JournalRecordEntries struct { …; CommandKey int32; Command []uint8 }`;
  `Encode(_m *SbeGoMarshaller, _w io.Writer, doRangeCheck bool) error` and
  `Decode(_m *SbeGoMarshaller, _r io.Reader, actingVersion, blockLength uint16,
  doRangeCheck bool) error`; `SbeBlockLength()`/`SbeTemplateId()`/`SbeSchemaId()`/
  `SbeSchemaVersion()`; `MessageHeader.Encode/Decode(io.Writer/Reader)` with
  `EncodedLength() int64 == 8`.

## Adapters + cells

Both cells plug into the existing `bench.RunJournal(experiment, cfg, build, encode,
decode)` (Go bench library) — no harness change. `build(i)` is untimed
pre-build; `encode(rec, scratch) int` and `decode(bytes) uint64` (folding every
field into the checksum for full materialization) are the timed paths.

### Flyweight — `serjournal/sbe.go` + `go/cmd/serialization-aeron_sbe/`

- `EncodeSBE(r *Record, scratch []byte) int`: `WrapAndApplyHeader(scratch, 0,
  len)`, write scalars via chained setters, `EntriesCount(len(entries))`, then per
  entry `Next()` + scalar setters + `PutCommand`. The command is a `[]byte` in the
  logical `Record`; to feed the string-typed `PutCommand` without a per-entry
  allocation, pass an `unsafe.String(&cmd[0], len(cmd))` view (Go 1.20+, zero-copy
  — `PutCommand` immediately `copy`s the bytes into the buffer and never retains
  the string). Return `MessageHeaderEncodedLength + EncodedLength()`.
- `DecodeSBEChecksum(b []byte) uint64`: `MessageHeader.Wrap` to read
  blockLength/version, `JournalRecord.WrapForDecode`, read every scalar in place,
  iterate `Entries()` via `HasNext()/Next()`, read entry scalars and
  `GetCommand(dst)` into a **reused** scratch buffer, folding all fields in
  `ChecksumRecord` order. Zero allocation.
- The cmd (`experiment = "aeron_sbe"`) captures the reused decode scratch in its
  closures and calls `bench.RunJournal`. `build(i)` returns the logical
  `serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)` (flyweight encodes
  directly from it — no intermediate message struct).

### Struct — `serjournal/sbe_struct.go` + `go/cmd/serialization-sbe_struct/`

- `ToSBEStruct(r *Record) journalsbestruct.JournalRecord`: convert the logical
  record to the owned message struct (untimed pre-build, like `ToBebop`).
- `EncodeSBEStruct(rec journalsbestruct.JournalRecord, scratch []byte) int`: write
  the 8-byte header then `rec.Encode` through a small **reused** zero-alloc
  slice-writer (`io.Writer` over `scratch` tracking an offset). Return bytes
  written.
- `DecodeSBEStructChecksum(b []byte) uint64`: reset a **reused** `bytes.Reader`
  over `b`, decode the header, decode into a **fresh** `journalsbestruct
  .JournalRecord` per call (the honest owned-decode cost — matches how bebop /
  protobuf produce fresh owned structs), fold all fields. Nonzero allocation.
- The cmd (`experiment = "sbe_struct"`) mirrors the bebop cmd: `build(i)` returns
  `ToSBEStruct(&BuildRecord(...))`.

## Metrics and measurement fairness

Each cell emits the same four `serialization` metrics via `RunJournal`:
`encode_p50`, `encode_p99` (int ns), `encode_mean` (float ns), `decode_p50`,
`decode_p99`, `decode_mean`, `encoded_bytes` (502, samples=1), `decode_alloc_bytes`
(bytes/decode, samples=iters). `bench.Emit` forces `language: "go"`;
`focus_area: "serialization"`.

The reused marshaller / slice-writer / `bytes.Reader` / command scratch are
allocated **once, before timing**, so `decode_alloc_bytes` isolates the codec's
materialization cost — ~0 for flyweight (in-place reads + copy into reused
buffer), the owned struct + `Entries` slice + per-entry command for struct mode —
not io-wrapper churn. Both decodes fully materialize every field (fold into the
checksum), so decode latency is comparable across all SBE cells and against
bincode/bebop/protobuf.

## Tests

- **Round-trip checksum, each mode**: `Decode…Checksum(Encode…(build(i)))` equals
  `serjournal.ChecksumRecord(build(i))` for several indices and configs.
- **Cross-language + cross-mode byte-identity**: both Go modes' encoded frames are
  byte-identical to a committed golden SBE frame produced by the Rust SBE encode
  of the same record (generated once from `serialization-sbe_gen` / `-aeron_sbe`
  and committed under `go/internal/serjournal/testdata/`). This proves all four
  SBE cells — Rust `sbe_gen`/`aeron_sbe`, Go flyweight/struct — put identical
  bytes on the wire, the fairness anchor that justifies reusing the `aeron_sbe`
  experiment name for the Go flyweight cell.
- **Size sanity**: `encoded_bytes == 502` for both modes at the default config.
- Green `cd go && go build ./... && go vet ./... && go test ./...`; new hand-
  written files gofmt-clean (generated files are gofmt'd by the regen scripts).

## bench-infra and docs

- `bench-infra/ansible/group_vars/all.yml`: change the `aeron_sbe` row from
  `languages: [rust]` to `languages: [rust, go]`, and add
  `- { focus_area: serialization, experiment: sbe_struct, kind: local, languages: [go] }`.
  The `ser_*` params are shared; no new params. `run_bench.sh` needs no change
  (`serialization` is already a known focus area and the Go artifacts are built by
  the existing `go build ./cmd/...`).
- CLAUDE.md: update the `serialization` status paragraph (Go now has SBE too — a
  zero-copy flyweight `aeron_sbe` cell plus an owned-decode `sbe_struct` cell), the
  artifact-names line (`serialization-{aeron_sbe}` now Rust **and** Go, add
  `serialization-sbe_struct` Go), and a Go run example.
- RESULTS.md is updated only after a real AWS run (the journaling discipline).

## Result-contract compliance

- stdout carries only result lines; codec/generator chatter is compile/regen-time.
- One line per metric; every line carries `experiment`, `language: "go"`, and
  `focus_area: "serialization"`.
- Only real AWS single-host runs are journaled; loopback/dev runs are fitness
  checks only.

## Non-goals

- No Java SBE serialization cell.
- No new module dependency (no dependency on aeron-go; codecs are generated in
  repo from the shared schema).
- No change to the Rust cells, `journal.xml`, the `SER_*` contract, or
  `RunJournal`.
- No attempt to make the struct-mode cell zero-copy — its owned-decode cost is the
  point of measuring it alongside the flyweight cell.
