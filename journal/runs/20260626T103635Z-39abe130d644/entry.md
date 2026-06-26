# 20260626T103635Z-39abe130d644

- commit: 39abe130d6440a81f3d324f9e99afabf046ea665 dirty
- instance: c6id.2xlarge, 8 vCPU, kernel 6.17.0-1017-aws
- params: payload=64B warmup=10000 iterations=100000
- placement: VERIFIED both nodes in one `cluster` placement group
  (`hi-perf-cmp-bench-pg`), single AZ us-east-1a — instances bind
  `placement_group = aws_placement_group.bench.id` and were created after the PG.

## What changed
First real cross-host AWS run (c6id.2xlarge x2, us-east-1, same-AZ cluster placement group): network-rtt tcp/udp/quic across rust/go/java

## Results

Per-cell values from this run (placeholder/stub cells omitted).

### network-rtt / quic

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 97342.8 | 92005 | 137507 |
| java | 162648.9 | 160841 | 195582 |
| rust | 65055.1 | 64625 | 100867 |

### network-rtt / tcp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 39219.9 | 37760 | 68607 |
| java | 35538.5 | 35156 | 43960 |
| rust | 36415.8 | 36025 | 45656 |

### network-rtt / udp

| language | rtt_mean (ns) | rtt_p50 (ns) | rtt_p99 (ns) |
|---|---|---|---|
| go | 36428.5 | 35993 | 45876 |
| java | 35111.1 | 34740 | 43505 |
| rust | 36903.0 | 35916 | 49931 |

## Hypothesis
On a real two-host network the physical link RTT should dominate, so the
busy-poll wins that mattered on loopback (6-14us p50 for tcp/udp) should largely
disappear, and QUIC's overhead — huge as a *multiple* on loopback (8-28x) —
should shrink to a small additive cost once a fixed network RTT is added to
every transport.

## Observations
(Numbers in the **Results** section above.)

- **The network RTT dominates and tcp ~= udp (~35-38us) across all languages.**
  The link/kernel round-trip (~35us) swamps the transport micro-differences, so
  the busy-poll optimizations that drove the loopback ranking are invisible here
  — the loopback 6-14us p50s were a kernel-park artifact, not a network number.
  This is exactly why loopback is local fitness only, never reported.
- **QUIC is now +30us (rust) to +126us (java) over tcp/udp — a ~1.8x (rust) /
  2.6x (go) / 4.6x (java) multiple**, far below the loopback 8-28x. QUIC's
  per-RTT crypto + userspace protocol cost is a fixed adder; against a real
  ~35us network RTT it's a modest premium for rust/go, but Kwik's ~125us adder
  makes java QUIC the clear outlier (consistent with the loopback finding that
  Kwik's per-RTT path is ~4x quinn).
- **Takeaway for the SMR hot path:** over a real network, transport choice
  between tcp/udp is a wash; QUIC costs ~2x for rust/go (its features may justify
  it) and is expensive in Kwik/java. This is the baseline; future runs compare
  against it (`journal compare`).
