# 20260720T120209Z-79706160a45d

- commit: 79706160a45da4cd9253f78d43604790623d59fc dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1019-aws
- params: payload=64B warmup=10000 iterations=100000

## What changed
rpc-roundtrip cross-host on AWS: mutating serialize/send/deserialize round-trip across sbe_udp (Rust/UDP), grpc (Go), bebop_tcp (Go/TCP)

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### rpc-roundtrip / bebop_tcp

| language | encoded_bytes (bytes) | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|---|
| go | 252 | 35652.8 | 34638 | 57060 |

### rpc-roundtrip / grpc

| language | encoded_bytes (bytes) | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|---|
| go | 247 | 130263.3 | 126100 | 189301 |

### rpc-roundtrip / sbe_udp

| language | encoded_bytes (bytes) | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|---|
| rust | 252 | 26770.9 | 26101 | 38520 |

## Hypothesis
First cross-host run of the `rpc-roundtrip` focus area — a mutating round-trip
(client serializes → sends → responder deserializes + increments hop +
reserializes → sends back → client deserializes and verifies hop+1/seq). Three
whole-stack cells that differ in both transport and codec, so we expected three
distinct round-trip latencies rather than an isolated variable: the two
hand-rolled datagram/stream cells (sbe_udp, bebop_tcp) close to raw network RTT,
and gRPC materially higher for its HTTP/2 + unary-call machinery. Sizes ~252 B
(SBE/bebop) and ~247–260 B (protobuf).

## Observations
- Ordering as expected: **sbe_udp fastest** (p50 26.1 µs, mean 26.8 µs),
  **bebop_tcp** next (p50 34.6 µs, mean 35.7 µs, ~1.3× sbe_udp), **grpc**
  far behind (p50 126.1 µs, mean 130.3 µs — ~4.8× sbe_udp, ~3.7× bebop_tcp).
  The gRPC gap is the HTTP/2 framing + unary-call + reflection-based protobuf
  path, exactly the whole-stack cost this focus area exists to expose.
- Tail behavior: grpc p99 189.3 µs (1.45× its p50) is the widest tail; sbe_udp
  and bebop_tcp tails are tighter (1.48× and 1.65× p50 respectively). UDP had no
  datagram loss at this rate (a lost datagram is a hard error; the run completed
  cleanly).
- Sizes landed exactly as designed: sbe_udp 252 B, bebop_tcp 252 B, grpc 247 B.
- **Caveats for cross-cell reading** (per the branch's final review): grpc's
  `encoded_bytes` = 247 reflects proto3 omitting the two zero-valued fields
  (hop=0, seq=0) of the index-0 request; a non-zero request encodes ~260–275 B.
  And bebop_tcp's timed path carries the bebop safe-API decode allocation
  (`make` for the two blob slices per decode) — the honest cost of that API,
  parallel to gRPC's internal allocation — whereas sbe_udp mutates hop in place
  and is genuinely zero-alloc on the timed path. So sbe_udp's lead is partly a
  zero-copy-vs-owned-decode story, not transport alone.
- First run of this focus area, so no baseline to compare against; these become
  the reference. (`params` line shows the RTT_* defaults from the manifest;
  the actual knobs were the RPC_* defaults — warmup 10000, iterations 100000.)
