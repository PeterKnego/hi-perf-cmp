# Serialization typed-command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the opaque `command []byte` on each journal-record entry with a typed command (`cmdQty` int64, `cmdPrice` float64, `cmdFlag` bool, `cmdText` string), in place across all eight serialization codecs, so the record exercises field-encoding instead of a memcpy.

**Architecture:** Change the two shared logical models (Rust `serialization-common`, Go `serjournal`) and their deterministic builder + checksum first, then propagate the field change through each codec's schema and adapter. SBE's four cells regenerate from one shared `journal.xml` (the string is SBE var-data bytes); bebop/protobuf/flatbuffers/bincode gain native `int64/double/bool/string` fields. Cross-language golden checksums and the SBE byte-identity golden are regenerated.

**Tech Stack:** Rust (sbe_gen 0.7.3, real-logic sbe-tool via vendored jar, serde+bincode), Go (200sc/bebop, google.golang.org/protobuf, google.golang.org/flatbuffers, SBE-Golang flyweight/struct). No new dependencies.

**Spec:** `docs/superpowers/specs/2026-07-22-serialization-typed-command-design.md`

## Global Constraints

- Each `Entry` **drops `command []byte`** and gains `cmdQty` int64, `cmdPrice` float64, `cmdFlag` bool, `cmdText` string. All other record/entry fields unchanged.
- The change is **in place**: same eight cells, same experiment names, same focus area, same metric set. No new cell/codec/focus area. `encoded_bytes` shrinks (~500 → ~300 B).
- SBE has no bool/string: `cmdFlag` → `uint8` 0/1; `cmdText` → var-data (uint8 bytes = the string's UTF-8 bytes). The adapter converts at the boundary. All four SBE cells consume the one shared `journal.xml`; the entries group `blockLength` becomes **45** (28 + int64 8 + double 8 + uint8 1).
- The builder is deterministic (no RNG/clock); every new field derives from the existing splitmix64 seed so the Rust and Go builders stay byte/field-identical (golden-checksum anchor). `cmdText` is **printable ASCII** (valid UTF-8). `cmdPrice` is a **finite** double.
- `SER_CMD_BYTES` is repurposed from the blob length to the `cmdText` length (default unchanged).
- Keep `cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check` and `cd go && go build ./... && go vet ./... && go test ./...` green; hand-written files gofmt/rustfmt-clean (generated code formatted by regen scripts).
- Generated codec code is committed; regen scripts are dev-time only. No journaling/AWS from this plan.

### Locked builder formula (identical Rust & Go)

Per entry, with `e = mix(h ^ k*0x100000001B3)` (unchanged), replacing the old `command` fill:

```
cmd_qty   = mix(e) as i64
cmd_price = (mix(e ^ 0xF0) >> 11) as f64 * 3.0517578125e-5   // finite normal double
cmd_flag  = (mix(e ^ 0x0F) & 1) == 1
cmd_text[i] = 0x20 + byte(mix(e ^ 0xAA) >> (i%8*8)) % 95     // printable ASCII, length = text_len (SER_CMD_BYTES)
```

`mix(x)` is the existing splitmix64 step. The entry's own fields (`entry_term_id`, `entry_index`, `entry_timestamp`, `command_key`) are unchanged.

### Checksum fold order (identical Rust & Go)

Header fields unchanged. Per entry, after `command_key` (i32), fold: `cmd_qty` (i64), `cmd_price` (raw IEEE-754 bits via `to_bits`/`Float64bits`), `cmd_flag` (as u8 0/1), `cmd_text` (length then bytes — the existing bytes rule). New checksum methods: Rust `add_f64(f64)`, `add_bool(bool)`, `add_str(&str)`; Go `AddF64(float64)`, `AddBool(bool)`, `AddString(string)`.

### Golden checksums (generated from the new Rust builder, 2026-07-22)

For `(index, entries, text_len)`:

```
(0,     4, 78) -> 0x86d721cbffdefc06
(1,     4, 78) -> 0xddb1bfa73e9819cb
(42,    4, 78) -> 0x495a0d763cc820ca
(99999, 4, 78) -> 0x552b92436dae830e
(7,     2, 8)  -> 0x9b525460dd070517
```

### Encoded-size guidance

The record shrinks to roughly ~280–320 B at the default config (was ~500). Size-band tests should be re-centered (e.g. `[240, 420]`) rather than tightened to a specific number, since each codec's field encoding differs; each task states the measured value to confirm.

---

### Task 1: Rust shared model — `serialization-common`

**Files:**
- Modify: `rust/serialization/common/src/lib.rs`

**Interfaces:**
- Produces (consumed by Tasks 2–3): `Entry { entry_term_id: i64, entry_index: i64, entry_timestamp: i64, command_key: i32, cmd_qty: i64, cmd_price: f64, cmd_flag: bool, cmd_text: String }`; `build_record(index: u64, entries: usize, text_len: usize) -> JournalRecord`; `checksum_record(&JournalRecord) -> u64`; `Checksum` with `add_i64/add_i32/add_u8/add_bytes` plus new `add_f64(f64)`, `add_bool(bool)`, `add_str(&str)`.

- [ ] **Step 1: Update the `Entry` struct**

In `rust/serialization/common/src/lib.rs`, replace the `command: Vec<u8>` field:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub entry_term_id: i64,
    pub entry_index: i64,
    pub entry_timestamp: i64,
    pub command_key: i32,
    pub cmd_qty: i64,
    pub cmd_price: f64,
    pub cmd_flag: bool,
    pub cmd_text: String,
}
```

(If `JournalRecord`/`Entry` derive `Serialize/Deserialize` for the bincode mirror, keep those derives — Task 3 depends on them.)

- [ ] **Step 2: Update `build_record`**

Replace the per-entry command fill and the `Entry` construction:

```rust
pub fn build_record(index: u64, entries: usize, text_len: usize) -> JournalRecord {
    let h = mix(index);
    let mut group = Vec::with_capacity(entries);
    for k in 0..entries as u64 {
        let e = mix(h ^ k.wrapping_mul(0x0100_0000_01B3));
        let cmd_qty = mix(e) as i64;
        let cmd_price = (mix(e ^ 0xF0) >> 11) as f64 * 3.0517578125e-5;
        let cmd_flag = (mix(e ^ 0x0F) & 1) == 1;
        let t = mix(e ^ 0xAA);
        let mut cmd_text = String::with_capacity(text_len);
        for i in 0..text_len {
            cmd_text.push((0x20u8 + (t >> (i % 8 * 8)) as u8 % 95) as char);
        }
        group.push(Entry {
            entry_term_id: e as i64,
            entry_index: (index * entries as u64 + k) as i64,
            entry_timestamp: mix(e) as i64,
            command_key: (e >> 32) as i32,
            cmd_qty,
            cmd_price,
            cmd_flag,
            cmd_text,
        });
    }
    JournalRecord {
        leadership_term_id: h as i64,
        log_position: (index as i64) << 8,
        timestamp: mix(h) as i64,
        cluster_session_id: (h >> 16) as i64,
        correlation_id: mix(h ^ 0xABCD) as i64,
        leader_member_id: (h >> 8) as i32,
        service_id: (h >> 24) as i32,
        event_type: (h & 1) as u8,
        flags: (h >> 1) as u8,
        entries: group,
    }
}
```

- [ ] **Step 3: Add the checksum methods and update `checksum_record`**

Add to `impl Checksum` (after `add_bytes`):

```rust
    #[inline]
    pub fn add_f64(&mut self, v: f64) {
        self.step(v.to_bits());
    }
    #[inline]
    pub fn add_bool(&mut self, v: bool) {
        self.step(v as u64);
    }
    #[inline]
    pub fn add_str(&mut self, s: &str) {
        let b = s.as_bytes();
        self.step(b.len() as u64);
        for &x in b {
            self.step(x as u64);
        }
    }
```

Update the per-entry fold in `checksum_record`:

```rust
    for e in &r.entries {
        c.add_i64(e.entry_term_id);
        c.add_i64(e.entry_index);
        c.add_i64(e.entry_timestamp);
        c.add_i32(e.command_key);
        c.add_i64(e.cmd_qty);
        c.add_f64(e.cmd_price);
        c.add_bool(e.cmd_flag);
        c.add_str(&e.cmd_text);
    }
```

- [ ] **Step 4: Update the golden test**

Replace the golden values in the `#[cfg(test)]` module (the build-determinism test's `command.len()` assertion becomes `cmd_text.len()`):

```rust
    #[test]
    fn golden_checksums() {
        assert_eq!(checksum_record(&build_record(0, 4, 78)), 0x86d7_21cb_ffde_fc06);
        assert_eq!(checksum_record(&build_record(1, 4, 78)), 0xddb1_bfa7_3e98_19cb);
        assert_eq!(checksum_record(&build_record(42, 4, 78)), 0x495a_0d76_3cc8_20ca);
        assert_eq!(checksum_record(&build_record(99999, 4, 78)), 0x552b_9243_6dae_830e);
        assert_eq!(checksum_record(&build_record(7, 2, 8)), 0x9b52_5460_dd07_0517);
    }
```

(If there is no `golden_checksums` test yet, add it; keep any existing determinism/varies-by-index tests, updating `a.entries[0].command.len()` → `a.entries[0].cmd_text.len()`.)

- [ ] **Step 5: Build + test**

Run: `cd rust && cargo test -p serialization-common && cargo clippy -p serialization-common --all-targets && cargo fmt --check`
Expected: PASS (golden checksums match the new values). The three Rust codec crates will not compile yet — that is expected and fixed in Tasks 2–3.

- [ ] **Step 6: Commit**

```bash
git add rust/serialization/common/
git commit -m "feat(serialization): typed command in shared Rust model (int/float/bool/string, golden regenerated)"
```

---

### Task 2: Rust SBE cells — `sbe_gen` + `aeron_sbe` + conformance

**Files:**
- Modify: `rust/serialization/sbe_gen/schema/journal.xml`
- Modify: `rust/serialization/aeron_sbe/schema/journal.xml`
- Regenerate + commit: `rust/serialization/aeron_sbe/generated/journal/`
- Modify: `rust/serialization/sbe_gen/src/lib.rs`
- Modify: `rust/serialization/aeron_sbe/src/lib.rs`
- Modify: `rust/serialization/conformance/` (byte-identity test, if it asserts a size/shape)

**Interfaces:**
- Consumes: `serialization_common::{JournalRecord, Entry, build_record, checksum_record}` (Task 1).
- Verified generator facts: sbe_gen emits, in the entry, `cmd_qty: I64`, `cmd_price: F64`, `cmd_flag: u8`, `cmd_text: VarData` (view accessors `cmd_qty()->Option<&I64>` with `.get()`, `cmd_price()` `.get() -> f64`, `cmd_flag` field, `cmd_text.bytes`); encoder setters `.cmd_qty(i64).cmd_price(f64).cmd_flag(u8)` and `.cmd_text(&[u8])`. The aeron-jar Rust output exposes analogous fields. Both handle `double` (confirmed).

- [ ] **Step 1: Update both SBE schemas (identical edit)**

In BOTH `rust/serialization/sbe_gen/schema/journal.xml` and `rust/serialization/aeron_sbe/schema/journal.xml`, change the `entries` group: bump `blockLength` to 45 and replace the `command` `<data>` with the three scalars + a `cmdText` var-data:

```xml
    <group name="entries" id="10" dimensionType="groupSizeEncoding" blockLength="45">
      <field name="entryTermId"    id="11" type="int64"/>
      <field name="entryIndex"     id="12" type="int64"/>
      <field name="entryTimestamp" id="13" type="int64"/>
      <field name="commandKey"     id="14" type="int32"/>
      <field name="cmdQty"         id="15" type="int64"/>
      <field name="cmdPrice"       id="16" type="double"/>
      <field name="cmdFlag"        id="17" type="uint8"/>
      <data  name="cmdText"        id="18" type="varDataEncoding"/>
    </group>
```

(The two files must stay identical — the byte-identity guarantee depends on it.)

- [ ] **Step 2: Regenerate the aeron_sbe committed crate**

```bash
cd rust && sh serialization/aeron_sbe/regen.sh && cargo fmt -p journal-aeron-sbe
```

Expected: `rust/serialization/aeron_sbe/generated/journal/` reflects the new entry fields; the regen script re-applies the workspace manifest and formats.

- [ ] **Step 3: Update `sbe_gen` adapter (encode + decode fold)**

In `rust/serialization/sbe_gen/src/lib.rs`, in the entry-encoding closure replace the `command` write with the four fields (the `cmd_text` string encodes as its UTF-8 bytes):

```rust
                ee.command_key(e.command_key)
                    .cmd_qty(e.cmd_qty)
                    .cmd_price(e.cmd_price)
                    .cmd_flag(e.cmd_flag as u8);
                ee.cmd_text(e.cmd_text.as_bytes())?;
                Ok(())
```

and in `decode_checksum`, replace the `add_bytes(entry.command.bytes)` fold:

```rust
        c.add_i64(entry.cmd_qty().map(|v| v.get()).unwrap_or(0));
        c.add_f64(entry.cmd_price().map(|v| v.get()).unwrap_or(0.0));
        c.add_bool(entry.cmd_flag().copied().unwrap_or(0) != 0);
        // cmd_text is var-data bytes = the string's UTF-8; fold via add_str over the bytes.
        c.add_str(std::str::from_utf8(entry.cmd_text.bytes).unwrap_or(""));
```

(If the generated accessor for `cmd_flag` differs, use the field the regenerated `journal_record.rs` exposes — it is a plain `u8` in the entry block. Adjust the exact `.get()`/field access to match the regenerated code; the field names `cmd_qty`/`cmd_price`/`cmd_flag`/`cmd_text` are fixed by the schema.)

- [ ] **Step 4: Update `aeron_sbe` adapter**

In `rust/serialization/aeron_sbe/src/lib.rs`, make the equivalent encode/decode changes using the aeron-generated flyweight API (setters for `cmd_qty`/`cmd_price`/`cmd_flag`, put for `cmd_text`; getters + `cmd_text` var-data on decode), folding in the same order (`add_i64`, `add_f64`, `add_bool`, `add_str`). Read the regenerated `generated/journal/src/lib.rs` accessor names and match them.

- [ ] **Step 5: Fix the conformance byte-identity test**

In `rust/serialization/conformance/`, the test asserts `sbe_gen` and `aeron_sbe` encode byte-identical bodies for `build_record(...)`. Update any hard-coded expected size and confirm the assertion still holds for the new record.

Run: `cd rust && cargo test -p serialization-sbe_gen -p serialization-aeron_sbe -p serialization-conformance`
Expected: round-trip checksum tests fold to `checksum_record` (new fields included); sbe_gen and aeron_sbe produce identical bytes.

- [ ] **Step 6: Gate + commit**

Run: `cd rust && cargo build --release && cargo clippy --all-targets && cargo fmt --check`
Then a loopback smoke to capture the new size:
`SER_ENTRIES=4 SER_CMD_BYTES=78 SER_WARMUP=100 SER_ITERS=1000 cargo run --release -q -p serialization-sbe_gen`
Expected: 4 JSON lines; note the `encoded_bytes` value (record now ~280–320 B, down from 502).

```bash
git add rust/serialization/sbe_gen/ rust/serialization/aeron_sbe/ rust/serialization/conformance/
git commit -m "feat(serialization): typed command in Rust SBE cells (sbe_gen + aeron_sbe, blockLength 45)"
```

---

### Task 3: Rust `bincode` cell

**Files:**
- Modify: `rust/serialization/bincode/src/lib.rs` (and its mirror struct if separate)

**Interfaces:**
- Consumes: `serialization_common::{JournalRecord, Entry}` (Task 1).

- [ ] **Step 1: Update the bincode mirror + adapter**

If the bincode cell uses a mirror struct, replace its entry `command: Vec<u8>` with `cmd_qty: i64, cmd_price: f64, cmd_flag: bool, cmd_text: String` (matching `serialization_common::Entry`); if it serializes `serialization_common::JournalRecord` directly via its serde derives, no struct edit is needed. Update the decode-fold to fold the four new fields in the canonical order:

```rust
        c.add_i64(e.cmd_qty);
        c.add_f64(e.cmd_price);
        c.add_bool(e.cmd_flag);
        c.add_str(&e.cmd_text);
```

(replacing `c.add_bytes(&e.command)`).

- [ ] **Step 2: Test + gate + commit**

Run: `cd rust && cargo test -p serialization-bincode && cargo clippy -p serialization-bincode --all-targets && cargo fmt --check && cargo build --release`
Expected: round-trip checksum matches `checksum_record`.

```bash
git add rust/serialization/bincode/
git commit -m "feat(serialization): typed command in Rust bincode cell"
```

---

### Task 4: Go shared model — `serjournal`

**Files:**
- Modify: `go/internal/serjournal/serjournal.go`
- Modify: `go/internal/serjournal/serjournal_test.go`

**Interfaces:**
- Produces (consumed by Tasks 5–8): `Entry { EntryTermID, EntryIndex, EntryTimestamp int64; CommandKey int32; CmdQty int64; CmdPrice float64; CmdFlag bool; CmdText string }`; `BuildRecord(index uint64, entries, textLen int) Record`; `ChecksumRecord(*Record) uint64`; `Checksum` with `AddI64/AddI32/AddU8/AddBytes` plus new `AddF64(float64)`, `AddBool(bool)`, `AddString(string)`.

- [ ] **Step 1: Update `Entry`, `BuildRecord`, `Checksum`, `ChecksumRecord`**

In `go/internal/serjournal/serjournal.go`, replace the `Command []byte` field and the command fill. Add `import "math"`.

```go
type Entry struct {
	EntryTermID    int64
	EntryIndex     int64
	EntryTimestamp int64
	CommandKey     int32
	CmdQty         int64
	CmdPrice       float64
	CmdFlag        bool
	CmdText        string
}
```

In `BuildRecord` (rename the `cmdBytes` param to `textLen`), replace the per-entry command block:

```go
	for k := uint64(0); k < uint64(entries); k++ {
		e := mix(h ^ k*0x100000001B3)
		t := mix(e ^ 0xAA)
		text := make([]byte, textLen)
		for i := range text {
			text[i] = 0x20 + byte(t>>(i%8*8))%95
		}
		group = append(group, Entry{
			EntryTermID:    int64(e),
			EntryIndex:     int64(index*uint64(entries) + k),
			EntryTimestamp: int64(mix(e)),
			CommandKey:     int32(e >> 32),
			CmdQty:         int64(mix(e)),
			CmdPrice:       float64(mix(e^0xF0)>>11) * 3.0517578125e-5,
			CmdFlag:        mix(e^0x0F)&1 == 1,
			CmdText:        string(text),
		})
	}
```

Add checksum methods (after `AddBytes`):

```go
func (c *Checksum) AddF64(v float64) { c.step(math.Float64bits(v)) }
func (c *Checksum) AddBool(v bool) {
	if v {
		c.step(1)
	} else {
		c.step(0)
	}
}
func (c *Checksum) AddString(s string) {
	c.step(uint64(len(s)))
	for i := 0; i < len(s); i++ {
		c.step(uint64(s[i]))
	}
}

// AddStringBytes folds a UTF-8 byte view identically to AddString, so the
// zero-copy decoders (flyweight, flatbuffers) can fold cmdText without a
// string() allocation.
func (c *Checksum) AddStringBytes(b []byte) {
	c.step(uint64(len(b)))
	for _, x := range b {
		c.step(uint64(x))
	}
}
```

(`AddStringBytes(b)` and `AddString(s)` produce the same fold for identical
content, so owned decoders can use `AddString(e.CmdText)` while zero-copy
decoders use `AddStringBytes(view)`.)

Update the per-entry fold in `ChecksumRecord`:

```go
	for i := range r.Entries {
		e := &r.Entries[i]
		c.AddI64(e.EntryTermID)
		c.AddI64(e.EntryIndex)
		c.AddI64(e.EntryTimestamp)
		c.AddI32(e.CommandKey)
		c.AddI64(e.CmdQty)
		c.AddF64(e.CmdPrice)
		c.AddBool(e.CmdFlag)
		c.AddString(e.CmdText)
	}
```

- [ ] **Step 2: Update the golden test**

In `go/internal/serjournal/serjournal_test.go`, replace the golden values (and any `len(a.Entries[0].Command)` assertion → `len(a.Entries[0].CmdText)`):

```go
var golden = []struct {
	index             uint64
	entries, textLen  int
	want              uint64
}{
	{0, 4, 78, 0x86d721cbffdefc06},
	{1, 4, 78, 0xddb1bfa73e9819cb},
	{42, 4, 78, 0x495a0d763cc820ca},
	{99999, 4, 78, 0x552b92436dae830e},
	{7, 2, 8, 0x9b525460dd070517},
}
```

Update the call to `BuildRecord(g.index, g.entries, g.textLen)`.

- [ ] **Step 3: Test**

Run: `cd go && go test ./internal/serjournal/ -run 'TestGolden|TestBuildRecord' && gofmt -l internal/serjournal/serjournal.go`
Expected: golden checksums match Rust (Task 1); the codec adapter tests will fail to compile until Tasks 5–8 — run only the model tests here.

- [ ] **Step 4: Commit**

```bash
git add go/internal/serjournal/serjournal.go go/internal/serjournal/serjournal_test.go
git commit -m "feat(serialization): typed command in shared Go model (golden matches Rust)"
```

---

### Task 5: Go SBE cells — flyweight (`aeron_sbe`) + struct (`sbe_struct`)

**Files:**
- Regenerate + commit: `go/internal/serjournal/journalsbe/`, `go/internal/serjournal/journalsbestruct/`
- Modify: `go/internal/serjournal/sbe.go`, `go/internal/serjournal/sbe_struct.go`
- Regenerate + commit: `go/internal/serjournal/testdata/journal_sbe_golden.bin`
- Modify: `go/internal/serjournal/sbe_test.go` (byte-identity + round-trip + size band)

**Interfaces:**
- Consumes: `serjournal.{Record, BuildRecord, ChecksumRecord, NewChecksum}` (Task 4), and the updated `rust/serialization/aeron_sbe/schema/journal.xml` (Task 2).
- Verified flyweight accessors: `SetCmdQty(int64)`, `SetCmdPrice(float64)`, `SetCmdFlag(uint8)`, `PutCmdText(string)`; getters `CmdQty() int64`, `CmdPrice() float64`, `CmdFlag() uint8`, `CmdText() string` / `GetCmdText(dst []byte) int`. Struct mode: fields `CmdQty int64`, `CmdPrice float64`, `CmdFlag uint8`, `CmdText []uint8`.

- [ ] **Step 1: Regenerate both SBE Go packages**

```bash
cd go
./internal/serjournal/regen-journalsbe.sh
./internal/serjournal/regen-journalsbestruct.sh
go build ./internal/serjournal/journalsbe/ ./internal/serjournal/journalsbestruct/
```

Expected: both packages reflect the new entry fields; build passes.

- [ ] **Step 2: Update the flyweight adapter (`sbe.go`)**

In `EncodeSBE`, replace the `PutCommand(...)` per-entry write with the four fields:

```go
		g.SetEntryTermId(e.EntryTermID).
			SetEntryIndex(e.EntryIndex).
			SetEntryTimestamp(e.EntryTimestamp).
			SetCommandKey(e.CommandKey).
			SetCmdQty(e.CmdQty).
			SetCmdPrice(e.CmdPrice).
			SetCmdFlag(boolU8(e.CmdFlag))
		g.PutCmdText(e.CmdText)
```

In `DecodeChecksum`, replace the entry fold's `GetCommand`/`AddBytes` with:

```go
		ck.AddI64(e.EntryTermId())
		ck.AddI64(e.EntryIndex())
		ck.AddI64(e.EntryTimestamp())
		ck.AddI32(e.CommandKey())
		ck.AddI64(e.CmdQty())
		ck.AddF64(e.CmdPrice())
		ck.AddBool(e.CmdFlag() != 0)
		n := e.GetCmdText(c.cmd)
		ck.AddStringBytes(c.cmd[:n]) // AddStringBytes, not AddString(string(...)), to stay 0-alloc
```

Add the helper near the top of `sbe.go`:

```go
func boolU8(b bool) uint8 {
	if b {
		return 1
	}
	return 0
}
```

(`GetCmdText(dst []byte) int` copies the var-data into the reused `c.cmd` buffer — zero-alloc read path, same as the old `GetCommand`. If the flyweight only exposes `CmdText() string`, use that but note it allocates; prefer `GetCmdText`.)

- [ ] **Step 3: Update the struct adapter (`sbe_struct.go`)**

In `ToSBEStruct`, set the new entry fields (`CmdText` is `[]uint8` in struct mode — the UTF-8 bytes):

```go
		entries[i] = journalsbestruct.JournalRecordEntries{
			EntryTermId: e.EntryTermID, EntryIndex: e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp, CommandKey: e.CommandKey,
			CmdQty: e.CmdQty, CmdPrice: e.CmdPrice,
			CmdFlag: boolU8(e.CmdFlag), CmdText: []uint8(e.CmdText),
		}
```

In `DecodeChecksum`, fold the four new fields:

```go
		ck.AddI64(e.CmdQty)
		ck.AddF64(e.CmdPrice)
		ck.AddBool(e.CmdFlag != 0)
		ck.AddStringBytes(e.CmdText) // CmdText is []uint8 in struct mode
```

(replacing `ck.AddBytes(e.Command)`.)

- [ ] **Step 4: Regenerate the byte-identity golden + update tests**

Regenerate the committed golden frame from the flyweight encode of `BuildRecord(7,4,78)` (all four SBE cells are byte-identical), then have the tests assert against it. Add a one-shot regen path (a small `go test` that writes the file, or a documented snippet) so the `.bin` is reproducible:

```bash
cd go
cat > /tmp/gen_golden.go <<'GO'
//go:build ignore
package main
import ("os"; "github.com/peterknego/hi-perf-cmp/go/internal/serjournal")
func main() {
	c := serjournal.NewSBECodec()
	buf := make([]byte, 64*1024)
	r := serjournal.BuildRecord(7, 4, 78)
	n := c.Encode(r, buf)
	_ = os.WriteFile("internal/serjournal/testdata/journal_sbe_golden.bin", buf[:n], 0o644)
}
GO
go run /tmp/gen_golden.go && wc -c internal/serjournal/testdata/journal_sbe_golden.bin
```

In `sbe_test.go`, update the size assertions in `TestSBEFlyweightByteIdentity`/`TestSBEStructByteIdentity` (the golden is no longer 502 B — assert `len(golden) == <measured>` or drop the hard-coded 502 and compare bytes only), and update `TestSBEFlyweightEncodedSize...`/`sbe_struct` size bands to the new record.

- [ ] **Step 5: Test + smoke**

Run: `cd go && go build ./... && go vet ./internal/serjournal/... && go test ./internal/serjournal/ -run 'TestSBE'`
Then confirm 0-alloc still holds for the flyweight decode and the struct decode still allocates:
`SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-aeron_sbe | tee /dev/stderr | wc -l` (8 lines, decode_alloc_bytes=0) and same for `serialization-sbe_struct` (decode_alloc_bytes > 0).

- [ ] **Step 6: Commit**

```bash
git add go/internal/serjournal/journalsbe/ go/internal/serjournal/journalsbestruct/ \
        go/internal/serjournal/sbe.go go/internal/serjournal/sbe_struct.go \
        go/internal/serjournal/testdata/journal_sbe_golden.bin go/internal/serjournal/sbe_test.go
git commit -m "feat(serialization): typed command in Go SBE cells (flyweight + struct, golden regenerated)"
```

---

### Task 6: Go `bebop` cell

**Files:**
- Modify: `go/internal/serjournal/schema/journal.bop`
- Regenerate + commit: `go/internal/serjournal/journalbop/`
- Modify: `go/internal/serjournal/bebop.go`, and the bebop test / size band in the serialization test file

**Interfaces:**
- Consumes: `serjournal.{Record, BuildRecord, ChecksumRecord}` (Task 4).

- [ ] **Step 1: Update the `.bop` schema**

In the bebop schema for the journal record (the `Entry` struct), replace `byte[] command;` with:

```
    int64 cmdQty;
    float64 cmdPrice;
    bool cmdFlag;
    string cmdText;
```

- [ ] **Step 2: Regenerate + commit the codec**

```bash
cd go && ./internal/serjournal/regen-journalbop.sh && go build ./internal/serjournal/journalbop/
```

Expected: generated `Entry` gains `CmdQty int64`, `CmdPrice float64`, `CmdFlag bool`, `CmdText string`.

- [ ] **Step 3: Update the adapter (`bebop.go`)**

In `ToBebop`, replace the `Command:` field with the four fields; in `DecodeBebopChecksum`, replace the `AddBytes(...)` fold with:

```go
		c.AddI64(e.CmdQty)
		c.AddF64(e.CmdPrice)
		c.AddBool(e.CmdFlag)
		c.AddString(e.CmdText)
```

- [ ] **Step 4: Test + commit**

Run: `cd go && go build ./... && go vet ./internal/serjournal/... && go test ./internal/serjournal/ -run TestBebop && gofmt -l internal/serjournal/bebop.go`
Expected: round-trip checksum matches; size band updated.

```bash
git add go/internal/serjournal/schema/journal.bop go/internal/serjournal/journalbop/ go/internal/serjournal/bebop.go go/internal/serjournal/*_test.go
git commit -m "feat(serialization): typed command in Go bebop cell"
```

---

### Task 7: Go `protobuf` cell

**Files:**
- Modify: `go/internal/serjournal/schema/journal.proto`
- Regenerate + commit: `go/internal/serjournal/journalpb/`
- Modify: `go/internal/serjournal/proto.go`, size band in test

**Interfaces:**
- Consumes: `serjournal.{Record, BuildRecord, ChecksumRecord}` (Task 4).

- [ ] **Step 1: Update the `.proto` schema**

In the `Entry` message, replace `bytes command = N;` with (keeping field numbers contiguous):

```proto
  sfixed64 cmd_qty = <n>;
  double   cmd_price = <n+1>;
  bool     cmd_flag = <n+2>;
  string   cmd_text = <n+3>;
```

(Use the next free field numbers after `command_key`; `sfixed64` matches the fixed-width convention already used for the wide ids.)

- [ ] **Step 2: Regenerate + commit**

```bash
cd go && ./internal/serjournal/regen-journalpb.sh && go build ./internal/serjournal/journalpb/
```

Expected: generated `Entry` gains `CmdQty int64`, `CmdPrice float64`, `CmdFlag bool`, `CmdText string`.

- [ ] **Step 3: Update the adapter (`proto.go`)**

In `ToProto`, set the four new fields; in `DecodeProtoChecksum`, replace the `AddBytes(...)` fold with:

```go
		c.AddI64(e.CmdQty)
		c.AddF64(e.CmdPrice)
		c.AddBool(e.CmdFlag)
		c.AddString(e.CmdText)
```

- [ ] **Step 4: Test + commit**

Run: `cd go && go build ./... && go vet ./internal/serjournal/... && go test ./internal/serjournal/ -run TestProto && gofmt -l internal/serjournal/proto.go`

```bash
git add go/internal/serjournal/schema/journal.proto go/internal/serjournal/journalpb/ go/internal/serjournal/proto.go go/internal/serjournal/*_test.go
git commit -m "feat(serialization): typed command in Go protobuf cell"
```

---

### Task 8: Go `flatbuffers` cell

**Files:**
- Modify: `go/internal/serjournal/schema/journal.fbs`
- Regenerate + commit: `go/internal/serjournal/journalfb/`
- Modify: `go/internal/serjournal/flatbuffers.go`, size band in test

**Interfaces:**
- Consumes: `serjournal.{Record, BuildRecord, ChecksumRecord}` (Task 4). Needs `flatc` 23.5.26 at regen time (fetch the prebuilt binary as in the flatbuffers cell's plan).

- [ ] **Step 1: Update the `.fbs` schema**

In `table Entry`, replace `command:[ubyte];` with:

```
  cmd_qty:long;
  cmd_price:double;
  cmd_flag:bool;
  cmd_text:string;
```

- [ ] **Step 2: Fetch flatc + regenerate + commit**

```bash
cd /home/claude/ultima/hi-perf-cmp
TMP=$(mktemp -d); curl -fsSL -o "$TMP/f.zip" "https://github.com/google/flatbuffers/releases/download/v23.5.26/Linux.flatc.binary.g%2B%2B-10.zip"; unzip -o "$TMP/f.zip" -d "$TMP" >/dev/null; chmod +x "$TMP/flatc"
PATH="$TMP:$PATH" ./go/internal/serjournal/regen-journalfb.sh
cd go && go build ./internal/serjournal/journalfb/
```

Expected: generated `Entry` gains `CmdQty() int64`, `CmdPrice() float64`, `CmdFlag() bool`, `CmdText() []byte` (FlatBuffers string accessor returns the byte view; `CmdText()` returns `[]byte`), plus builder `EntryAddCmdQty/CmdPrice/CmdFlag/CmdText`.

- [ ] **Step 3: Update the adapter (`flatbuffers.go`)**

Bottom-up build requires the `cmd_text` string offset created before `EntryStart` (like the old command vector). In `Encode`, per entry create the string offset first, then set the scalars and the string:

```go
	for i := range r.Entries {
		e := &r.Entries[i]
		textOff := b.CreateString(e.CmdText)
		journalfb.EntryStart(b)
		journalfb.EntryAddEntryTermId(b, e.EntryTermID)
		journalfb.EntryAddEntryIndex(b, e.EntryIndex)
		journalfb.EntryAddEntryTimestamp(b, e.EntryTimestamp)
		journalfb.EntryAddCommandKey(b, e.CommandKey)
		journalfb.EntryAddCmdQty(b, e.CmdQty)
		journalfb.EntryAddCmdPrice(b, e.CmdPrice)
		journalfb.EntryAddCmdFlag(b, e.CmdFlag)
		journalfb.EntryAddCmdText(b, textOff)
		c.offs[i] = journalfb.EntryEnd(b)
	}
```

In `DecodeChecksum`, replace the `AddBytes(c.ent.CommandBytes())` fold with:

```go
		ck.AddI64(c.ent.CmdQty())
		ck.AddF64(c.ent.CmdPrice())
		ck.AddBool(c.ent.CmdFlag())
		ck.AddStringBytes(c.ent.CmdText()) // []byte view; AddStringBytes keeps decode 0-alloc
```

(`CmdText()` returns `[]byte` — the zero-copy view. Use `AddStringBytes` (Task 4), not `AddString(string(...))`, so the flatbuffers decode stays 0-alloc — confirm with the smoke run in Step 4.)

- [ ] **Step 4: Test + smoke + commit**

Run: `cd go && go build ./... && go vet ./internal/serjournal/... && go test ./internal/serjournal/ -run TestFlatBuffers && gofmt -l internal/serjournal/flatbuffers.go`
Then confirm the decode is still 0-alloc: `SER_WARMUP=100 SER_ITERS=1000 go run ./cmd/serialization-flatbuffers | tee /dev/stderr | wc -l` (8 lines, decode_alloc_bytes=0). If `string(CmdText())` broke 0-alloc, switch to the `AddStringBytes` byte-loop path.

```bash
git add go/internal/serjournal/schema/journal.fbs go/internal/serjournal/journalfb/ go/internal/serjournal/flatbuffers.go go/internal/serjournal/*_test.go
git commit -m "feat(serialization): typed command in Go flatbuffers cell"
```

---

### Task 9: Full-suite gate + docs

**Files:**
- Modify: `CLAUDE.md` (serialization description), `docs/RESULTS.md` (note the record change)

**Interfaces:**
- Consumes: all prior tasks.

- [ ] **Step 1: Full gates**

Run: `cd rust && cargo build --release && cargo test && cargo clippy --all-targets && cargo fmt --check`
Run: `cd go && go build ./... && go vet ./... && go test ./...`
Expected: all green — every codec round-trips to the shared `ChecksumRecord`, the four SBE cells are byte-identical, cross-language goldens match.

- [ ] **Step 2: Docs**

In `CLAUDE.md`, update the `serialization` description: the record's per-entry command is no longer an opaque variable-length blob but a **typed command** (`cmdQty` int64, `cmdPrice` float64, `cmdFlag` bool, `cmdText` string), so the grid exercises field encoding (float/varint/bool/string) rather than a memcpy; `SER_CMD_BYTES` now sizes the `cmdText` string.

In `docs/RESULTS.md`, add a one-line note under the serialization section that as of the next run the record uses a typed command (the prior payload-dominated numbers are historical); do **not** fabricate new numbers — RESULTS is updated with real figures only after an AWS run.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md docs/RESULTS.md
git commit -m "docs(serialization): typed command replaces opaque blob (field-heavy record)"
```
