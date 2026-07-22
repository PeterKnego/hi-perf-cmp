# Go FlatBuffers serialization cell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `serialization-flatbuffers` Go cell — a Google FlatBuffers codec for the JournalRecord using the zero-copy read path (0 decode allocation).

**Architecture:** A committed `flatc`-generated Go package (`journalfb`) from a fresh `journal.fbs`, an `FBCodec` adapter that reuses the `flatbuffers.Builder` (encode, bottom-up) and reads via default zero-copy accessors (decode, 0 alloc), and a thin cmd over the existing `bench.RunJournal`. Correctness is anchored by a round-trip checksum test (FlatBuffers is its own wire format — no byte-identity with the SBE cells).

**Tech Stack:** Go 1.22, `github.com/google/flatbuffers/go` v23.5.26 runtime (pure Go, no go-directive), the `flatc` 23.5.26 compiler at regen time only (committed output).

**Spec:** `docs/superpowers/specs/2026-07-22-serialization-flatbuffers-design.md`

## Global Constraints

- stdout carries **only** result-contract JSON lines; codec/flatc chatter is compile/regen-time.
- Metrics via `bench.RunJournal`: `encode_p50/p99/mean`, `decode_p50/p99/mean`, `encoded_bytes` (≈616, samples=1), `decode_alloc_bytes` (**0** — zero-copy). experiment `flatbuffers`, language `go` (forced by `bench.Emit`), focus_area `serialization`.
- Use FlatBuffers' **default accessors** (zero-copy read) — NOT `--gen-object-api` (which allocates).
- Generated code (`journalfb/`) is **committed**; the regen script is dev-time only and needs `flatc` on PATH; bench hosts need no flatc.
- New module dependency: `github.com/google/flatbuffers` **v23.5.26** (pure Go; the module has no `go` directive so no bench-host constraint). Do not bump.
- Decode folds every field in `ChecksumRecord` order (full materialization) and allocates nothing (reused `Entry` accessor + `CommandBytes` slice views).
- Encode reuses the `Builder` (`Reset()` per call) and a reusable offsets slice — no per-encode heap allocation on the timed path.
- Keep `cd go && go build ./... && go vet ./... && go test ./...` green; hand-written files gofmt-clean (generated files gofmt'd by the regen script). No change to existing cells, the shared builder, `SER_*`, or `RunJournal`.
- Local runs are fitness checks only — never journaled.

### Verified facts (scratch-confirmed with flatc 23.5.26 + runtime v23.5.26)

- `serjournal.Record`: `LeadershipTermID, LogPosition, Timestamp, ClusterSessionID, CorrelationID int64; LeaderMemberID, ServiceID int32; EventType, Flags uint8; Entries []Entry`. `serjournal.Entry`: `EntryTermID, EntryIndex, EntryTimestamp int64; CommandKey int32; Command []byte`.
- `serjournal.BuildRecord(index uint64, entries, cmdBytes int) Record`; `serjournal.ChecksumRecord(*Record) uint64`; `serjournal.NewChecksum() Checksum` with `(*Checksum).AddI64/AddI32/AddU8/AddBytes`, `(Checksum).Finish() uint64`. Fold order: LeadershipTermID, LogPosition, Timestamp, ClusterSessionID, CorrelationID (I64); LeaderMemberID, ServiceID (I32); EventType, Flags (U8); per entry EntryTermID, EntryIndex, EntryTimestamp (I64), CommandKey (I32), Command (Bytes).
- `bench.RunJournal[R any](experiment string, cfg bench.SerialConfig, build func(uint64) R, encode func(R, []byte) int, decode func([]byte) uint64)`; `bench.LoadSerialConfig() (SerialConfig, error)` (`cfg.Entries`, `cfg.CmdBytes`); `bench.Fatalf(prefix, format, args...)`.
- Generated `journalfb` API: runtime import `flatbuffers "github.com/google/flatbuffers/go"`. `GetRootAsJournalRecord(buf []byte, offset flatbuffers.UOffsetT) *JournalRecord`; getters `(*JournalRecord).LeadershipTermId()/LogPosition()/Timestamp()/ClusterSessionId()/CorrelationId() int64`, `LeaderMemberId()/ServiceId() int32`, `EventType()/Flags() byte`, `Entries(obj *Entry, j int) bool`, `EntriesLength() int`. `(*Entry).EntryTermId()/EntryIndex()/EntryTimestamp() int64`, `CommandKey() int32`, `CommandBytes() []byte`. Builder: `EntryStart/EntryAddEntryTermId/EntryAddEntryIndex/EntryAddEntryTimestamp/EntryAddCommandKey/EntryAddCommand/EntryEnd`, `JournalRecordStart/JournalRecordAdd*(…)/JournalRecordAddEntries/JournalRecordEnd`, `JournalRecordStartEntriesVector(b, n)`; and `flatbuffers.Builder` methods `Reset()`, `CreateByteVector([]byte) UOffsetT`, `PrependUOffsetT(UOffsetT)`, `EndVector(n) UOffsetT`, `Finish(UOffsetT)`, `FinishedBytes() []byte`, `flatbuffers.NewBuilder(n)`.
- Default config (4 entries × 78 command bytes) encodes to **616 bytes**; decode is **0 allocs/op**; encode with reused builder is 0 allocs/op.

---

### Task 1: FlatBuffers codec + cell

**Files:**
- Create: `go/internal/serjournal/schema/journal.fbs`
- Create: `go/internal/serjournal/regen-journalfb.sh` (mode 755)
- Create: `go/internal/serjournal/journalfb/` (generated, committed: `JournalRecord.go`, `Entry.go`)
- Create: `go/internal/serjournal/flatbuffers.go`
- Test: `go/internal/serjournal/flatbuffers_test.go`
- Create: `go/cmd/serialization-flatbuffers/main.go`
- Modify: `go/go.mod`, `go/go.sum`

**Interfaces:**
- Consumes: `serjournal.{Record, BuildRecord, ChecksumRecord, NewChecksum, Checksum}`, `bench.{SerialConfig, LoadSerialConfig, RunJournal, Fatalf}`.
- Produces:
  - `serjournal.FBCodec` with `serjournal.NewFBCodec() *FBCodec`, `(*FBCodec) Encode(r Record, scratch []byte) int`, `(*FBCodec) DecodeChecksum(frame []byte) uint64`.
  - artifact `serialization-flatbuffers` (Go).

- [ ] **Step 1: Write the schema**

`go/internal/serjournal/schema/journal.fbs`:

```
// FlatBuffers schema for the serialization JournalRecord (see the 2026-07-22
// design spec). Entry is a table (not a struct) because command is
// variable-length. Field order/names mirror the shared logical record.
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

- [ ] **Step 2: Write the regen script**

`go/internal/serjournal/regen-journalfb.sh`:

```sh
#!/usr/bin/env sh
# Regenerate the committed FlatBuffers Go codec from schema/journal.fbs.
# Requires flatc 23.5.26 on PATH (regeneration only; normal builds use the
# committed output). Install: apt-get install flatbuffers-compiler, or download
# the prebuilt binary from https://github.com/google/flatbuffers/releases/tag/v23.5.26
set -eu
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$here/journalfb"
flatc --go -o "$here" "$here/schema/journal.fbs"
gofmt -w "$here/journalfb"
echo "regenerated + gofmt'd $here/journalfb" 1>&2
```

- [ ] **Step 3: Fetch flatc and generate the committed codec**

flatc is not on the dev box; fetch the pinned prebuilt binary, then run the regen script with it on PATH:

```bash
chmod +x go/internal/serjournal/regen-journalfb.sh
TMP=$(mktemp -d)
curl -fsSL -o "$TMP/flatc.zip" "https://github.com/google/flatbuffers/releases/download/v23.5.26/Linux.flatc.binary.g%2B%2B-10.zip"
unzip -o "$TMP/flatc.zip" -d "$TMP" >/dev/null
chmod +x "$TMP/flatc"
"$TMP/flatc" --version   # → flatc version 23.5.26
PATH="$TMP:$PATH" ./go/internal/serjournal/regen-journalfb.sh
ls go/internal/serjournal/journalfb/   # → Entry.go  JournalRecord.go
cd go && go get github.com/google/flatbuffers/go@v23.5.26+incompatible && go build ./internal/serjournal/journalfb/
```

Expected: `journalfb/{JournalRecord,Entry}.go` (package `journalfb`) generated; `go.mod` pins `github.com/google/flatbuffers v23.5.26+incompatible`; build passes.

- [ ] **Step 4: Write the failing tests**

`go/internal/serjournal/flatbuffers_test.go`:

```go
package serjournal

import "testing"

func TestFlatBuffersRoundTrip(t *testing.T) {
	codec := NewFBCodec()
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

func TestFlatBuffersEncodedSizeBand(t *testing.T) {
	codec := NewFBCodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(0, 4, 78)
	n := codec.Encode(r, scratch)
	// FlatBuffers carries vtables + offsets; ~616 B at the default config.
	if n < 550 || n > 700 {
		t.Fatalf("encoded size %d outside [550,700]", n)
	}
}

func TestFlatBuffersDecodeZeroAlloc(t *testing.T) {
	codec := NewFBCodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(0, 4, 78)
	n := codec.Encode(r, scratch)
	frame := scratch[:n]
	avg := testing.AllocsPerRun(1000, func() { _ = codec.DecodeChecksum(frame) })
	if avg != 0 {
		t.Errorf("flatbuffers decode allocs/op = %v, want 0", avg)
	}
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestFlatBuffers`
Expected: FAIL — `undefined: NewFBCodec`.

- [ ] **Step 5: Write the adapter**

`go/internal/serjournal/flatbuffers.go`:

```go
package serjournal

import (
	flatbuffers "github.com/google/flatbuffers/go"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalfb"
)

// FBCodec is the FlatBuffers adapter. It reuses the Builder, an offsets slice,
// and an Entry accessor so encode/decode allocate nothing on the timed path.
// Decode uses the default zero-copy accessors (not the object API).
type FBCodec struct {
	b    *flatbuffers.Builder
	offs []flatbuffers.UOffsetT
	ent  journalfb.Entry
}

// NewFBCodec allocates the reusable builder + offsets slice once.
func NewFBCodec() *FBCodec {
	return &FBCodec{b: flatbuffers.NewBuilder(4096), offs: make([]flatbuffers.UOffsetT, 0, 16)}
}

// Encode builds the record bottom-up into the reused Builder, then copies the
// finished bytes into scratch and returns the length.
func (c *FBCodec) Encode(r Record, scratch []byte) int {
	b := c.b
	b.Reset()

	// Nested objects (command vectors, Entry tables) must be built before the
	// containing entries vector / root table.
	if cap(c.offs) < len(r.Entries) {
		c.offs = make([]flatbuffers.UOffsetT, len(r.Entries))
	}
	c.offs = c.offs[:len(r.Entries)]
	for i := range r.Entries {
		e := &r.Entries[i]
		cmdOff := b.CreateByteVector(e.Command)
		journalfb.EntryStart(b)
		journalfb.EntryAddEntryTermId(b, e.EntryTermID)
		journalfb.EntryAddEntryIndex(b, e.EntryIndex)
		journalfb.EntryAddEntryTimestamp(b, e.EntryTimestamp)
		journalfb.EntryAddCommandKey(b, e.CommandKey)
		journalfb.EntryAddCommand(b, cmdOff)
		c.offs[i] = journalfb.EntryEnd(b)
	}

	journalfb.JournalRecordStartEntriesVector(b, len(r.Entries))
	for i := len(r.Entries) - 1; i >= 0; i-- {
		b.PrependUOffsetT(c.offs[i])
	}
	entriesVec := b.EndVector(len(r.Entries))

	journalfb.JournalRecordStart(b)
	journalfb.JournalRecordAddLeadershipTermId(b, r.LeadershipTermID)
	journalfb.JournalRecordAddLogPosition(b, r.LogPosition)
	journalfb.JournalRecordAddTimestamp(b, r.Timestamp)
	journalfb.JournalRecordAddClusterSessionId(b, r.ClusterSessionID)
	journalfb.JournalRecordAddCorrelationId(b, r.CorrelationID)
	journalfb.JournalRecordAddLeaderMemberId(b, r.LeaderMemberID)
	journalfb.JournalRecordAddServiceId(b, r.ServiceID)
	journalfb.JournalRecordAddEventType(b, r.EventType)
	journalfb.JournalRecordAddFlags(b, r.Flags)
	journalfb.JournalRecordAddEntries(b, entriesVec)
	root := journalfb.JournalRecordEnd(b)
	b.Finish(root)

	return copy(scratch, b.FinishedBytes())
}

// DecodeChecksum reads via zero-copy accessors and folds every field in the
// canonical ChecksumRecord order. Zero allocation: scalars read in place, the
// Entry accessor is reused, CommandBytes returns a view into the buffer.
func (c *FBCodec) DecodeChecksum(frame []byte) uint64 {
	rec := journalfb.GetRootAsJournalRecord(frame, 0)
	ck := NewChecksum()
	ck.AddI64(rec.LeadershipTermId())
	ck.AddI64(rec.LogPosition())
	ck.AddI64(rec.Timestamp())
	ck.AddI64(rec.ClusterSessionId())
	ck.AddI64(rec.CorrelationId())
	ck.AddI32(rec.LeaderMemberId())
	ck.AddI32(rec.ServiceId())
	ck.AddU8(rec.EventType())
	ck.AddU8(rec.Flags())
	n := rec.EntriesLength()
	for i := 0; i < n; i++ {
		rec.Entries(&c.ent, i)
		ck.AddI64(c.ent.EntryTermId())
		ck.AddI64(c.ent.EntryIndex())
		ck.AddI64(c.ent.EntryTimestamp())
		ck.AddI32(c.ent.CommandKey())
		ck.AddBytes(c.ent.CommandBytes())
	}
	return ck.Finish()
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestFlatBuffers && gofmt -l internal/serjournal/flatbuffers.go`
Expected: all three tests PASS (round-trip, size band ~616, 0 allocs/op); gofmt clean.

- [ ] **Step 6: Write the cell main**

`go/cmd/serialization-flatbuffers/main.go`:

```go
// serialization-flatbuffers (Go): encode/decode cost of the ~500-byte journal
// record via Google FlatBuffers using the zero-copy read path (0 decode alloc).
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
)

const experiment = "flatbuffers"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	codec := serjournal.NewFBCodec()
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

Run: `cd go && go build ./... && go vet ./... && go test ./internal/serjournal/ && go mod tidy`
Then: `cd go && SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-flatbuffers | tee /dev/stderr | wc -l`
Expected: exactly 8 JSON lines — `encode_{p50,p99,mean}`, `decode_{p50,p99,mean}`, `encoded_bytes` (≈616), `decode_alloc_bytes` (**0**) — each `"focus_area":"serialization"`, `"language":"go"`, `"experiment":"flatbuffers"`. Confirm `go.mod` shows `github.com/google/flatbuffers v23.5.26+incompatible` (not bumped by tidy).

- [ ] **Step 8: Commit**

```bash
git add go/internal/serjournal/schema/journal.fbs go/internal/serjournal/regen-journalfb.sh \
        go/internal/serjournal/journalfb/ go/internal/serjournal/flatbuffers.go \
        go/internal/serjournal/flatbuffers_test.go go/cmd/serialization-flatbuffers/ \
        go/go.mod go/go.sum
git commit -m "feat(serialization): Go flatbuffers cell — zero-copy read (0 decode alloc)"
```

---

### Task 2: bench-infra matrix + docs

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: artifact name `serialization-flatbuffers` (Go).
- Produces: nothing downstream — run-matrix + docs only.

- [ ] **Step 1: Add the matrix row**

In `bench-infra/ansible/group_vars/all.yml`, add to the serialization block (after the existing Go serialization rows, e.g. after `protobuf`):

```yaml
  - { focus_area: serialization,    experiment: flatbuffers, kind: local, languages: [go] }
```

(The `ser_*` params are shared; no new params. `run_bench.sh` needs no change — `serialization` is already a known focus area and `go build ./cmd/...` picks up the new artifact.) If the serialization params comment above the block enumerates the cells/experiments, update its count to include `flatbuffers`.

- [ ] **Step 2: Update CLAUDE.md**

Two edits:
1. **Status paragraph** — extend the `serialization` sentence: Go also has a **FlatBuffers** cell (`flatbuffers`) using the zero-copy read path — 0 decode allocation, a larger wire (~616 B vs SBE's 502) but the fastest decode.
2. **Artifact-names line** — add `serialization-flatbuffers` to the Go artifact list. And add a Go run example: `go run ./cmd/serialization-flatbuffers`.

- [ ] **Step 3: Verify**

Run: `cd go && go build ./... && go vet ./... && go test ./...` (green) and
`python3 -c "import yaml; yaml.safe_load(open('bench-infra/ansible/group_vars/all.yml'))"` (parses).
Expected: both pass. No fabricated benchmark numbers; `journal/` and `RESULTS.md` untouched.

- [ ] **Step 4: Commit**

```bash
git add bench-infra/ansible/group_vars/all.yml CLAUDE.md
git commit -m "chore(serialization): register Go flatbuffers cell in bench-infra matrix + docs"
```
