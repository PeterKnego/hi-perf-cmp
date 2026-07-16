# Go bebop + protobuf Serialization Cells Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two Go-only experiments, `bebop` and `protobuf`, to the `serialization` focus area, benchmarking the same ~500-byte SMR journal record the Rust cells use.

**Architecture:** A Go port of the Rust journal write/replay harness lives in `go/internal/bench/serial.go` (generic over the record type, focus-neutral); the record model, deterministic builder, checksum fold, schemas, and generated codecs live in `go/internal/serjournal/`; two thin mains plug codec closures into the harness. Cross-language fairness is anchored by golden checksums generated from the Rust `serialization-common` implementation.

**Tech Stack:** Go 1.22 module, `google.golang.org/protobuf` v1.36.6 (+ `protoc` 3.21.x, `protoc-gen-go` v1.36.6 at regen time), `github.com/200sc/bebop` v0.6.2 (safe API only).

**Spec:** `docs/superpowers/specs/2026-07-16-serialization-go-design.md`

## Global Constraints

- stdout carries **only** result-contract JSON lines; diagnostics go to stderr (`bench.Logf`/`bench.Fatalf`).
- Metric names/units identical to the Rust cells: `encode_p50`, `encode_p99`, `encode_mean`, `decode_p50`, `decode_p99`, `decode_mean` (ns), `encoded_bytes` (bytes, samples=1), `decode_alloc_bytes` (bytes, samples=iters). Focus area `serialization`, experiments `bebop` / `protobuf`, language `go` (forced by `bench.Emit`).
- Env knobs: `SER_WARMUP` (default 1000), `SER_ITERS` (100000), `SER_ENTRIES` (4), `SER_CMD_BYTES` (78) — hard-error on malformed values via `positiveEnv`.
- Generated code is **committed**; regen scripts are dev-time only. Bench hosts need no protoc/generators.
- `internal/bench` must NOT import `internal/serjournal` (generic `RunJournal[R any]`).
- Benchmark the bebop **safe** API (`MarshalBebopTo`/`UnmarshalBebop`), never the unsafe fast path.
- `flags` is a **reserved bebop keyword** — the .bop field is `recordFlags` (generated Go field `RecordFlags`).
- Keep `cd go && go build ./... && go vet ./... && go test ./...` green after every task.
- Local runs are fitness checks only — never journaled, never `terraform apply`.

### Golden checksums (generated from Rust `serialization-common` on 2026-07-16)

For `(index, entries, cmd_bytes)` → `checksum_record(build_record(...))`:

```
(0,     4, 78) -> 0x7b8ca2b4f6f556d9
(1,     4, 78) -> 0x2ecb381439a319d6
(42,    4, 78) -> 0xe0e5b9514969d90d
(99999, 4, 78) -> 0xd19fa98130a517fe
(7,     2, 8)  -> 0x6d62ff2cced105df
```

Regenerate with a scratch crate depending on `serialization-common`:
`println!("{:016x}", checksum_record(&build_record(i, e, c)))`.

---

### Task 1: `serjournal` model, builder, checksum (golden-anchored)

**Files:**
- Create: `go/internal/serjournal/serjournal.go`
- Test: `go/internal/serjournal/serjournal_test.go`

**Interfaces:**
- Consumes: nothing (stdlib only).
- Produces (used by Tasks 3–5):
  - `type Entry struct { EntryTermID, EntryIndex, EntryTimestamp int64; CommandKey int32; Command []byte }`
  - `type Record struct { LeadershipTermID, LogPosition, Timestamp, ClusterSessionID, CorrelationID int64; LeaderMemberID, ServiceID int32; EventType, Flags uint8; Entries []Entry }`
  - `func BuildRecord(index uint64, entries, cmdBytes int) Record`
  - `type Checksum uint64`, `func NewChecksum() Checksum`, methods `AddI64(int64)`, `AddI32(int32)`, `AddU8(uint8)`, `AddBytes([]byte)`, `Finish() uint64` (Add* on `*Checksum`)
  - `func ChecksumRecord(r *Record) uint64`

- [ ] **Step 1: Write the failing test**

`go/internal/serjournal/serjournal_test.go`:

```go
package serjournal

import "testing"

// Golden values generated from the Rust serialization-common implementation
// (checksum_record(build_record(index, entries, cmdBytes))) on 2026-07-16.
// To regenerate: build a scratch crate depending on rust/serialization/common
// and print the checksums for the tuples below (see the implementation plan).
var golden = []struct {
	index            uint64
	entries, cmdBytes int
	want             uint64
}{
	{0, 4, 78, 0x7b8ca2b4f6f556d9},
	{1, 4, 78, 0x2ecb381439a319d6},
	{42, 4, 78, 0xe0e5b9514969d90d},
	{99999, 4, 78, 0xd19fa98130a517fe},
	{7, 2, 8, 0x6d62ff2cced105df},
}

func TestGoldenChecksumsMatchRust(t *testing.T) {
	for _, g := range golden {
		r := BuildRecord(g.index, g.entries, g.cmdBytes)
		if got := ChecksumRecord(&r); got != g.want {
			t.Errorf("(%d,%d,%d): got %#016x, want %#016x",
				g.index, g.entries, g.cmdBytes, got, g.want)
		}
	}
}

func TestBuildRecordIsDeterministic(t *testing.T) {
	a := BuildRecord(42, 4, 78)
	b := BuildRecord(42, 4, 78)
	if ChecksumRecord(&a) != ChecksumRecord(&b) {
		t.Fatal("same index produced different records")
	}
	if len(a.Entries) != 4 || len(a.Entries[0].Command) != 78 {
		t.Fatalf("unexpected shape: %d entries, %d command bytes",
			len(a.Entries), len(a.Entries[0].Command))
	}
}

func TestBuildRecordVariesByIndex(t *testing.T) {
	a := BuildRecord(1, 4, 78)
	b := BuildRecord(2, 4, 78)
	if ChecksumRecord(&a) == ChecksumRecord(&b) {
		t.Fatal("different indices produced identical records")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd go && go test ./internal/serjournal/`
Expected: FAIL — `undefined: BuildRecord` (package doesn't compile yet).

- [ ] **Step 3: Write the implementation**

`go/internal/serjournal/serjournal.go` — a direct port of
`rust/serialization/common/src/lib.rs` (Go's native wrapping arithmetic replaces
Rust's `wrapping_*`):

```go
// Package serjournal holds the shared logical model for the serialization
// focus area's Go cells: one ~500-byte SMR journal record, a deterministic
// index-seeded builder, and the canonical checksum every codec's decode must
// reproduce (the full-materialization proof). Ports rust/serialization/common;
// the golden test anchors the two implementations to identical records.
package serjournal

// Entry is one replicated command in the record's repeating group.
type Entry struct {
	EntryTermID    int64
	EntryIndex     int64
	EntryTimestamp int64
	CommandKey     int32
	Command        []byte
}

// Record mirrors Rust serialization-common's JournalRecord.
type Record struct {
	LeadershipTermID int64
	LogPosition      int64
	Timestamp        int64
	ClusterSessionID int64
	CorrelationID    int64
	LeaderMemberID   int32
	ServiceID        int32
	EventType        uint8
	Flags            uint8
	Entries          []Entry
}

// mix is one splitmix64 step — spreads field values from the record index so a
// record is byte-reproducible without RNG state or wall-clock input.
func mix(x uint64) uint64 {
	z := x + 0x9E3779B97F4A7C15
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// BuildRecord builds one journal record deterministically from index, with
// entries group members each carrying a cmdBytes-long command payload.
// Defaults of entries=4, cmdBytes=78 encode to ~500 bytes.
func BuildRecord(index uint64, entries, cmdBytes int) Record {
	h := mix(index)
	group := make([]Entry, 0, entries)
	for k := uint64(0); k < uint64(entries); k++ {
		e := mix(h ^ k*0x100000001B3)
		command := make([]byte, cmdBytes)
		for i := range command {
			command[i] = byte(e>>(i%8*8)) ^ byte(i)
		}
		group = append(group, Entry{
			EntryTermID:    int64(e),
			EntryIndex:     int64(index*uint64(entries) + k),
			EntryTimestamp: int64(mix(e)),
			CommandKey:     int32(e >> 32),
			Command:        command,
		})
	}
	return Record{
		LeadershipTermID: int64(h),
		LogPosition:      int64(index) << 8,
		Timestamp:        int64(mix(h)),
		ClusterSessionID: int64(h >> 16),
		CorrelationID:    int64(mix(h ^ 0xABCD)),
		LeaderMemberID:   int32(h >> 8),
		ServiceID:        int32(h >> 24),
		EventType:        uint8(h & 1), // 0 = APPEND, 1 = SNAPSHOT
		Flags:            uint8(h >> 1),
		Entries:          group,
	}
}

// Checksum is the order-sensitive FNV-style accumulator every codec folds the
// decoded fields into, in the same order; equal outputs prove identical
// materialization.
type Checksum uint64

// NewChecksum starts at the FNV-1a offset basis.
func NewChecksum() Checksum { return 0xcbf29ce484222325 }

func (c *Checksum) step(v uint64) { *c = Checksum((uint64(*c) ^ v) * 0x100000001B3) }

func (c *Checksum) AddI64(v int64) { c.step(uint64(v)) }

func (c *Checksum) AddI32(v int32) { c.step(uint64(uint32(v))) }

func (c *Checksum) AddU8(v uint8) { c.step(uint64(v)) }

func (c *Checksum) AddBytes(b []byte) {
	c.step(uint64(len(b)))
	for _, x := range b {
		c.step(uint64(x))
	}
}

func (c Checksum) Finish() uint64 { return uint64(c) }

// ChecksumRecord is the canonical fold over a fully-owned record. Codec decode
// paths fold the same order from their decoded representations.
func ChecksumRecord(r *Record) uint64 {
	c := NewChecksum()
	c.AddI64(r.LeadershipTermID)
	c.AddI64(r.LogPosition)
	c.AddI64(r.Timestamp)
	c.AddI64(r.ClusterSessionID)
	c.AddI64(r.CorrelationID)
	c.AddI32(r.LeaderMemberID)
	c.AddI32(r.ServiceID)
	c.AddU8(r.EventType)
	c.AddU8(r.Flags)
	for i := range r.Entries {
		e := &r.Entries[i]
		c.AddI64(e.EntryTermID)
		c.AddI64(e.EntryIndex)
		c.AddI64(e.EntryTimestamp)
		c.AddI32(e.CommandKey)
		c.AddBytes(e.Command)
	}
	return c.Finish()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd go && go test ./internal/serjournal/ && go vet ./internal/serjournal/`
Expected: PASS (golden test proves parity with Rust).

- [ ] **Step 5: Commit**

```bash
git add go/internal/serjournal/
git commit -m "feat(serialization): Go journal-record model + builder, golden-anchored to Rust"
```

---

### Task 2: `RunJournal` harness in `internal/bench`

**Files:**
- Create: `go/internal/bench/serial.go`
- Test: `go/internal/bench/serial_test.go`

**Interfaces:**
- Consumes: `positiveEnv` (`config.go`), `Emit`/`Result` (`result.go`), `Percentile`/`Mean` (`stats.go`).
- Produces (used by Task 5):
  - `type SerialConfig struct { Warmup, Iters, Entries, CmdBytes int }`
  - `func LoadSerialConfig() (SerialConfig, error)`
  - `func RunJournal[R any](experiment string, cfg SerialConfig, build func(uint64) R, encode func(R, []byte) int, decode func([]byte) uint64)`

- [ ] **Step 1: Write the failing test**

`go/internal/bench/serial_test.go`:

```go
package bench

import "testing"

func TestLoadSerialConfigDefaults(t *testing.T) {
	cfg, err := LoadSerialConfig()
	if err != nil {
		t.Fatalf("defaults errored: %v", err)
	}
	want := SerialConfig{Warmup: 1000, Iters: 100000, Entries: 4, CmdBytes: 78}
	if cfg != want {
		t.Fatalf("got %+v, want %+v", cfg, want)
	}
}

func TestLoadSerialConfigOverrides(t *testing.T) {
	t.Setenv("SER_WARMUP", "10")
	t.Setenv("SER_ITERS", "200")
	t.Setenv("SER_ENTRIES", "2")
	t.Setenv("SER_CMD_BYTES", "8")
	cfg, err := LoadSerialConfig()
	if err != nil {
		t.Fatalf("overrides errored: %v", err)
	}
	want := SerialConfig{Warmup: 10, Iters: 200, Entries: 2, CmdBytes: 8}
	if cfg != want {
		t.Fatalf("got %+v, want %+v", cfg, want)
	}
}

func TestLoadSerialConfigRejectsMalformed(t *testing.T) {
	t.Setenv("SER_ITERS", "not-a-number")
	if _, err := LoadSerialConfig(); err == nil {
		t.Fatal("malformed SER_ITERS did not error")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd go && go test ./internal/bench/ -run TestLoadSerialConfig`
Expected: FAIL — `undefined: LoadSerialConfig`.

- [ ] **Step 3: Write the implementation**

`go/internal/bench/serial.go` — a port of `rust/bench-common/src/serial.rs`'s
`run_journal` (the counting allocator becomes a `runtime.MemStats.TotalAlloc`
delta — cumulative and monotonic, so GC cannot deflate it; read exactly twice,
outside all timed regions):

```go
package bench

import (
	"runtime"
	"sort"
	"time"
)

const serFocusArea = "serialization"

// serSink absorbs the decode checksums so the fold cannot be elided.
var serSink uint64

// SerialConfig configures the serialization journal benchmark, sourced from
// SER_* env vars (same names and defaults as the Rust cells; defaults encode
// to a ~500-byte record).
type SerialConfig struct {
	Warmup   int
	Iters    int
	Entries  int
	CmdBytes int
}

// LoadSerialConfig reads and validates the SER_* environment.
func LoadSerialConfig() (SerialConfig, error) {
	warmup, err := positiveEnv("SER_WARMUP", 1000)
	if err != nil {
		return SerialConfig{}, err
	}
	iters, err := positiveEnv("SER_ITERS", 100000)
	if err != nil {
		return SerialConfig{}, err
	}
	entries, err := positiveEnv("SER_ENTRIES", 4)
	if err != nil {
		return SerialConfig{}, err
	}
	cmdBytes, err := positiveEnv("SER_CMD_BYTES", 78)
	if err != nil {
		return SerialConfig{}, err
	}
	return SerialConfig{Warmup: warmup, Iters: iters, Entries: entries, CmdBytes: cmdBytes}, nil
}

// RunJournal drives the journal write/replay loop and emits the eight
// result-contract metrics. build(index) produces one codec-native record
// deterministically (pre-built, untimed — conversion from the logical model is
// not part of the measurement); encode(record, scratch) serializes into the
// reused scratch buffer and returns the encoded length; decode(bytes) decodes
// and fully materializes every field into a checksum, so owned-decode codecs
// and any future lazy codec pay for the same reads.
//
// Generic over the record type R so this package stays focus-neutral and never
// imports a focus area's model package.
func RunJournal[R any](experiment string, cfg SerialConfig, build func(uint64) R, encode func(R, []byte) int, decode func([]byte) uint64) {
	n := cfg.Iters

	// Pre-build all records (untimed); building is deterministic from index.
	records := make([]R, cfg.Warmup+n)
	for i := range records {
		records[i] = build(uint64(i))
	}

	scratch := make([]byte, 64*1024)
	encodeNs := make([]int64, 0, n)
	recordLen := 0

	// Warmup encode.
	for _, r := range records[:cfg.Warmup] {
		recordLen = encode(r, scratch)
	}
	// Timed encode.
	for _, r := range records[cfg.Warmup:] {
		t0 := time.Now()
		l := encode(r, scratch)
		dt := time.Since(t0).Nanoseconds()
		serSink ^= uint64(scratch[0])
		recordLen = l
		encodeNs = append(encodeNs, dt)
	}

	// Build the contiguous in-memory journal from the timed records.
	type frame struct{ off, len int }
	journal := make([]byte, 0, recordLen*n+64)
	frames := make([]frame, 0, n)
	for _, r := range records[cfg.Warmup:] {
		start := len(journal)
		l := encode(r, scratch)
		journal = append(journal, scratch[:l]...)
		frames = append(frames, frame{start, l})
	}

	decodeNs := make([]int64, 0, n)
	var sink uint64

	// Warmup decode.
	warm := cfg.Warmup
	if warm > len(frames) {
		warm = len(frames)
	}
	for _, f := range frames[:warm] {
		sink ^= decode(journal[f.off : f.off+f.len])
	}

	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)
	for _, f := range frames {
		t0 := time.Now()
		sum := decode(journal[f.off : f.off+f.len])
		dt := time.Since(t0).Nanoseconds()
		sink ^= sum
		decodeNs = append(decodeNs, dt)
	}
	runtime.ReadMemStats(&after)
	serSink ^= sink

	decodeAllocPer := (after.TotalAlloc - before.TotalAlloc) / uint64(n)

	emitSerLatency(experiment, "encode", encodeNs)
	emitSerLatency(experiment, "decode", decodeNs)
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: "encoded_bytes",
		Value: float64(recordLen), Unit: "bytes", Samples: 1})
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: "decode_alloc_bytes",
		Value: float64(decodeAllocPer), Unit: "bytes", Samples: int64(n)})
}

// emitSerLatency sorts samples and emits {op}_p50/p99/mean (ns), mirroring
// EmitSmrLatency but for the serialization focus area.
func emitSerLatency(experiment, op string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: op + "_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: op + "_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: op + "_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd go && go test ./internal/bench/ && go vet ./internal/bench/`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add go/internal/bench/serial.go go/internal/bench/serial_test.go
git commit -m "feat(serialization): Go RunJournal harness — SER_* config, timed encode/replay, TotalAlloc delta"
```

---

### Task 3: bebop schema, codegen, codec adapter

**Files:**
- Create: `go/internal/serjournal/schema/journal.bop`
- Create: `go/internal/serjournal/regen-journalbop.sh` (mode 755)
- Create: `go/internal/serjournal/journalbop/journal.go` (generated, committed)
- Create: `go/internal/serjournal/bebop.go`
- Test: `go/internal/serjournal/bebop_test.go`
- Modify: `go/go.mod` (+ `github.com/200sc/bebop v0.6.2`)

**Interfaces:**
- Consumes: Task 1's `Record`, `NewChecksum`, `ChecksumRecord`.
- Produces (used by Task 5):
  - `func ToBebop(r *Record) journalbop.JournalRecord`
  - `func EncodeBebop(r journalbop.JournalRecord, scratch []byte) int`
  - `func DecodeBebopChecksum(buf []byte) uint64`
- Generated API (200sc/bebop v0.6.2, verified): `MarshalBebopTo(buf []byte) int` (value receiver), `UnmarshalBebop(buf []byte) error` (pointer receiver), `Size() int`; struct fields `LeadershipTermId`, …, `EventType byte`, `RecordFlags byte`, `Entries []Entry`, `Command []byte`.

- [ ] **Step 1: Write the schema**

`go/internal/serjournal/schema/journal.bop` — bebop `struct`s (fixed layout,
not `message`); `flags` is reserved in bebop, hence `recordFlags`:

```
// The serialization focus area's JournalRecord (see the 2026-07-16 design
// spec). Bebop structs are fixed-layout; `flags` is a reserved bebop
// keyword, so the field is named recordFlags.

struct Entry {
    int64 entryTermId;
    int64 entryIndex;
    int64 entryTimestamp;
    int32 commandKey;
    byte[] command;
}

struct JournalRecord {
    int64 leadershipTermId;
    int64 logPosition;
    int64 timestamp;
    int64 clusterSessionId;
    int64 correlationId;
    int32 leaderMemberId;
    int32 serviceId;
    byte eventType;
    byte recordFlags;
    Entry[] entries;
}
```

- [ ] **Step 2: Write the regen script and add the dependency**

`go/internal/serjournal/regen-journalbop.sh`:

```sh
#!/bin/sh
# Regenerate journalbop/ from schema/journal.bop with the 200sc/bebop
# generator at the version pinned in go.mod. Dev-time only; the output is
# committed so bench hosts need no generator.
set -eu
cd "$(dirname "$0")"
mkdir -p journalbop
go run github.com/200sc/bebop/main/bebopc-go \
    -i schema/journal.bop -o journalbop/journal.go -package journalbop
gofmt -w journalbop/journal.go
```

```bash
chmod +x go/internal/serjournal/regen-journalbop.sh
cd go && go get github.com/200sc/bebop@v0.6.2
```

- [ ] **Step 3: Generate and commit the codec**

```bash
cd go && ./internal/serjournal/regen-journalbop.sh && go mod tidy && go build ./...
```

Expected: `journalbop/journal.go` appears with `package journalbop`, imports
`github.com/200sc/bebop` + `github.com/200sc/bebop/iohelp`, and the API listed
in Interfaces above. Build passes.

- [ ] **Step 4: Write the failing adapter test**

`go/internal/serjournal/bebop_test.go`:

```go
package serjournal

import "testing"

func TestBebopRoundTripChecksum(t *testing.T) {
	scratch := make([]byte, 64*1024)
	for _, index := range []uint64{0, 1, 42} {
		r := BuildRecord(index, 4, 78)
		n := EncodeBebop(ToBebop(&r), scratch)
		if got, want := DecodeBebopChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
			t.Errorf("index %d: decode checksum %#x, direct fold %#x", index, got, want)
		}
	}
}

func TestBebopEncodedSizeBand(t *testing.T) {
	r := BuildRecord(0, 4, 78)
	scratch := make([]byte, 64*1024)
	n := EncodeBebop(ToBebop(&r), scratch)
	// ~500-byte target; loose band allows per-codec framing differences.
	if n < 450 || n > 570 {
		t.Fatalf("encoded size %d outside [450, 570]", n)
	}
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestBebop`
Expected: FAIL — `undefined: EncodeBebop`.

- [ ] **Step 5: Write the adapter**

`go/internal/serjournal/bebop.go`:

```go
package serjournal

import "github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalbop"

// ToBebop converts the logical record to the generated bebop representation.
// Command slices are shared, not copied — encode only reads them. Conversion
// happens in the harness's untimed pre-build phase.
func ToBebop(r *Record) journalbop.JournalRecord {
	entries := make([]journalbop.Entry, len(r.Entries))
	for i := range r.Entries {
		e := &r.Entries[i]
		entries[i] = journalbop.Entry{
			EntryTermId:    e.EntryTermID,
			EntryIndex:     e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp,
			CommandKey:     e.CommandKey,
			Command:        e.Command,
		}
	}
	return journalbop.JournalRecord{
		LeadershipTermId: r.LeadershipTermID,
		LogPosition:      r.LogPosition,
		Timestamp:        r.Timestamp,
		ClusterSessionId: r.ClusterSessionID,
		CorrelationId:    r.CorrelationID,
		LeaderMemberId:   r.LeaderMemberID,
		ServiceId:        r.ServiceID,
		EventType:        r.EventType,
		RecordFlags:      r.Flags,
		Entries:          entries,
	}
}

// EncodeBebop serializes via the safe MarshalBebopTo into the reused scratch
// buffer (the unsafe fast path is deliberately not benchmarked).
func EncodeBebop(r journalbop.JournalRecord, scratch []byte) int {
	return r.MarshalBebopTo(scratch)
}

// DecodeBebopChecksum decodes (owned, allocating — the story this cell tells)
// and folds every field in the canonical checksum order.
func DecodeBebopChecksum(buf []byte) uint64 {
	var d journalbop.JournalRecord
	if err := d.UnmarshalBebop(buf); err != nil {
		panic("serjournal: bebop decode failed on harness-encoded bytes: " + err.Error())
	}
	c := NewChecksum()
	c.AddI64(d.LeadershipTermId)
	c.AddI64(d.LogPosition)
	c.AddI64(d.Timestamp)
	c.AddI64(d.ClusterSessionId)
	c.AddI64(d.CorrelationId)
	c.AddI32(d.LeaderMemberId)
	c.AddI32(d.ServiceId)
	c.AddU8(d.EventType)
	c.AddU8(d.RecordFlags)
	for i := range d.Entries {
		e := &d.Entries[i]
		c.AddI64(e.EntryTermId)
		c.AddI64(e.EntryIndex)
		c.AddI64(e.EntryTimestamp)
		c.AddI32(e.CommandKey)
		c.AddBytes(e.Command)
	}
	return c.Finish()
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd go && go build ./... && go vet ./... && go test ./internal/serjournal/`
Expected: PASS; bebop encoded size is 494 bytes at defaults (inside the band).

- [ ] **Step 7: Commit**

```bash
git add go/internal/serjournal/ go/go.mod go/go.sum
git commit -m "feat(serialization): bebop schema + committed 200sc/bebop codegen + codec adapter"
```

---

### Task 4: protobuf schema, codegen, codec adapter

**Files:**
- Create: `go/internal/serjournal/schema/journal.proto`
- Create: `go/internal/serjournal/regen-journalpb.sh` (mode 755)
- Create: `go/internal/serjournal/journalpb/journal.pb.go` (generated, committed)
- Create: `go/internal/serjournal/proto.go`
- Test: `go/internal/serjournal/proto_test.go`
- Modify: `go/go.mod` (+ `google.golang.org/protobuf v1.36.6`)

**Interfaces:**
- Consumes: Task 1's `Record`, `NewChecksum`, `ChecksumRecord`.
- Produces (used by Task 5):
  - `func ToProto(r *Record) *journalpb.JournalRecord`
  - `func EncodeProto(r *journalpb.JournalRecord, scratch []byte) int`
  - `func DecodeProtoChecksum(buf []byte) uint64`

- [ ] **Step 1: Write the schema**

`go/internal/serjournal/schema/journal.proto` — fixed-width `sfixed64`/`sfixed32`
for the full-width id/timestamp fields (design decision 2026-07-16: Aeron-style
ids are mixed bit patterns; varints would measure encoding pathology, not the
codec); `uint32` for the two byte-wide fields (proto3 has no 8-bit type):

```proto
// The serialization focus area's JournalRecord (see the 2026-07-16 design
// spec). sfixed64/sfixed32 by design: the ids/timestamps are full-width
// mixed bit patterns, where default varints would encode ~10 bytes/field.
syntax = "proto3";
package hiperf.journal;

option go_package = "github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalpb";

message Entry {
  sfixed64 entry_term_id = 1;
  sfixed64 entry_index = 2;
  sfixed64 entry_timestamp = 3;
  sfixed32 command_key = 4;
  bytes command = 5;
}

message JournalRecord {
  sfixed64 leadership_term_id = 1;
  sfixed64 log_position = 2;
  sfixed64 timestamp = 3;
  sfixed64 cluster_session_id = 4;
  sfixed64 correlation_id = 5;
  sfixed32 leader_member_id = 6;
  sfixed32 service_id = 7;
  uint32 event_type = 8;
  uint32 flags = 9;
  repeated Entry entries = 10;
}
```

- [ ] **Step 2: Write the regen script and add the dependency**

`go/internal/serjournal/regen-journalpb.sh`:

```sh
#!/bin/sh
# Regenerate journalpb/ from schema/journal.proto. Requires protoc (3.21+) on
# PATH; protoc-gen-go is version-pinned and installed to a temp dir. The
# protoc and plugin versions used are recorded in the generated file header.
# Dev-time only; the output is committed so bench hosts need no protoc.
set -eu
cd "$(dirname "$0")"
PLUGIN_DIR="$(mktemp -d)"
trap 'rm -rf "$PLUGIN_DIR"' EXIT
GOBIN="$PLUGIN_DIR" go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.6
PATH="$PLUGIN_DIR:$PATH" protoc \
    --go_out=. \
    --go_opt=module=github.com/peterknego/hi-perf-cmp/go/internal/serjournal \
    schema/journal.proto
```

(The `module=` option maps the `go_package` path onto this directory, so the
output lands at `journalpb/journal.pb.go`.)

```bash
chmod +x go/internal/serjournal/regen-journalpb.sh
cd go && go get google.golang.org/protobuf@v1.36.6
```

- [ ] **Step 3: Generate and commit the codec**

```bash
cd go && ./internal/serjournal/regen-journalpb.sh && go mod tidy && go build ./...
```

Expected: `journalpb/journal.pb.go` appears with `package journalpb`, message
structs with `LeadershipTermId int64`, `EventType uint32`, `Flags uint32`,
`Entries []*Entry`, `Command []byte`. Build passes.

- [ ] **Step 4: Write the failing adapter test**

`go/internal/serjournal/proto_test.go`:

```go
package serjournal

import "testing"

func TestProtoRoundTripChecksum(t *testing.T) {
	scratch := make([]byte, 64*1024)
	for _, index := range []uint64{0, 1, 42} {
		r := BuildRecord(index, 4, 78)
		n := EncodeProto(ToProto(&r), scratch)
		if got, want := DecodeProtoChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
			t.Errorf("index %d: decode checksum %#x, direct fold %#x", index, got, want)
		}
	}
}

func TestProtoEncodedSizeBand(t *testing.T) {
	r := BuildRecord(0, 4, 78)
	scratch := make([]byte, 64*1024)
	n := EncodeProto(ToProto(&r), scratch)
	// ~500-byte target; loose band allows per-codec framing differences.
	if n < 450 || n > 570 {
		t.Fatalf("encoded size %d outside [450, 570]", n)
	}
}
```

Run: `cd go && go test ./internal/serjournal/ -run TestProto`
Expected: FAIL — `undefined: EncodeProto`.

- [ ] **Step 5: Write the adapter**

`go/internal/serjournal/proto.go`:

```go
package serjournal

import (
	"google.golang.org/protobuf/proto"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalpb"
)

// ToProto converts the logical record to the generated protobuf
// representation. Command slices are shared, not copied — encode only reads
// them. Conversion happens in the harness's untimed pre-build phase.
func ToProto(r *Record) *journalpb.JournalRecord {
	entries := make([]*journalpb.Entry, len(r.Entries))
	for i := range r.Entries {
		e := &r.Entries[i]
		entries[i] = &journalpb.Entry{
			EntryTermId:    e.EntryTermID,
			EntryIndex:     e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp,
			CommandKey:     e.CommandKey,
			Command:        e.Command,
		}
	}
	return &journalpb.JournalRecord{
		LeadershipTermId: r.LeadershipTermID,
		LogPosition:      r.LogPosition,
		Timestamp:        r.Timestamp,
		ClusterSessionId: r.ClusterSessionID,
		CorrelationId:    r.CorrelationID,
		LeaderMemberId:   r.LeaderMemberID,
		ServiceId:        r.ServiceID,
		EventType:        uint32(r.EventType),
		Flags:            uint32(r.Flags),
		Entries:          entries,
	}
}

var protoMarshalOpts = proto.MarshalOptions{}

// EncodeProto serializes into the reused scratch buffer via MarshalAppend.
// The record (~516 B) never outgrows the 64 KiB scratch, so no reallocation
// happens inside the timed region; the guard makes a violation loud instead
// of silently corrupting the journal buffer.
func EncodeProto(r *journalpb.JournalRecord, scratch []byte) int {
	out, err := protoMarshalOpts.MarshalAppend(scratch[:0], r)
	if err != nil {
		panic("serjournal: proto encode failed: " + err.Error())
	}
	if len(out) > 0 && &out[0] != &scratch[0] {
		panic("serjournal: scratch buffer too small for encoded record")
	}
	return len(out)
}

// DecodeProtoChecksum decodes (owned, allocating — the story this cell tells)
// and folds every field in the canonical checksum order.
func DecodeProtoChecksum(buf []byte) uint64 {
	var d journalpb.JournalRecord
	if err := proto.Unmarshal(buf, &d); err != nil {
		panic("serjournal: proto decode failed on harness-encoded bytes: " + err.Error())
	}
	c := NewChecksum()
	c.AddI64(d.LeadershipTermId)
	c.AddI64(d.LogPosition)
	c.AddI64(d.Timestamp)
	c.AddI64(d.ClusterSessionId)
	c.AddI64(d.CorrelationId)
	c.AddI32(d.LeaderMemberId)
	c.AddI32(d.ServiceId)
	c.AddU8(uint8(d.EventType))
	c.AddU8(uint8(d.Flags))
	for _, e := range d.Entries {
		c.AddI64(e.EntryTermId)
		c.AddI64(e.EntryIndex)
		c.AddI64(e.EntryTimestamp)
		c.AddI32(e.CommandKey)
		c.AddBytes(e.Command)
	}
	return c.Finish()
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd go && go build ./... && go vet ./... && go test ./internal/serjournal/`
Expected: PASS; protobuf encoded size is 516 bytes at defaults (inside the band).

- [ ] **Step 7: Commit**

```bash
git add go/internal/serjournal/ go/go.mod go/go.sum
git commit -m "feat(serialization): protobuf schema (sfixed64) + committed protoc-gen-go codegen + codec adapter"
```

---

### Task 5: the two benchmark mains + local smoke run

**Files:**
- Create: `go/cmd/serialization-bebop/main.go`
- Create: `go/cmd/serialization-protobuf/main.go`

**Interfaces:**
- Consumes: `bench.LoadSerialConfig`, `bench.RunJournal`, `bench.Fatalf` (Task 2); `serjournal.BuildRecord`, `ToBebop`/`EncodeBebop`/`DecodeBebopChecksum` (Task 3), `ToProto`/`EncodeProto`/`DecodeProtoChecksum` (Task 4).
- Produces: the runnable artifacts `serialization-bebop`, `serialization-protobuf`.

- [ ] **Step 1: Write the bebop main**

`go/cmd/serialization-bebop/main.go`:

```go
// serialization-bebop (Go): encode/decode cost of the ~500-byte journal
// record via the 200sc/bebop safe API.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalbop"
)

const experiment = "bebop"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	bench.RunJournal(experiment, cfg,
		func(i uint64) journalbop.JournalRecord {
			r := serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
			return serjournal.ToBebop(&r)
		},
		serjournal.EncodeBebop,
		serjournal.DecodeBebopChecksum,
	)
}
```

- [ ] **Step 2: Write the protobuf main**

`go/cmd/serialization-protobuf/main.go`:

```go
// serialization-protobuf (Go): encode/decode cost of the ~500-byte journal
// record via the canonical google.golang.org/protobuf runtime.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalpb"
)

const experiment = "protobuf"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	bench.RunJournal(experiment, cfg,
		func(i uint64) *journalpb.JournalRecord {
			r := serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
			return serjournal.ToProto(&r)
		},
		serjournal.EncodeProto,
		serjournal.DecodeProtoChecksum,
	)
}
```

- [ ] **Step 3: Build, vet, test everything**

Run: `cd go && go build ./... && go vet ./... && go test ./...`
Expected: all green.

- [ ] **Step 4: Local smoke run (fitness check only — never journaled)**

```bash
cd go
SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-bebop
SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-protobuf
```

Expected: each prints exactly **8 JSON lines** to stdout and nothing else —
metrics `encode_p50`, `encode_p99`, `encode_mean`, `decode_p50`, `decode_p99`,
`decode_mean`, `encoded_bytes` (494 bebop / 516 protobuf), `decode_alloc_bytes`
(> 0 for both — owned decode), each with `"focus_area":"serialization"`,
`"language":"go"`, the right `"experiment"`. Verify line count and validity:

```bash
SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-bebop | tee /dev/stderr | wc -l   # → 8
SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-protobuf | python3 -c 'import json,sys; [json.loads(l) for l in sys.stdin]; print("valid JSON")'
```

- [ ] **Step 5: Commit**

```bash
git add go/cmd/serialization-bebop/ go/cmd/serialization-protobuf/
git commit -m "feat(serialization): Go bebop + protobuf benchmark mains"
```

---

### Task 6: bench-infra matrix rows + docs

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml` (experiments list ~line 20–26; `ser_*` params comment ~line 52)
- Modify: `CLAUDE.md` (status paragraph, artifact-name list, Go run examples)
- Modify: `README.md` and `docs/result-contract.md` wherever serialization is described as Rust-only or experiments are enumerated (grep first)

**Interfaces:**
- Consumes: the artifact names `serialization-bebop`, `serialization-protobuf` (Task 5).
- Produces: nothing downstream — documentation and run-matrix only.

- [ ] **Step 1: Add the matrix rows and fix the Rust-only comments**

In `bench-infra/ansible/group_vars/all.yml`, after the `serialization` rows:

```yaml
  - { focus_area: serialization,    experiment: bebop,       kind: local, languages: [go] }
  - { focus_area: serialization,    experiment: protobuf,    kind: local, languages: [go] }
```

Update the comment above the serialization rows (currently "serialization is
Rust-only: the optional `languages` key restricts which…") to say the focus
area is Rust + Go, with each row's `languages` key naming its language. Update
the `ser_*` params comment block ("Rust-only focus area: three codecs…") to
mention five experiments across Rust (sbe_gen, aeron_sbe, bincode) and Go
(bebop, protobuf); the `ser_*` params are shared by all five. Keep the "NO JDK
needed at bench time" note — it still holds, and the Go cells' generated code
is committed too.

- [ ] **Step 2: Update CLAUDE.md**

Three places:

1. **Status paragraph**: change "`serialization` is implemented in Rust only
   (three codecs — …) — Go/Java are not planned for this focus area" to say it
   is implemented in Rust (`sbe_gen` zerocopy SBE, `aeron_sbe` real-logic
   SBE-tool Rust output, `bincode` serde+bincode) **and Go** (`bebop` via
   200sc/bebop safe API, `protobuf` via the canonical google.golang.org/protobuf
   runtime), single-host, measuring encode/decode latency + decode allocation;
   Java is not planned.
2. **Artifact names line**: `serialization-{sbe_gen,aeron_sbe,bincode}` (Rust
   only) becomes `serialization-{sbe_gen,aeron_sbe,bincode}` (Rust) and
   `serialization-{bebop,protobuf}` (Go).
3. **Build & run, Go section**: add `go run ./cmd/serialization-protobuf` (or
   `-bebop`) as an example line.

- [ ] **Step 3: Sweep the remaining docs**

```bash
grep -rn "Rust only\|Rust-only\|sbe_gen" README.md docs/result-contract.md docs/RESULTS.md
```

Update any hit that enumerates serialization experiments or calls the focus
area Rust-only so it lists the Go `bebop`/`protobuf` cells too. Do NOT touch
`journal/` — journal rows appear only after a real AWS run.

- [ ] **Step 4: Verify nothing broke**

Run: `cd go && go build ./... && go vet ./... && go test ./...`
Expected: green (docs/infra changes shouldn't affect it; this is the final gate).

- [ ] **Step 5: Commit**

```bash
git add bench-infra/ansible/group_vars/all.yml CLAUDE.md README.md docs/
git commit -m "chore(serialization): register Go bebop/protobuf cells in bench-infra matrix + docs"
```
