# serialization — Go FlatBuffers cell (zero-copy decode) — Design

**Date:** 2026-07-22
**Status:** Approved

## Purpose

Add a Google **FlatBuffers** Go codec to the `serialization` focus area — cell
`(serialization, flatbuffers, go)` — encoding/decoding the same ~500-byte
`JournalRecord`. FlatBuffers is **zero-copy on read**: generated accessors read
fields directly from the buffer with no unpack step, so decode allocates
**nothing** and is (per the `kcchu/buffer-benchmarks` comparison) the fastest
decode. This adds a second zero-copy-decode data point alongside the SBE
flyweight cell, via a completely different wire format.

## The zero-copy config

FlatBuffers' **default** generated accessors are the zero-copy path. The
allocating alternative is the object API (`flatc --gen-object-api`, which
unpacks the buffer into owned Go structs). This cell deliberately uses the
**default accessors, not the object API**. Decode is:

- `journalfb.GetRootAsJournalRecord(buf, 0)` (wraps the buffer, no copy),
- in-place scalar getters (`rec.LeadershipTermId()`, …),
- group iteration via a **reused** `Entry` accessor (`rec.Entries(&e, i)` fills a
  caller-provided `*Entry` — no per-entry allocation),
- `e.CommandBytes()` — a `[]byte` view into the buffer (zero-copy).

Every field is folded into the checksum (full materialization, so the decode
pays for the reads and is comparable to the owned-decode codecs). Verified: 0
allocs/op via `testing.AllocsPerRun`.

## Schema and codegen

FlatBuffers uses its own IDL, so this cell needs a fresh `.fbs` schema (it is
not SBE/proto). `go/internal/serjournal/schema/journal.fbs`:

```
namespace journalfb;

table Entry {
  entry_term_id:long;
  entry_index:long;
  entry_timestamp:long;
  command_key:int;
  command:[ubyte];
}

table JournalRecord {
  leadership_term_id:long;
  log_position:long;
  timestamp:long;
  cluster_session_id:long;
  correlation_id:long;
  leader_member_id:int;
  service_id:int;
  event_type:ubyte;
  flags:ubyte;
  entries:[Entry];
}

root_type JournalRecord;
```

`Entry` is a **table** (not a struct) because `command:[ubyte]` is
variable-length; tables allow variable and optional fields.

Generated with `flatc --go` into **committed** `go/internal/serjournal/journalfb/`
(`JournalRecord.go`, `Entry.go`, package `journalfb`), via a
`regen-journalfb.sh` that documents needing **flatc 23.5.26** at regen time only
(committed output → bench hosts need no flatc). flatc is provisioned as a
prebuilt GitHub-release binary (apt's `flatbuffers-compiler` also works); the
regen script notes this. The generated code is pure Go over the runtime import
`github.com/google/flatbuffers/go`.

## Adapter and cell

`go/internal/serjournal/flatbuffers.go` — an `FBCodec` that reuses all encode
state so allocation stays off the timed path:

- `type FBCodec struct { b *flatbuffers.Builder; offs []flatbuffers.UOffsetT; ent journalfb.Entry }`
- `NewFBCodec() *FBCodec` — one `flatbuffers.NewBuilder(4096)`, a reusable offsets
  slice.
- `Encode(r Record, scratch []byte) int` — `b.Reset()`, then bottom-up (FlatBuffers
  requires nested objects built before their container): for each entry create the
  command byte-vector (`b.CreateByteVector`), then the `Entry` table
  (`EntryStart`/`EntryAdd*`/`EntryEnd`), collecting offsets into the reused slice;
  build the entries vector (`JournalRecordStartEntriesVector` + `PrependUOffsetT` +
  `EndVector`); build the root table (`JournalRecordStart`/`JournalRecordAdd*`/
  `JournalRecordEnd`); `b.Finish(root)`; `return copy(scratch, b.FinishedBytes())`.
- `DecodeChecksum(frame []byte) uint64` — `GetRootAsJournalRecord`, fold every field
  in `ChecksumRecord` order (`rec.Entries(&c.ent, i)` + `c.ent.CommandBytes()`), 0
  allocation.

`go/cmd/serialization-flatbuffers/main.go` — a thin cmd (`experiment =
"flatbuffers"`) that creates an `FBCodec` and plugs `codec.Encode` /
`codec.DecodeChecksum` into the existing `bench.RunJournal`, with `build(i) =
serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)`.

## Metrics; no byte-identity anchor

Emits the four `serialization` metrics via `RunJournal`: `encode_{p50,p99,mean}`,
`decode_{p50,p99,mean}`, `encoded_bytes` (≈ **616** at the default config —
measured, not forced; FlatBuffers carries vtables + offsets so its frame is
larger than SBE's 502), `decode_alloc_bytes` (**0** — zero-copy). `bench.Emit`
forces `language: "go"`; `focus_area: "serialization"`.

FlatBuffers is a distinct wire format, so — unlike the four SBE cells — there is
**no cross-codec byte-identity**. Correctness is anchored by a **round-trip
checksum** test (`DecodeChecksum(Encode(BuildRecord(i))) == ChecksumRecord(build)`
across several indices and configs), on top of the existing `serjournal` golden
checksum that already ties the Go builder to the Rust one.

## Dependency

`github.com/google/flatbuffers` (Go runtime, **v23.5.26** to match the flatc
version) enters the Go module — pure Go, and the module has no `go` directive
(old-style `+incompatible`), so no minimum-Go constraint on the bench host (go
1.22.5). Contained to this cell. `bebop` and `protobuf` are already present.

## Tests

- **Round-trip checksum**: `DecodeChecksum(Encode(BuildRecord(i)))` equals
  `serjournal.ChecksumRecord(build)` for several `(index, entries, cmdBytes)`.
- **Zero-alloc decode**: `testing.AllocsPerRun` over `DecodeChecksum` == 0.
- **Size sanity**: `encoded_bytes` in a band (e.g. `[550, 700]`) at the default
  config, guarding against accidental object-API/format regressions.
- Green `cd go && go build ./... && go vet ./... && go test ./...`; hand-written
  files gofmt-clean (generated files gofmt'd by the regen script).

## bench-infra and docs

- `bench-infra/ansible/group_vars/all.yml`: add
  `- { focus_area: serialization, experiment: flatbuffers, kind: local, languages: [go] }`.
  Shared `ser_*` params; no new params. `run_bench.sh` needs no change.
- CLAUDE.md: extend the `serialization` status paragraph (Go now also has a
  FlatBuffers cell — zero-copy read, 0 decode-alloc, larger wire), add
  `serialization-flatbuffers` to the Go artifact list, add a Go run example.
- RESULTS.md updated only after a real AWS run.

## Result-contract compliance

- stdout carries only result lines; codec/flatc chatter is compile/regen-time.
- One line per metric; every line carries `experiment: "flatbuffers"`,
  `language: "go"`, `focus_area: "serialization"`.
- Only real AWS single-host runs are journaled; local runs are fitness checks.

## Non-goals

- No FlatBuffers object-API (allocating) variant — the point is the zero-copy
  read path.
- No Rust or Java FlatBuffers cell.
- No byte-identity with the SBE cells (different wire format).
- No change to existing cells, the shared `JournalRecord` builder, `SER_*`, or
  `RunJournal`.
