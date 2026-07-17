# rpc-roundtrip — Mutating Serialize/Send/Deserialize Round-Trip — Design

**Date:** 2026-07-17
**Status:** Approved

## Purpose

A new focus area, **`rpc-roundtrip`**, that measures the full application-level
round-trip latency of a *mutating* request/response across three realistic
transport+codec stacks. It combines the two existing hot paths — `serialization`
(codec cost) and `network-rtt` (wire cost) — into one measurement: unlike
`network-rtt`, where the responder echoes raw bytes, here **both ends do real
codec work**. The client serializes a ~250-byte struct and sends it; the
responder **deserializes it, mutates one field, and re-serializes** the reply;
the client deserializes and verifies. This is the shape of a real SMR RPC — a
follower that receives a command, updates state, and replies.

The headline question: what does a serialize → send → deserialize+mutate+
reserialize → send → deserialize round trip cost on each stack?

## The three cells (heterogeneous whole-stack combos)

Each cell is a runnable artifact `rpc-roundtrip-<experiment>`, one language per
experiment (the `serialization` precedent — experiments are language-restricted
via the bench-infra `languages` key):

| experiment    | language | transport      | codec                     |
|---------------|----------|----------------|---------------------------|
| `sbe_udp`     | Rust     | hand-rolled UDP| `sbe_gen` zero-copy SBE   |
| `grpc`        | Go       | gRPC (HTTP/2)  | protobuf (gRPC-native)    |
| `bebop_tcp`   | Go       | hand-rolled TCP| bebop safe API (200sc)    |

These vary in **both** transport and codec by design: the focus area compares
realistic end-to-end stacks, not one isolated variable. The shared invariant is
the **logical struct and the mutate-and-verify semantics**, not the wire bytes.
A run reports three whole-stack round-trip latencies; it does not attribute the
difference to transport vs codec (that would need a fuller matrix, out of scope).

- **`sbe_udp`** (Rust): a `connect()`ed UDP socket issues one datagram at a time
  through the shared measure loop; the responder bounces a mutated datagram back.
  A read timeout is a hard error, never a retransmit (mirrors `network-rtt-udp`).
- **`grpc`** (Go): a single unary RPC `Roundtrip(Payload) returns (Payload)` is
  the round trip. gRPC owns serialization (protobuf), framing (HTTP/2), and the
  call; the server handler increments the hop field and returns. One connection,
  one call outstanding at a time.
- **`bebop_tcp`** (Go): a strict TCP ping-pong (one request outstanding), bebop
  `MarshalBebopTo`/`UnmarshalBebop` on both ends, `TCP_NODELAY` set (mirrors
  `network-rtt-tcp`'s connection handling).

## The payload — one flat ~250-byte struct, three schemas

A purpose-built **flat** record (no repeating group), expressed once each as an
SBE XML schema (Rust), a bebop `.bop` schema (Go), and a proto3 schema (Go/gRPC).
Generated codecs are **committed** (the `serialization` precedent — bench hosts
need no generators at run time).

### Logical fields

| field       | type        | role                                              |
|-------------|-------------|---------------------------------------------------|
| `hop`       | uint32      | **mutated**: responder returns `hop + 1`          |
| `seq`       | uint64      | echoed unchanged; client verifies it is preserved |
| `timestamp` | int64       | scalar payload                                    |
| `order_id`  | uint64      | scalar payload                                    |
| `price`     | int64       | scalar payload                                    |
| `qty`       | int64       | scalar payload                                    |
| `symbol_id` | uint32      | scalar payload                                    |
| `account_id`| uint64      | scalar payload                                    |
| `venue_id`  | uint16      | scalar payload                                    |
| `side`      | uint8       | scalar payload                                    |
| `flags`     | uint8       | scalar payload                                    |
| `signature` | 32 bytes    | fixed-length blob                                 |
| `context`   | 152 bytes   | fixed-length blob (pads to the ~250-byte target)  |

**Wire sizes** land in the 200–300 byte band and differ per codec (reported, not
forced): SBE ≈ 252 B (8-byte message header + 244-byte fixed block); bebop ≈
252 B (length-prefixed `byte[]` for the two blobs, no header); protobuf ≈ 260–275
B (field tags + `bytes` length prefixes; `sfixed64`/`sfixed32` for the wide
scalars, as in the serialization protobuf cell, `uint32` for the byte-wide
fields). `signature`/`context` are fixed-length in the logical model; bebop and
protobuf encode them length-prefixed, which is fine — verification is by field
value, not byte identity across codecs.

### Deterministic builder + verification

- A shared **index-seeded builder** (splitmix64, no RNG, no wall clock — the
  project's byte-reproducibility discipline) builds request `i`. The blobs are
  filled deterministically from the index.
- **Round-trip verification** (the correctness anchor that proves real codec
  work, not an echo): after decoding the reply, the client asserts
  `resp.hop == req.hop + 1` **and** `resp.seq == req.seq`. A byte echo would fail
  the hop check; a corrupted reserialize would fail one of them.
- **Cross-language fairness anchor**: a golden FNV-style checksum over the folded
  fields (same mechanism as `serialization`) anchors the Rust and Go builders to
  the *same logical request* for a handful of indices. Golden values are
  generated from the Rust builder and pasted into a Go test. This proves all
  three cells benchmark equivalent ~250-byte payloads even though their wire
  formats differ. It is a fairness check, not a wire-interop requirement (each
  cell's client and responder are the same language).

## Metrics

Each cell emits, aligned on `(focus_area, experiment, language, metric)`:

| metric        | unit    | what it captures                                        |
|---------------|---------|---------------------------------------------------------|
| `rtt_p50`     | `ns`    | median full mutating round-trip latency                 |
| `rtt_p99`     | `ns`    | tail round-trip latency                                 |
| `rtt_mean`    | `ns`    | mean round-trip latency                                 |
| `encoded_bytes`| `bytes`| on-wire size of one encoded request (per-codec footprint)|

The `rtt_*` triple matches `network-rtt`'s metric shape exactly, so the
`tools/journal` CLI aligns these cells against the network-rtt cells for free.

## Harness & modes

The round-trip loop reuses the existing measure/emit machinery, generalized to
take the focus-area string (today `emit_rtt`/`EmitRTT` hardcode
`"network-rtt"`):

- **Go**: `bench.Measure(cfg, roundTrip)` already times an arbitrary
  `func() error` and preallocates its sample buffer; reuse it. Add
  `bench.EmitRoundtrip(focusArea, experiment, samples)` (or parametrize
  `EmitRTT`) so `rpc-roundtrip` cells emit under their own focus area.
- **Rust**: `bench_common::measure::run(cfg, round_trip)` and `emit_rtt` are the
  analogs; generalize `emit_rtt` to take the focus area.

Each cell supplies a `roundTrip` closure that serializes into a **reused** buffer,
sends, receives into a reused buffer, deserializes, and verifies — **no
allocation on the timed path** (buffers built before timing). For gRPC the
"closure" is one unary call on a persistent connection; protobuf message reuse
follows the grpc-go idioms (the timed path still allocates inside gRPC, which is
part of the stack's honest cost and noted in RESULTS).

**Three modes**, identical contract to `network-rtt`:
- `loopback` (default): in-process responder on an ephemeral loopback port +
  client; emits the result lines. **Local-dev fitness check only, never a
  reported result** (real RPC round-trip is cross-host).
- `server`: bind the responder on `0.0.0.0` at this experiment's port, serve
  until killed, emit nothing to stdout (logs to stderr).
- `client`: connect to `RPC_HOST` on this experiment's port, measure, emit.

## Env contract — `RPC_*`

A new prefix, consistent with every focus area owning its contract (`SER_*`,
`FSW_*`, `TH_*`, `SMRC_*`); `network-rtt` keeps `RTT_*`:

| var              | default | meaning                                    |
|------------------|---------|--------------------------------------------|
| `RPC_MODE`       | loopback| `loopback` \| `server` \| `client`         |
| `RPC_HOST`       | (unset) | responder address (required in client mode)|
| `RPC_UDP_PORT`   | 9200    | `sbe_udp` datagram port                     |
| `RPC_TCP_PORT`   | 9201    | `bebop_tcp` port                            |
| `RPC_GRPC_PORT`  | 9202    | `grpc` HTTP/2 port                          |
| `RPC_WARMUP`     | 10000   | discarded round trips                       |
| `RPC_ITERATIONS` | 100000  | timed round trips                           |

The payload size is fixed by the struct schema, so there is no
`RPC_PAYLOAD_BYTES` knob (unlike `network-rtt`). Malformed values hard-error,
per the other configs.

## New dependency

`google.golang.org/grpc` (+ `protoc-gen-go-grpc` at regen time) enters the Go
module — a large transitive tree (HTTP/2, genproto), but explicitly required and
contained to the `grpc` cell. `github.com/200sc/bebop` v0.6.2 and
`google.golang.org/protobuf` are already present from the `serialization` work.
The committed generated gRPC service code means bench hosts need no protoc.

## Layout & build integration

Rust (Cargo workspace):
```
rust/rpc-roundtrip/
  common/       # rpc-roundtrip-common: struct model, index-seeded builder, checksum, RpcConfig (RPC_* parse)
  sbe_udp/      # rpc-roundtrip-sbe_udp binary (+ build.rs → sbe_gen::generate_to over rpc_payload.xml)
```
`RpcConfig::from_env` parses the `RPC_*` contract (new, alongside the existing
`network-rtt` `Config`); `emit_rtt` generalized to a focus-area arg.

Go (single module):
```
go/internal/rpcpayload/
  schema/rpc_payload.bop        # bebop schema
  schema/rpc_payload.proto      # proto3 message + Roundtrip service
  payloadbop/                   # committed 200sc/bebop codegen
  payloadpb/                    # committed protoc-gen-go + protoc-gen-go-grpc codegen
  regen-payloadbop.sh
  regen-payloadpb.sh            # runs both go and go-grpc plugins
  rpcpayload.go                 # Record model, BuildRecord, Checksum
  *_test.go
go/cmd/rpc-roundtrip-grpc/
go/cmd/rpc-roundtrip-bebop_tcp/
```
`go/internal/bench/rpc.go` holds `RpcConfig` + `LoadRpcConfig` (RPC_* parse,
loopback/server/client mode) and the focus-area-parametrized emit helper.

bench-infra:
- Three `cross_host` rows in `group_vars/all.yml`:
  `- { focus_area: rpc-roundtrip, experiment: sbe_udp,    kind: cross_host, languages: [rust] }`
  `- { focus_area: rpc-roundtrip, experiment: grpc,       kind: cross_host, languages: [go] }`
  `- { focus_area: rpc-roundtrip, experiment: bebop_tcp,  kind: cross_host, languages: [go] }`
- New `rpc_*` params (warmup/iterations/ports) in `all.yml`.
- `cross_host.yml` exports the `RPC_*` contract (host, ports, warmup, iterations)
  alongside the existing `RTT_*` exports, on both the node1 responder and node0
  client tasks. `run_bench.sh` adds `rpc-roundtrip` to its focus-area case and
  exports the `RPC_*` defaults.
- The responder-kill `pkill` patterns in `cross_host.yml`'s `always` block cover
  `rpc-roundtrip-<exp>` artifacts (the existing patterns are prefix-based on
  `<focus_area>-<experiment>`, so they already match).

## Tests

- **Cross-language golden checksum**: Go test asserts the builder+fold reproduces
  hardcoded Rust-generated checksums for a few indices (fairness anchor).
- **Round-trip verification, per cell**: in-process (loopback) round trip asserts
  `resp.hop == req.hop + 1` and `resp.seq == req.seq`, and that all other fields
  are preserved.
- **Encoded-bytes sanity**: each codec encodes to within the 200–300 byte band.
- **Builder determinism**: same index → identical record; different indices differ.
- Green `cargo build/test/clippy/fmt` and `go build/vet/test`.

## Result-contract compliance

- stdout carries only result lines; gRPC/codec logs go to stderr.
- One line per metric; every line carries `experiment`, the right `language`, and
  `focus_area: "rpc-roundtrip"`.
- Only real cross-host AWS runs are journaled; loopback is a fitness check only.
  The reported result is cross-host (node0 client ↔ node1 responder), per the
  project's network-perf discipline.

## Non-goals

- No Java cell.
- No isolated transport-vs-codec matrix — the three combos are the chosen
  whole-stack comparison. (A `rust+tcp+sbe` or `go+udp+bebop` cell to isolate a
  variable is a possible future experiment, not this spec.)
- No reliability/retransmit layer over UDP (a lost datagram is a hard error).
- No streaming or multiplexed gRPC (unary, one call outstanding at a time).
- No `RPC_PAYLOAD_BYTES` knob — the struct schema fixes the size.
