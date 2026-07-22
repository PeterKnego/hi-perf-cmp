# Go SBE serialization cells (flyweight + struct) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two Go SBE codecs to the `serialization` focus area — a zero-copy **flyweight** cell (experiment `aeron_sbe`, language go) and an owned-struct cell (experiment `sbe_struct`, language go) — both generated from the shared `journal.xml` by the vendored real-logic sbe-tool.

**Architecture:** Two committed generated packages (`journalsbe` flyweight, `journalsbestruct` struct) produced by the same `sbe-all-1.38.1.jar` that the Rust `aeron_sbe` cell uses, differing only by the `-Dsbe.go.generate.generate.flyweights` flag. Thin adapters + `bench.RunJournal` cells reuse all io state so `decode_alloc_bytes` isolates the codec's materialization cost (~0 flyweight, owned graph struct). A committed golden SBE frame (Rust-sourced) anchors byte-identity across all four SBE cells.

**Tech Stack:** Go 1.22 (stdlib only — no new module dependency; `unsafe.String` on the flyweight encode path), the vendored `sbe-all-1.38.1.jar` + a JDK at regen time only.

**Spec:** `docs/superpowers/specs/2026-07-22-serialization-go-sbe-design.md`

## Global Constraints

- stdout carries **only** result-contract JSON lines; codec/generator chatter is compile/regen-time.
- Metrics per cell (via `bench.RunJournal`): `encode_p50`, `encode_p99` (int ns), `encode_mean` (float ns), `decode_p50`, `decode_p99`, `decode_mean`, `encoded_bytes` (502, samples=1), `decode_alloc_bytes` (bytes/decode, samples=iters). Focus area `serialization`; language `go` (forced by `bench.Emit`).
- Experiments: flyweight cell emits `experiment: "aeron_sbe"`; struct cell emits `experiment: "sbe_struct"`.
- Generated code is **committed** (bench hosts need no JDK); regen scripts are dev-time only, mirroring `go/internal/smrcoll/regen-booksnap.sh`.
- No new Go module dependency; no dependency on `../aeron-go`. No change to the Rust cells, `journal.xml`, the `SER_*` contract, or `bench.RunJournal`.
- Reused io state (marshaller / slice-writer / `bytes.Reader` / flyweight structs / command scratch) is allocated once, outside the timed loop, so `decode_alloc_bytes` reflects materialization only.
- Both decodes fully materialize every field (fold into the checksum in `ChecksumRecord` order).
- Keep `cd go && go build ./... && go vet ./... && go test ./...` green; hand-written files gofmt-clean (generated files gofmt'd by the regen scripts).
- Local runs are fitness checks only — never journaled.

### Shared facts (verified against the vendored jar + repo)

- Schema: `rust/serialization/aeron_sbe/schema/journal.xml` (package/schemaId 7, message `JournalRecord` blockLength 50, group `entries` blockLength 28, var-data `command`). Jar: `rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar`.
- `serjournal.Record` fields: `LeadershipTermID, LogPosition, Timestamp, ClusterSessionID, CorrelationID int64; LeaderMemberID, ServiceID int32; EventType, Flags uint8; Entries []Entry`. `serjournal.Entry`: `EntryTermID, EntryIndex, EntryTimestamp int64; CommandKey int32; Command []byte`.
- `serjournal.BuildRecord(index uint64, entries, cmdBytes int) Record`; `serjournal.ChecksumRecord(*Record) uint64`; `serjournal.Checksum` with `NewChecksum() Checksum`, `(*Checksum).AddI64/AddI32/AddU8/AddBytes`, `(Checksum).Finish() uint64`. Fold order: LeadershipTermID, LogPosition, Timestamp, ClusterSessionID, CorrelationID (I64); LeaderMemberID, ServiceID (I32); EventType, Flags (U8); per entry EntryTermID, EntryIndex, EntryTimestamp (I64), CommandKey (I32), Command (Bytes).
- `bench.RunJournal[R any](experiment string, cfg bench.SerialConfig, build func(uint64) R, encode func(R, []byte) int, decode func([]byte) uint64)`; `bench.LoadSerialConfig() (SerialConfig, error)` with `cfg.Entries`, `cfg.CmdBytes`; `bench.Fatalf(prefix, format, args...)`.
- Framed encoded size at default config (4 entries × 78 command bytes) = **502 bytes** for every SBE cell.

### Golden SBE frame (Rust-sourced, verified byte-identical for both Go modes)

`serjournal.BuildRecord(7, 4, 78)` encodes (SBE, header+body) to this 502-byte frame (hex), identical from Rust `sbe_gen`/`aeron_sbe` and both Go modes:

```
3200010007000100d70d3259e4e1cb63000700000000000045ceab7e97c2b4b83259e4e1cb63000005d12c8c1f0590090d3259e459e4e1cb01eb1c00040045ceab7e97c2b4b81c00000000000000785dbc619147019c97c2b4b84e00000045cfa97d93c7b2bf4dc7a1759bcfbab755dfb96d83d7a2af5dd7b1658bdfaaa765ef895db3e7929f6de78155bbef9a9775ff994da3f7828f7df79145abff8a87058fe93dd387f2ff0d87e135db8f1f3b8c704c1c88311d00000000000000fb869f9984ff40734c1c88314e0000001f3a8e7348198e361732867b4011863e0f2a9e6358099e260722966b5001962e3f1aae536839ae163712a65b6031a61e2f0abe437829be062702b64b7021b60e5f7ace330859ce765772c63b00513a9fd3d33c1766081e000000000000000fdea9e9a6b736873c1766084e0000003a9ed1d03812600f3296d9d8301a68072a8ec1c02802701f2286c9c8200a78171abef1f01832402f12b6f9f8103a48270aaee1e00822503f02a6e9e8002a58377ade91907852204f72d69998705ae35c788cea9145e41f00000000000000ef9b1191933cd961ea9145e44e000000e35d7a8fee9443e3eb557287e69c4bebf34d6a9ffe8453f3fb456297f68c5bfbc37d5aafceb463c3cb7552a7c6bc6bcbd36d4abfdea473d3db6542b7d6ac7bdba31d3acfaed403a3ab1532c7a6dc
```

Regenerate with a scratch Rust bin depending on `serialization-common` + `serialization-aeron_sbe`, printing the hex of `serialization_aeron_sbe::encode(&build_record(7,4,78), buf)`.

---

### Task 1: Flyweight cell — `aeron_sbe` (go), zero-copy

**Files:**
- Create: `go/internal/serjournal/regen-journalsbe.sh` (mode 755)
- Create: `go/internal/serjournal/journalsbe/` (generated, committed)
- Create: `go/internal/serjournal/testdata/journal_sbe_golden.bin` (502 bytes)
- Create: `go/internal/serjournal/sbe.go`
- Test: `go/internal/serjournal/sbe_test.go`
- Create: `go/cmd/serialization-aeron_sbe/main.go`

**Interfaces:**
- Consumes: `serjournal.{Record, Entry, BuildRecord, ChecksumRecord, NewChecksum, Checksum}`, `bench.{SerialConfig, LoadSerialConfig, RunJournal, Fatalf}`.
- Produces:
  - `serjournal.SBECodec` with `serjournal.NewSBECodec() *SBECodec`, `(*SBECodec) Encode(r Record, scratch []byte) int`, `(*SBECodec) DecodeChecksum(frame []byte) uint64`.
  - artifact `serialization-aeron_sbe` (Go).
- Generated flyweight API (verified): `journalsbe.JournalRecord` value flyweight with `WrapAndApplyHeader(buf []byte, offset, bufferLength uint64) *JournalRecord`, chained setters `SetLeadershipTermId(int64)`/`SetLogPosition`/`SetTimestamp`/`SetClusterSessionId`/`SetCorrelationId`/`SetLeaderMemberId(int32)`/`SetServiceId`/`SetEventType(journalsbe.EventType)`/`SetFlags(uint8)`, group `EntriesCount(uint16) *JournalRecordEntries` then `Next()` + `SetEntryTermId/SetEntryIndex/SetEntryTimestamp(int64)`/`SetCommandKey(int32)`/`PutCommand(string)`, and `EncodedLength() uint64`; const `journalsbe.MessageHeaderEncodedLength uint64 = 8`. Decode: `journalsbe.MessageHeader` with `Wrap(buf []byte, offset, actingVersion, bufferLength uint64) *MessageHeader`, `BlockLength() uint16`, `Version() uint16`; `JournalRecord.WrapForDecode(buf []byte, offset, actingBlockLength, actingVersion, bufferLength uint64) *JournalRecord`, scalar getters (`LeadershipTermId() int64`, …, `EventType() journalsbe.EventType`, `Flags() uint8`), group `Entries() *JournalRecordEntries` + `HasNext() bool`/`Next()`, entry getters + `GetCommand(dst []byte) int`. Enum: `journalsbe.EventType` (uint8), `EventType_APPEND`/`EventType_SNAPSHOT`.

- [ ] **Step 1: Regen script**

`go/internal/serjournal/regen-journalsbe.sh`:

```sh
#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Go *flyweight* codec from the shared
# schema. Requires a JDK (regeneration only; normal builds use committed output).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jar="$root/rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$root/rust/serialization/aeron_sbe/schema/journal.xml"
rm -rf "$here/journalsbe"
java -Dsbe.target.language=Golang \
     -Dsbe.go.generate.generate.flyweights=true \
     -Dsbe.target.namespace=journalsbe \
     -Dsbe.output.dir="$here" -jar "$jar" "$schema"
gofmt -w "$here/journalsbe"
echo "regenerated + gofmt'd $here/journalsbe" 1>&2
```

- [ ] **Step 2: Generate + commit the codec**

```bash
chmod +x go/internal/serjournal/regen-journalsbe.sh
./go/internal/serjournal/regen-journalsbe.sh
cd go && go build ./internal/serjournal/journalsbe/
```

Expected: `go/internal/serjournal/journalsbe/{JournalRecord,MessageHeader,EventType,GroupSizeEncoding,VarDataEncoding,Utils}.go` appear (package `journalsbe`, pure Go); build passes.

- [ ] **Step 3: Create the golden testdata file**

Write the 502-byte golden frame from the plan's hex to `go/internal/serjournal/testdata/journal_sbe_golden.bin`:

```bash
mkdir -p go/internal/serjournal/testdata
printf '3200010007000100d70d3259e4e1cb63000700000000000045ceab7e97c2b4b83259e4e1cb63000005d12c8c1f0590090d3259e459e4e1cb01eb1c00040045ceab7e97c2b4b81c00000000000000785dbc619147019c97c2b4b84e00000045cfa97d93c7b2bf4dc7a1759bcfbab755dfb96d83d7a2af5dd7b1658bdfaaa765ef895db3e7929f6de78155bbef9a9775ff994da3f7828f7df79145abff8a87058fe93dd387f2ff0d87e135db8f1f3b8c704c1c88311d00000000000000fb869f9984ff40734c1c88314e0000001f3a8e7348198e361732867b4011863e0f2a9e6358099e260722966b5001962e3f1aae536839ae163712a65b6031a61e2f0abe437829be062702b64b7021b60e5f7ace330859ce765772c63b00513a9fd3d33c1766081e000000000000000fdea9e9a6b736873c1766084e0000003a9ed1d03812600f3296d9d8301a68072a8ec1c02802701f2286c9c8200a78171abef1f01832402f12b6f9f8103a48270aaee1e00822503f02a6e9e8002a58377ade91907852204f72d69998705ae35c788cea9145e41f00000000000000ef9b1191933cd961ea9145e44e000000e35d7a8fee9443e3eb557287e69c4bebf34d6a9ffe8453f3fb456297f68c5bfbc37d5aafceb463c3cb7552a7c6bc6bcbd36d4abfdea473d3db6542b7d6ac7bdba31d3acfaed403a3ab1532c7a6dc' | xxd -r -p > go/internal/serjournal/testdata/journal_sbe_golden.bin
wc -c go/internal/serjournal/testdata/journal_sbe_golden.bin   # → 502
```

- [ ] **Step 4: Write the failing tests**

`go/internal/serjournal/sbe_test.go`:

```go
package serjournal

import (
	"bytes"
	"os"
	"testing"
)

func TestSBEFlyweightRoundTrip(t *testing.T) {
	codec := NewSBECodec()
	scratch := make([]byte, 64*1024)
	for _, cfg := range [][2]int{{4, 78}, {2, 8}, {6, 40}} {
		for _, idx := range []uint64{0, 1, 42} {
			r := BuildRecord(idx, cfg[0], cfg[1])
			n := codec.Encode(r, scratch)
			if got, want := codec.DecodeChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
				t.Errorf("cfg%v idx%d: decode checksum %#x != fold %#x", cfg, idx, got, want)
			}
		}
	}
}

func TestSBEFlyweightByteIdentity(t *testing.T) {
	golden, err := os.ReadFile("testdata/journal_sbe_golden.bin")
	if err != nil {
		t.Fatal(err)
	}
	if len(golden) != 502 {
		t.Fatalf("golden size %d, want 502", len(golden))
	}
	codec := NewSBECodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(7, 4, 78)
	n := codec.Encode(r, scratch)
	if !bytes.Equal(scratch[:n], golden) {
		t.Fatalf("flyweight frame (%d bytes) not byte-identical to Rust golden", n)
	}
}

func TestSBEFlyweightDecodeZeroAlloc(t *testing.T) {
	codec := NewSBECodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(0, 4, 78)
	n := codec.Encode(r, scratch)
	frame := scratch[:n]
	avg := testing.AllocsPerRun(1000, func() { _ = codec.DecodeChecksum(frame) })
	if avg != 0 {
		t.Errorf("flyweight decode allocs/op = %v, want 0", avg)
	}
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestSBEFlyweight`
Expected: FAIL — `undefined: NewSBECodec`.

- [ ] **Step 5: Write the flyweight adapter**

`go/internal/serjournal/sbe.go`:

```go
package serjournal

import (
	"unsafe"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalsbe"
)

// SBECodec is the zero-copy SBE (flyweight) adapter. It reuses the flyweight
// message/header structs and a command scratch buffer so DecodeChecksum
// allocates nothing on the timed path.
type SBECodec struct {
	enc journalsbe.JournalRecord
	hdr journalsbe.MessageHeader
	dec journalsbe.JournalRecord
	cmd []byte
}

// NewSBECodec allocates the reusable flyweight state once.
func NewSBECodec() *SBECodec {
	return &SBECodec{cmd: make([]byte, 64*1024)}
}

// Encode writes a full framed message (header + body) into scratch and returns
// its length. Zero-copy: the flyweight writes fields directly at wire offsets.
func (c *SBECodec) Encode(r Record, scratch []byte) int {
	m := &c.enc
	m.WrapAndApplyHeader(scratch, 0, uint64(len(scratch)))
	m.SetLeadershipTermId(r.LeadershipTermID).
		SetLogPosition(r.LogPosition).
		SetTimestamp(r.Timestamp).
		SetClusterSessionId(r.ClusterSessionID).
		SetCorrelationId(r.CorrelationID).
		SetLeaderMemberId(r.LeaderMemberID).
		SetServiceId(r.ServiceID).
		SetEventType(journalsbe.EventType(r.EventType)).
		SetFlags(r.Flags)
	g := m.EntriesCount(uint16(len(r.Entries)))
	for i := range r.Entries {
		e := &r.Entries[i]
		g.Next()
		g.SetEntryTermId(e.EntryTermID).
			SetEntryIndex(e.EntryIndex).
			SetEntryTimestamp(e.EntryTimestamp).
			SetCommandKey(e.CommandKey)
		// PutCommand takes a string but copies the bytes immediately and never
		// retains the header, so an unsafe.String view avoids a per-entry alloc.
		if len(e.Command) > 0 {
			g.PutCommand(unsafe.String(&e.Command[0], len(e.Command)))
		} else {
			g.PutCommand("")
		}
	}
	return int(journalsbe.MessageHeaderEncodedLength) + int(m.EncodedLength())
}

// DecodeChecksum decodes the framed message in place and folds every field in
// the canonical ChecksumRecord order (full materialization). Zero allocation:
// scalars are read in place, the command is copied into the reused buffer.
func (c *SBECodec) DecodeChecksum(frame []byte) uint64 {
	c.hdr.Wrap(frame, 0, 0, uint64(len(frame)))
	c.dec.WrapForDecode(frame, uint64(journalsbe.MessageHeaderEncodedLength),
		uint64(c.hdr.BlockLength()), uint64(c.hdr.Version()), uint64(len(frame)))
	ck := NewChecksum()
	ck.AddI64(c.dec.LeadershipTermId())
	ck.AddI64(c.dec.LogPosition())
	ck.AddI64(c.dec.Timestamp())
	ck.AddI64(c.dec.ClusterSessionId())
	ck.AddI64(c.dec.CorrelationId())
	ck.AddI32(c.dec.LeaderMemberId())
	ck.AddI32(c.dec.ServiceId())
	ck.AddU8(uint8(c.dec.EventType()))
	ck.AddU8(c.dec.Flags())
	e := c.dec.Entries()
	for e.HasNext() {
		e.Next()
		ck.AddI64(e.EntryTermId())
		ck.AddI64(e.EntryIndex())
		ck.AddI64(e.EntryTimestamp())
		ck.AddI32(e.CommandKey())
		n := e.GetCommand(c.cmd)
		ck.AddBytes(c.cmd[:n])
	}
	return ck.Finish()
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestSBEFlyweight && gofmt -l internal/serjournal/sbe.go`
Expected: all three tests PASS (round-trip, byte-identity, 0 allocs/op); gofmt clean.

- [ ] **Step 6: Write the cell main**

`go/cmd/serialization-aeron_sbe/main.go`:

```go
// serialization-aeron_sbe (Go): encode/decode cost of the ~500-byte journal
// record via the real-logic SBE tool's zero-copy Golang flyweight codec — the
// Go twin of the Rust aeron_sbe cell (same tool, same wire).
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
)

const experiment = "aeron_sbe"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	codec := serjournal.NewSBECodec()
	bench.RunJournal(experiment, cfg,
		func(i uint64) serjournal.Record {
			return serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
		},
		codec.Encode,
		codec.DecodeChecksum,
	)
}
```

- [ ] **Step 7: Build, test, loopback smoke**

Run: `cd go && go build ./... && go vet ./... && go test ./internal/serjournal/`
Then: `cd go && SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-aeron_sbe | tee /dev/stderr | wc -l`
Expected: exactly 8 JSON lines — `encode_{p50,p99,mean}`, `decode_{p50,p99,mean}`, `encoded_bytes` (502), `decode_alloc_bytes` (0 — zero-copy) — each `"focus_area":"serialization"`, `"language":"go"`, `"experiment":"aeron_sbe"`.

- [ ] **Step 8: Commit**

```bash
git add go/internal/serjournal/regen-journalsbe.sh go/internal/serjournal/journalsbe/ \
        go/internal/serjournal/testdata/ go/internal/serjournal/sbe.go \
        go/internal/serjournal/sbe_test.go go/cmd/serialization-aeron_sbe/
git commit -m "feat(serialization): Go aeron_sbe cell — zero-copy SBE flyweight (0 decode alloc, byte-identical to Rust)"
```

---

### Task 2: Struct cell — `sbe_struct` (go), owned decode

**Files:**
- Create: `go/internal/serjournal/regen-journalsbestruct.sh` (mode 755)
- Create: `go/internal/serjournal/journalsbestruct/` (generated, committed)
- Create: `go/internal/serjournal/sbe_struct.go`
- Test: append to `go/internal/serjournal/sbe_test.go`
- Create: `go/cmd/serialization-sbe_struct/main.go`

**Interfaces:**
- Consumes: `serjournal.{Record, BuildRecord, ChecksumRecord, NewChecksum}`, the golden `testdata/journal_sbe_golden.bin` (Task 1), `bench.{SerialConfig, LoadSerialConfig, RunJournal, Fatalf}`.
- Produces:
  - `serjournal.ToSBEStruct(r *Record) journalsbestruct.JournalRecord`
  - `serjournal.SBEStructCodec` with `serjournal.NewSBEStructCodec() *SBEStructCodec`, `(*SBEStructCodec) Encode(msg journalsbestruct.JournalRecord, scratch []byte) int`, `(*SBEStructCodec) DecodeChecksum(frame []byte) uint64`.
  - artifact `serialization-sbe_struct` (Go).
- Generated struct API (verified): owned `journalsbestruct.JournalRecord{ LeadershipTermId, LogPosition, Timestamp, ClusterSessionId, CorrelationId int64; LeaderMemberId, ServiceId int32; EventType journalsbestruct.EventTypeEnum; Flags uint8; Entries []JournalRecordEntries }`, `JournalRecordEntries{ EntryTermId, EntryIndex, EntryTimestamp int64; CommandKey int32; Command []uint8 }`. `NewSbeGoMarshaller() *SbeGoMarshaller`; `(*JournalRecord).Encode(_m, _w io.Writer, doRangeCheck bool) error` / `.Decode(_m, _r io.Reader, actingVersion, blockLength uint16, doRangeCheck bool) error`; `SbeBlockLength()/SbeTemplateId()/SbeSchemaId()/SbeSchemaVersion() uint16`; `MessageHeader{ BlockLength, TemplateId, SchemaId, Version uint16 }` with `.Encode(_m, _w) error` / `.Decode(_m, _r, actingVersion uint16) error`.

- [ ] **Step 1: Regen script + generate + commit**

`go/internal/serjournal/regen-journalsbestruct.sh`:

```sh
#!/usr/bin/env sh
# Regenerate the committed real-logic SBE Go *struct* (owned) codec from the
# shared schema. Requires a JDK (regeneration only; builds use committed output).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
jar="$root/rust/serialization/aeron_sbe/vendor/sbe-all-1.38.1.jar"
schema="$root/rust/serialization/aeron_sbe/schema/journal.xml"
rm -rf "$here/journalsbestruct"
java -Dsbe.target.language=Golang \
     -Dsbe.target.namespace=journalsbestruct \
     -Dsbe.output.dir="$here" -jar "$jar" "$schema"
gofmt -w "$here/journalsbestruct"
echo "regenerated + gofmt'd $here/journalsbestruct" 1>&2
```

```bash
chmod +x go/internal/serjournal/regen-journalsbestruct.sh
./go/internal/serjournal/regen-journalsbestruct.sh
cd go && go build ./internal/serjournal/journalsbestruct/
```

Expected: `journalsbestruct/{JournalRecord,MessageHeader,EventType,GroupSizeEncoding,VarDataEncoding,SbeMarshalling}.go` (package `journalsbestruct`, pure Go); build passes.

- [ ] **Step 2: Write the failing tests**

Append to `go/internal/serjournal/sbe_test.go`:

```go
func TestSBEStructRoundTrip(t *testing.T) {
	codec := NewSBEStructCodec()
	scratch := make([]byte, 64*1024)
	for _, cfg := range [][2]int{{4, 78}, {2, 8}, {6, 40}} {
		for _, idx := range []uint64{0, 1, 42} {
			r := BuildRecord(idx, cfg[0], cfg[1])
			n := codec.Encode(ToSBEStruct(&r), scratch)
			if got, want := codec.DecodeChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
				t.Errorf("cfg%v idx%d: decode checksum %#x != fold %#x", cfg, idx, got, want)
			}
		}
	}
}

func TestSBEStructByteIdentity(t *testing.T) {
	golden, err := os.ReadFile("testdata/journal_sbe_golden.bin")
	if err != nil {
		t.Fatal(err)
	}
	codec := NewSBEStructCodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(7, 4, 78)
	n := codec.Encode(ToSBEStruct(&r), scratch)
	if !bytes.Equal(scratch[:n], golden) {
		t.Fatalf("struct frame (%d bytes) not byte-identical to Rust golden", n)
	}
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestSBEStruct`
Expected: FAIL — `undefined: NewSBEStructCodec`.

- [ ] **Step 3: Write the struct adapter**

`go/internal/serjournal/sbe_struct.go`:

```go
package serjournal

import (
	"bytes"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalsbestruct"
)

// sliceWriter is a zero-alloc io.Writer over a caller buffer (the encode
// scratch), so the streaming SbeGoMarshaller writes without an intermediate
// bytes.Buffer.
type sliceWriter struct {
	b []byte
	n int
}

func (w *sliceWriter) Write(p []byte) (int, error) {
	c := copy(w.b[w.n:], p)
	w.n += c
	return c, nil
}

// ToSBEStruct converts the logical record to the owned SBE message struct
// (untimed pre-build, like ToBebop). Command slices are shared, not copied.
func ToSBEStruct(r *Record) journalsbestruct.JournalRecord {
	entries := make([]journalsbestruct.JournalRecordEntries, len(r.Entries))
	for i := range r.Entries {
		e := &r.Entries[i]
		entries[i] = journalsbestruct.JournalRecordEntries{
			EntryTermId: e.EntryTermID, EntryIndex: e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp, CommandKey: e.CommandKey, Command: e.Command,
		}
	}
	return journalsbestruct.JournalRecord{
		LeadershipTermId: r.LeadershipTermID, LogPosition: r.LogPosition, Timestamp: r.Timestamp,
		ClusterSessionId: r.ClusterSessionID, CorrelationId: r.CorrelationID,
		LeaderMemberId: r.LeaderMemberID, ServiceId: r.ServiceID,
		EventType: journalsbestruct.EventTypeEnum(r.EventType), Flags: r.Flags, Entries: entries,
	}
}

// SBEStructCodec is the owned (struct-mode) SBE adapter. The marshaller,
// slice-writer, and bytes.Reader are reused; decode still materializes a fresh
// owned JournalRecord per call (the honest owned-decode cost).
type SBEStructCodec struct {
	m  *journalsbestruct.SbeGoMarshaller
	w  sliceWriter
	rd bytes.Reader
}

func NewSBEStructCodec() *SBEStructCodec {
	return &SBEStructCodec{m: journalsbestruct.NewSbeGoMarshaller()}
}

// Encode writes header + body into scratch through the reused slice-writer.
func (c *SBEStructCodec) Encode(msg journalsbestruct.JournalRecord, scratch []byte) int {
	c.w.b = scratch
	c.w.n = 0
	hdr := journalsbestruct.MessageHeader{
		BlockLength: msg.SbeBlockLength(), TemplateId: msg.SbeTemplateId(),
		SchemaId: msg.SbeSchemaId(), Version: msg.SbeSchemaVersion(),
	}
	_ = hdr.Encode(c.m, &c.w)
	_ = msg.Encode(c.m, &c.w, false)
	return c.w.n
}

// DecodeChecksum decodes into a fresh owned struct and folds every field.
func (c *SBEStructCodec) DecodeChecksum(frame []byte) uint64 {
	c.rd.Reset(frame)
	var msg journalsbestruct.JournalRecord
	var hdr journalsbestruct.MessageHeader
	_ = hdr.Decode(c.m, &c.rd, msg.SbeSchemaVersion())
	_ = msg.Decode(c.m, &c.rd, hdr.Version, hdr.BlockLength, false)
	ck := NewChecksum()
	ck.AddI64(msg.LeadershipTermId)
	ck.AddI64(msg.LogPosition)
	ck.AddI64(msg.Timestamp)
	ck.AddI64(msg.ClusterSessionId)
	ck.AddI64(msg.CorrelationId)
	ck.AddI32(msg.LeaderMemberId)
	ck.AddI32(msg.ServiceId)
	ck.AddU8(uint8(msg.EventType))
	ck.AddU8(msg.Flags)
	for i := range msg.Entries {
		e := &msg.Entries[i]
		ck.AddI64(e.EntryTermId)
		ck.AddI64(e.EntryIndex)
		ck.AddI64(e.EntryTimestamp)
		ck.AddI32(e.CommandKey)
		ck.AddBytes(e.Command)
	}
	return ck.Finish()
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestSBEStruct && gofmt -l internal/serjournal/sbe_struct.go`
Expected: PASS (round-trip + byte-identity); gofmt clean.

- [ ] **Step 4: Write the cell main**

`go/cmd/serialization-sbe_struct/main.go`:

```go
// serialization-sbe_struct (Go): encode/decode cost of the ~500-byte journal
// record via the real-logic SBE tool's default (struct/owned) Golang codec —
// same wire as aeron_sbe, but decode materializes an owned struct (nonzero alloc).
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalsbestruct"
)

const experiment = "sbe_struct"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	codec := serjournal.NewSBEStructCodec()
	bench.RunJournal(experiment, cfg,
		func(i uint64) journalsbestruct.JournalRecord {
			r := serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
			return serjournal.ToSBEStruct(&r)
		},
		codec.Encode,
		codec.DecodeChecksum,
	)
}
```

- [ ] **Step 5: Build, test, loopback smoke**

Run: `cd go && go build ./... && go vet ./... && go test ./internal/serjournal/`
Then: `cd go && SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-sbe_struct | tee /dev/stderr | wc -l`
Expected: exactly 8 JSON lines — `encoded_bytes` (502), `decode_alloc_bytes` **> 0** (owned decode), `"experiment":"sbe_struct"`, `"language":"go"`, `"focus_area":"serialization"`.

- [ ] **Step 6: Commit**

```bash
git add go/internal/serjournal/regen-journalsbestruct.sh go/internal/serjournal/journalsbestruct/ \
        go/internal/serjournal/sbe_struct.go go/internal/serjournal/sbe_test.go \
        go/cmd/serialization-sbe_struct/
git commit -m "feat(serialization): Go sbe_struct cell — owned-decode SBE (nonzero alloc, same wire as aeron_sbe)"
```

---

### Task 3: bench-infra matrix + docs

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: artifact names `serialization-aeron_sbe` (Go), `serialization-sbe_struct` (Go).
- Produces: nothing downstream — run-matrix + docs only.

- [ ] **Step 1: Update the matrix**

In `bench-infra/ansible/group_vars/all.yml`, change the `aeron_sbe` row to add Go, and add the `sbe_struct` row. The block becomes:

```yaml
  - { focus_area: serialization,    experiment: sbe_gen,     kind: local, languages: [rust] }
  - { focus_area: serialization,    experiment: aeron_sbe,   kind: local, languages: [rust, go] }
  - { focus_area: serialization,    experiment: bincode,     kind: local, languages: [rust] }
  - { focus_area: serialization,    experiment: sbe_struct,  kind: local, languages: [go] }
  - { focus_area: serialization,    experiment: bebop,       kind: local, languages: [go] }
  - { focus_area: serialization,    experiment: protobuf,    kind: local, languages: [go] }
```

(The `ser_*` params are shared; no new params. `run_bench.sh` needs no change — `serialization` is already a known focus area and `go build ./cmd/...` picks up the two new artifacts.)

- [ ] **Step 2: Update CLAUDE.md**

Three edits:
1. **Status paragraph** — extend the `serialization` sentence: Go now also has SBE — a zero-copy flyweight cell reusing experiment `aeron_sbe` (the Go twin of the Rust `aeron_sbe` flyweight, byte-identical wire, 0 decode-alloc) and an owned-decode `sbe_struct` cell (same wire, materializes an owned struct); both generated from the shared `journal.xml` by the vendored real-logic sbe-tool (`-Dsbe.go.generate.generate.flyweights` toggles the two modes).
2. **Artifact-names line** — `serialization-aeron_sbe` is now Rust **and** Go; add `serialization-sbe_struct` (Go). Result: `serialization-{sbe_gen,aeron_sbe,bincode}` (Rust; `aeron_sbe` also Go) and `serialization-{aeron_sbe,sbe_struct,bebop,protobuf}` (Go).
3. **Go build & run examples** — add `go run ./cmd/serialization-aeron_sbe` (and `-sbe_struct`).

- [ ] **Step 3: Verify**

Run: `cd go && go build ./... && go vet ./... && go test ./...` (green) and
`python3 -c "import yaml; yaml.safe_load(open('bench-infra/ansible/group_vars/all.yml'))"` (parses).
Expected: both pass.

- [ ] **Step 4: Commit**

```bash
git add bench-infra/ansible/group_vars/all.yml CLAUDE.md
git commit -m "chore(serialization): register Go aeron_sbe (flyweight) + sbe_struct cells in bench-infra matrix + docs"
```
