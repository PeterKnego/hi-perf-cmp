# 20260723T081721Z-95af18f1353d

- commit: 95af18f1353d9a94c71a44c503cc666d1b85d2d6 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
serialization full grid on the new FIELD-HEAVY typed-command record (int/float/bool/string replaces opaque blob) — exercises field encoding, not memcpy

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### serialization / aeron_sbe

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| go | 248.1 | 238 | 354 | 136.2 | 125 | 269 | 306 |
| rust | 125.8 | 120 | 193 | 69.3 | 56 | 297 | 306 |

### serialization / bebop

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 352 | 454.4 | 404 | 912 | 146.1 | 112 | 479 | 298 |

### serialization / bincode

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| rust | 336 | 365.8 | 360 | 439 | 71.3 | 60 | 305 | 290 |

### serialization / flatbuffers

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| go | 477.3 | 459 | 766 | 833.8 | 817 | 979 | 472 |

### serialization / protobuf

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 696 | 1387.4 | 1192 | 5173 | 680.9 | 658 | 928 | 326 |

### serialization / sbe_gen

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 125.1 | 120 | 194 | 57.8 | 42 | 308 | 306 |

### serialization / sbe_struct

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 384 | 1116.8 | 941 | 4932 | 429.1 | 404 | 597 | 306 |

## Hypothesis
First run on the **field-heavy** record: the opaque per-entry `command []byte`
blob was replaced by a typed command (`cmdQty` int64, `cmdPrice` float64,
`cmdFlag` bool, `cmdText` string, the string now a short 12-char field). The
prior blob-dominated record made every codec's per-entry work ~a memcpy plus a
big byte-fold, masking field-encoding differences. Expectation: with the blob
gone, the real encode/decode-cost spread across codecs widens sharply, and the
four zero-copy cells still decode at 0 allocation.

## Observations
Record ~290–472 B (down from ~500–616). encode/decode p50 (ns), decode_alloc:

| cell | enc p50 | dec p50 | dec mean | bytes | alloc |
|---|---|---|---|---|---|
| rust/sbe_gen    |  42 |  120 |  125 | 306 | **0**   |
| rust/aeron_sbe  |  56 |  120 |  126 | 306 | **0**   |
| go/aeron_sbe    | 125 |  238 |  248 | 306 | **0**   |
| go/flatbuffers  | 817 |  459 |  477 | 472 | **0**   |
| rust/bincode    |  60 |  360 |  366 | 290 | 336 |
| go/bebop        | 112 |  404 |  454 | 298 | 352 |
| go/sbe_struct   | 404 |  941 | 1117 | 306 | 384 |
| go/protobuf     | 658 | 1192 | 1387 | 326 | 696 |

- **Removing the blob widened the spread and sped up decode.** Decode p50 ranges
  120 ns (Rust SBE) to 1192 ns (Go protobuf) — a ~10× spread, vs the blob
  record's ~4×. SBE decode itself dropped from ~408 ns to **120 ns** because the
  old 78-byte command blob's byte-by-byte checksum fold (identical busywork for
  all codecs) is gone; what remains is genuine field materialization. This is
  exactly the effect the change was made to expose.
- **The four zero-copy cells still decode at 0 allocation** (Rust sbe_gen/
  aeron_sbe, Go SBE flyweight, Go flatbuffers) — the invariant survived the
  field-shape change. The owned decoders allocate 336–696 B (protobuf highest, at
  696 B, for its owned message + string).
- **FlatBuffers is now the most expensive to ENCODE (817 ns)** — the blob record
  hid this (its FB encode was 572 ns). With more typed fields + a nested string,
  FB's bottom-up builder (CreateString before each table, vtable construction per
  entry) dominates, and its wire is the largest (472 B) because the per-table
  vtable overhead is a bigger fraction when fields are small. Its decode (459 ns)
  is also slower than the SBE flyweight (238 ns) — SBE's fixed offsets still beat
  FB's vtable indirection at 0 alloc.
- **protobuf is the slowest at both ends** among the field-heavy codecs (encode
  658 ns, decode 1192 ns) — varint-decoding many typed scalars is costly — though
  its wire (326 B) stays compact.
- **bincode looks better on this record** (decode 360 ns) than it did dominated
  by the blob (933 ns) — its owned field decode is cheap once the big byte copy is
  gone; it still allocates 336 B and stays the smallest wire (290 B, varints).
- **Rust SBE remains the champion**: 42–56 ns encode, 120 ns decode, 0 alloc,
  306 B. The Go SBE flyweight is ~2× its decode (238 ns) at the same 0-alloc and
  identical 306-byte wire — the persistent Rust-vs-Go zero-copy language gap.
- Scoped run (8 serialization cells); the three new-in-this-run rows are the
  field-heavy re-measurement of the existing experiments (record shape changed,
  so these are not directly comparable to the 20260722 blob-record numbers).
