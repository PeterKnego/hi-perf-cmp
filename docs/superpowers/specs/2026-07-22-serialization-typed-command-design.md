# serialization — typed command (replace opaque byte blob) — Design

**Date:** 2026-07-22
**Status:** Approved

## Purpose

The shared serialization record (`JournalRecord`) is dominated by an opaque
`command []byte` blob (4 × 78 B ≈ 62 % of the ~500 B record). For a codec
comparison that is close to dead weight: every codec treats a byte vector as
length-prefix + memcpy, and it drowns out the field-encoding machinery
(float/varint/bool/string/offset handling) that actually distinguishes SBE from
protobuf from FlatBuffers from bincode.

This change **replaces the opaque `command` blob on each entry with a small
made-up command of typed fields** — one int, one float, one bool, one string —
so the record becomes field-heavy and small, and every codec's typed-field
encoding is exercised. It is an **in-place** change to the shared record: no new
cells, no new focus area; the existing eight serialization cells simply re-run
against the new record shape.

## The new entry shape

Each `Entry` keeps its own four fields (`entryTermId`, `entryIndex`,
`entryTimestamp`, `commandKey`) and **drops `command []byte`**, gaining:

| field      | logical type | wire mapping                                   |
|------------|--------------|------------------------------------------------|
| `cmdQty`   | int64        | int64 everywhere                               |
| `cmdPrice` | float64      | double / float64 (IEEE-754, 8 B, all codecs)   |
| `cmdFlag`  | bool         | bool where native; **uint8 0/1** in SBE (no bool)|
| `cmdText`  | string       | string where native; **var-data** in SBE       |

`cmdText` is the only variable-length field remaining. The record shrinks from
~500 B to ~300 B and shifts from payload-dominated to field-dominated.

## In-place scope (all eight codecs, both languages)

- **Shared logical models**: Rust `serialization-common` `Entry`
  (`command: Vec<u8>` → `cmd_qty: i64, cmd_price: f64, cmd_flag: bool, cmd_text:
  String`) and Go `serjournal` `Entry` (`Command []byte` → `CmdQty int64,
  CmdPrice float64, CmdFlag bool, CmdText string`). Update the deterministic
  `build_record`/`BuildRecord` and `checksum_record`/`ChecksumRecord`.
- **Shared checksum** gains, in both languages: `add_f64`/`AddF64` (fold the raw
  IEEE-754 bits, `f64::to_bits`/`math.Float64bits`), `add_bool`/`AddBool` (fold as
  `u8` 0/1), and `add_str`/`AddString` (fold as bytes, reusing the existing
  length-then-bytes rule). The FNV offset/prime are unchanged.
- **Schemas** (the entry's fixed block gains the three scalars; the var-data
  becomes the string):
  - SBE `rust/serialization/aeron_sbe/schema/journal.xml` and the identical
    `rust/serialization/sbe_gen/schema/journal.xml`: in the `entries` group add
    `cmdQty` int64, `cmdPrice` double (a `float` primitive), `cmdFlag` uint8,
    and replace the `command` `<data>` with a `cmdText` `<data>` (var-data,
    UTF-8 string). Group `blockLength` grows 28 → 45 (28 + 8 + 8 + 1).
  - bebop `.bop`, proto3 `.proto`, FlatBuffers `.fbs`, and the Rust `bincode`
    mirror struct: replace the byte field with `cmdQty`/`cmdPrice`/`cmdFlag`/
    `cmdText` (int64/double/bool/string).
- **Adapters** (encode + decode + fold): Rust sbe_gen, aeron_sbe, bincode; Go SBE
  flyweight (`aeron_sbe`), SBE struct (`sbe_struct`), bebop, protobuf,
  flatbuffers. All SBE-generated code regenerates from the one shared
  `journal.xml`.
- **Golden artifacts regenerated**: the cross-language golden checksums (Rust →
  Go tests) and the committed SBE byte-identity golden frame
  (`go/internal/serjournal/testdata/journal_sbe_golden.bin`). Encoded-bytes size
  bands in every codec's tests updated to the new ~300 B record.

## Determinism & cross-language safety

The golden-checksum anchor (Rust and Go build byte/field-identical records) must
survive, so every new field is derived deterministically from the existing
splitmix64 seed, and each maps to a cross-language-stable value:

- `cmdQty = mix(seed) as i64` — plain int.
- `cmdPrice`: a **finite** double (no NaN/inf). Derived as e.g.
  `(mix(seed) >> 11) as f64 * 3.0517578125e-5` (a normal, finite value); the
  value itself is immaterial to encoding cost, but keeping it finite avoids
  NaN-bit ambiguity in round-trip/golden checks. Folded by raw bits, identical in
  Rust and Go.
- `cmdFlag = (mix(seed) & 1) == 1`.
- `cmdText`: deterministic **printable ASCII** (bytes mapped into `0x20..0x7E`)
  of length `SER_CMD_BYTES` (this knob is repurposed from the old blob length to
  the command-string length; default kept at its current value). Printable so it
  is valid UTF-8 for the codecs that type it as `string`.

## Metrics & result contract

No change to the metric set, experiment names, focus area, or harness. The eight
cells still emit `encode_{p50,p99,mean}`, `decode_{p50,p99,mean}`,
`encoded_bytes` (now ~300, not ~500), `decode_alloc_bytes`. Because the record
changed, the pre-change serialization numbers are historical; the next AWS run
measures the field-heavy record. `tools/journal` continues to align on
`(focus_area, experiment, language, metric)`.

## Tests

- **Cross-language golden checksums** regenerated from the Rust builder and
  updated in the Go tests (`serjournal`, `serialization-common`).
- **SBE byte-identity**: all four SBE cells still produce a byte-identical frame;
  regenerate the committed golden `.bin` and keep the identity assertion.
- **Round-trip per codec**: `Decode…(Encode(build))` folds to `ChecksumRecord`
  (now including the four new fields) across configs.
- **Size bands** updated to the ~300 B record in every codec's size test.
- `cargo build/test/clippy/fmt` and `go build/vet/test` stay green.

## Non-goals

- No new codec, cell, or focus area.
- No opaque bytes left in the record.
- No change to the record header fields, the `RunJournal`/`run_journal` harness,
  or the `SER_*` contract beyond repurposing `SER_CMD_BYTES` as the string length.
- No FlatBuffers/SBE nested sub-message for the command — the fields are inlined
  on the entry (SBE has no nested variable-length composite; inlining keeps all
  five schemas uniform).
