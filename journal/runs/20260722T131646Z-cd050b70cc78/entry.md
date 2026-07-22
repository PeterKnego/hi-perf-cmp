# 20260722T131646Z-cd050b70cc78

- commit: cd050b70cc78f8f452294f953e9a09fd554dbec4 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
serialization full grid on AWS: adds Go SBE flyweight (aeron_sbe) + SBE struct (sbe_struct) + flatbuffers, alongside Rust sbe_gen/aeron_sbe/bincode and Go bebop/protobuf

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### serialization / aeron_sbe

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| go | 908.6 | 892 | 958 | 143.1 | 126 | 276 | 502 |
| rust | 416.2 | 409 | 447 | 83.7 | 60 | 381 | 502 |

### serialization / bebop

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 544 | 1265.4 | 1063 | 4013 | 103.5 | 83 | 293 | 494 |

### serialization / bincode

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| rust | 536 | 960.7 | 933 | 1068 | 105.0 | 90 | 366 | 482 |

### serialization / flatbuffers

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| go | 1062.6 | 1041 | 1295 | 590.2 | 572 | 756 | 608 |

### serialization / protobuf

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 888 | 1987.7 | 1718 | 5701 | 501.5 | 480 | 732 | 514 |

### serialization / sbe_gen

| language | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|
| rust | 415.4 | 408 | 443 | 72.7 | 45 | 387 | 502 |

### serialization / sbe_struct

| language | decode_alloc_bytes (bytes) | decode_mean (ns) | decode_p50 (ns) | decode_p99 (ns) | encode_mean (ns) | encode_p50 (ns) | encode_p99 (ns) | encoded_bytes (bytes) |
|---|---|---|---|---|---|---|---|---|
| go | 544 | 1676.9 | 1450 | 5229 | 350.1 | 338 | 502 | 502 |

## Hypothesis
First AWS run of the three new SBE/FlatBuffers Go cells alongside the existing
codecs. Expected: the two Go zero-copy cells — SBE flyweight (`aeron_sbe` go) and
`flatbuffers` — decode with **0 allocation** like the Rust SBE cells, while the
owned-decode cells (`bincode`, `sbe_struct`, `bebop`, `protobuf`) allocate. The
user's premise (from `kcchu/buffer-benchmarks`) was that FlatBuffers has the
fastest decode; testing whether that holds here.

## Observations
Decode p50 (ns) / decode_alloc_bytes / encoded_bytes, this run:

| cell | enc p50 | dec p50 | alloc | bytes |
|---|---|---|---|---|
| rust/sbe_gen    |  45 |  408 | **0**   | 502 |
| rust/aeron_sbe  |  60 |  409 | **0**   | 502 |
| go/aeron_sbe    | 126 |  892 | **0**   | 502 |
| go/flatbuffers  | 572 | 1041 | **0**   | 608 |
| rust/bincode    |  90 |  933 | 536     | 482 |
| go/bebop        |  83 | 1063 | 544     | 494 |
| go/sbe_struct   | 338 | 1450 | 544     | 502 |
| go/protobuf     | 480 | 1718 | 888     | 514 |

- **The four zero-copy cells all decode at 0 allocation, as designed** — the two
  Rust SBE cells, the Go SBE flyweight, and Go FlatBuffers. The three new cells
  behaved exactly as their design predicted: `aeron_sbe` go and `flatbuffers` at
  0 alloc, `sbe_struct` at 544 B (owned, same as bebop).
- **The FlatBuffers "fastest decode" premise does NOT hold here.** FlatBuffers
  decodes at 1041 ns — *slower* than the Go SBE flyweight (892 ns) and ~2.5× the
  Rust SBE cells (~408 ns), despite all four being zero-copy. The reason is field
  access, not allocation: SBE reads fixed byte offsets, while FlatBuffers chases
  vtable + offset indirection per field. Zero-copy eliminates *allocation*, not
  the per-field read cost — so "zero-copy" ≠ "fastest decode".
- **FlatBuffers also has the most expensive encode of the zero-copy codecs**
  (572 ns vs the SBE flyweight's 126 ns) because its Builder constructs the buffer
  bottom-up (byte-vectors → tables → vector → root), and the largest wire (608 B
  vs SBE's 502 B) from vtables + offsets.
- **Rust SBE is the decode champion** (~408 ns, 0 alloc) — ~2.2× faster than the
  Go SBE flyweight at identical 0-alloc and identical 502-byte wire, a pure
  language/codegen gap (fixed-offset reads, no bounds-check overhead).
- **Among owned-decode Go codecs**: bebop (1063 ns / 544 B) is fastest, then
  sbe_struct (1450 ns / 544 B — the SBE-tool's owned Golang codec streams through
  its SbeGoMarshaller), then protobuf (1718 ns / 888 B). The same SBE schema in Go
  costs ~1.6× more to decode in struct mode than flyweight mode (1450 vs 892 ns)
  and, of course, allocates.
- This run is scoped to `serialization` (8 cells), so `compare` shows only these
  cells moving vs the baseline; the three new cells are `added` rows.
