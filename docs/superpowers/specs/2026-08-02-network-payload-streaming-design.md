# network — Payload Ladder + Streaming Throughput — Design

**Date:** 2026-08-02 (updated 2026-08-03)
**Status:** Draft, reviewed 2026-08-03 against `ultima_cluster`'s `uc2_net`
transport and Aeron's flow-control model; the findings are folded in below
(§Findings from the uc2_net review) — the original ack rule was corrected as
a result. Committed on its own branch per the original handoff note. Not yet
approved for implementation.

## Purpose

Two motivations, one deliberately standalone and one external:

1. **The grid's own value.** Every `network-rtt` number so far is a single
   payload size (`RTT_PAYLOAD_BYTES` default 64). Payload size is a real
   dimension of transport behavior — syscall amortization, per-byte vs
   per-packet cost, MTU boundaries — and the grid currently cannot speak to
   it. This extension adds the dimension.

2. **Reference lines for `ultima_cluster`'s workload-envelope map.** UC is
   planning a payload-size envelope sweep of its full commit pipeline
   (`ultima_cluster/docs/superpowers/specs/2026-08-02-uc2-envelope-map-brief.md`)
   on the **same instance class this repo's fleet already uses**
   (`c6id.2xlarge`, `bench-infra/terraform/variables.tf`). What that map needs
   from here is the *isolated* ceiling: the sustained MB/s and pps a plain
   socket pair can move on this hardware, per datagram size, including how EC2
   network burst credits decay. UC below the isolated ceiling = UC software is
   the binder; UC at the ceiling = the NIC is. Measured reference lines beat
   the spec sheet the map would otherwise pre-register predictions against.
   Sharper still after the uc2_net review (§Findings): UC's real stream is
   windowed at the quorum-th STATUS advert with NAK repair, so its sustained
   goodput can never exceed this primitive's max-rate ceiling at the same
   datagram size — the gap between the two numbers is exactly the
   flow-control + repair overhead the net-decomp brief exists to attribute.

**Sequencing note:** this work is justified independently of
`ultima_cluster`'s net-decomp decide rule (that rule gates *technology* cells
— mmsg/GSO/AF_XDP/EFA — which remain out of scope here, §Out of scope). It
does not block UC's envelope map, but running first strengthens the map's
pre-registered predictions and the 2-node fleet here is the cheaper one.

## RTT is kept — the two axes

A question raised while scoping: RTT ping-pong was originally a simplistic
proxy for "log dispatch→commit"; does streaming supersede it?

**No. They are the two axes of one envelope, and both stay:**

- **RTT (one outstanding)** is the *latency floor* of a single
  leader→follower→leader hop, unloaded. For UC it is the physics floor under
  the WIRE metric of the net-decomp brief. Its known simplification is not
  ping-pong itself — it is that real dispatch→commit also contains fsync and
  pipelining, and *no isolated network cell should model those*: composition
  is the job of UC's own instruments (net-decomp measures the wire share of
  the real pipeline with a skew-free subtraction). As the wire-floor
  primitive, ping-pong is exactly right.
- **Streaming (windowed, pipelined)** is the *capacity* axis: sustained
  goodput and packet rate at a given datagram size. UC under load is a
  pipelined stream with a small ack stream back — this shape, not ping-pong,
  bounds UC's bytes-bound plateau.
- The windowed methodology unifies them: window = 1 degenerates to RTT, which
  doubles as a sanity cross-check between the new cells and the old.

The existing `network-rtt` `tcp`/`udp`/`quic` cells stay untouched at their
default 64 B — they are the historical journal baseline and remain the
cross-language latency comparison.

## Grid

Two additions:

### A. Payload ladder on `network-rtt` (nearly free)

Sweep `RTT_PAYLOAD_BYTES` over the existing cells. New experiments are named
with a size suffix — `udp_256`, `udp_1024`, `udp_1360`, `udp_8192`, and
likewise `tcp_*` (`quic_*` optional) — following the contract's own rule that
the variant lives in `experiment` (precedent: `filesystem-write`'s
`fsync`/`fdatasync`/`prealloc`/`batch` are variants-as-experiments). The bare
`udp`/`tcp`/`quic` names keep meaning "64 B" so the journal history stays
comparable.

*Rejected alternative:* extending the result contract with a `params` object.
Cleaner in principle, but it touches `tools/journal` and every alignment key
for a dimension the experiment name can carry; not worth the churn until a
second parameterized dimension appears.

Datagram sizes above ~1.4 KB require jumbo frames (below).

### B. New focus area: `network-throughput`

`network-rtt` cannot host a streaming mode without its name lying. New focus
area `network-throughput`, new artifacts `network-throughput-udp` (required,
std-only) and `network-throughput-tcp` (cheap, for reference); QUIC deferred.
Rust first; Go/Java later if wanted — the matrix is sparse-friendly.

**Methodology (udp):** one-way sequenced flood with a windowed ack stream
back — the sender keeps at most `STREAM_WINDOW` unacked datagrams in flight,
the receiver acks every `STREAM_ACK_EVERY` datagrams with the **highest
sequence seen** plus its **received-datagram count**. No retransmit — this is
a primitive measuring the socket + NIC, not a reliable transport; loss is
*reported, not repaired*. Sender-side pacing off by default (max-rate = the
ceiling measurement). A sender that hears no ack for a full timeout exits
FAILURE (unreachable-peer discipline), never hangs.

Window advancement is on highest-seen, **not** highest-contiguous — this is
load-bearing, not a nuance. The original draft of this spec said contiguous,
and contiguous-position windowing with no retransmit deadlocks at the first
hole: the ack frontier freezes at the gap, the window fills, and the sender
stalls forever — at max rate, within milliseconds. Aeron and `uc2_net` both
advance on contiguous progress and both can only afford to because NAK-driven
retransmit fills the hole (see §Findings); a no-retransmit primitive must
take the other fork. Ack loss is tolerated by construction: a later
highest-seen ack supersedes a lost one.

**Sizes:** {64, 256, 1024, 1360} at standard MTU; {2048, 4096, 8192} on the
jumbo arm. 1360 is UC's exact single-datagram frame ceiling at MTU 1408, kept
so the reference line lands on the number UC cares about.

**Durations:** loopback smoke seconds-scale; fleet rungs 60 s; plus **one
soak rung per MTU arm** (≥ 600 s, largest size, max rate) to walk through the
EC2 burst-credit window and find the *sustained* floor — the single number
UC's envelope map consumes most.

## Burst credits are the headline threat, and the instrument for them

`c6id.2xlarge` is "up to 12.5 Gbps" with a baseline near 3.1 Gbps. A 60 s rung
can ride burst credits entirely and report fiction as a sustained ceiling.
Three defenses, all mandatory for fleet runs:

1. **ENA allowance counters.** The orchestrator snapshots `ethtool -S <iface>`
   (`bw_out_allowance_exceeded`, `bw_in_allowance_exceeded`,
   `pps_allowance_exceeded`, `conntrack_allowance_exceeded`) before and after
   every rung and stores the deltas with the run artifacts. A rung with a
   nonzero bandwidth-allowance delta is **labelled throttled in its result
   `notes`** — its number is a burst observation, not a ceiling.
2. **Within-rung split.** The benchmark itself emits first-half and
   second-half goodput; the soak rung emits `goodput_sustained` computed over
   the final 25 % of the window. Divergence between halves is credit decay
   made visible in-band, without host access.
3. **Kernel drops separated from wire loss.** At max rate the dominant "loss"
   on a healthy path is the receiver's `SO_RCVBUF` overflowing — a kernel
   drop, not a NIC or wire property. Both benchmarks pin `SO_SNDBUF`/
   `SO_RCVBUF` explicitly (env below, values echoed in the result `notes`),
   and the orchestrator snapshots `netstat -su` (`UdpRcvbufErrors`,
   `UdpSndbufErrors`) deltas alongside the ethtool counters, so a rung's
   `loss_ppm` decomposes into kernel-side vs allowance-side loss instead of
   arriving uninterpretable. (Precedent: the uc2_net review found UC's own
   transport silently swallowing exactly this failure class; its new
   `SenderStats::send_errors` counter is the same separation on the UC side —
   §Findings.)

## Metrics (result-contract lines)

`network-rtt` ladder cells: the existing `rtt_p50`/`rtt_p99`/`rtt_mean` (ns),
unchanged shape, per suffixed experiment.

`network-throughput` cells, per experiment:

| metric | unit | meaning |
| --- | --- | --- |
| `goodput` | `bytes_per_sec` | payload bytes/s over the full window |
| `goodput_first_half` / `goodput_second_half` | `bytes_per_sec` | credit-decay split |
| `goodput_sustained` | `bytes_per_sec` | soak rungs only: final 25 % of window |
| `pps` | `ops_per_sec` | datagrams/s received |
| `loss_ppm` | `ppm` | `sender_sent − receiver_received` over the window, from the ack stream's received count — never inferred from unacked positions (flow control and loss accounting are separate channels, per §Findings) |
| `window` | `count` | the in-flight window used (config echo, aids alignment) |

stdout stays results-only; progress and the ethtool guidance stay on stderr /
in the orchestrator.

## Config

Env, following the `RTT_*` pattern (`bench-common/src/config.rs` gains a
sibling block):

- `STREAM_MODE` = `loopback` (default) | `server` | `client`
- `STREAM_HOST`, `STREAM_UDP_PORT`, `STREAM_TCP_PORT`
- `STREAM_PAYLOAD_BYTES` (default 1360), `STREAM_SECS` (default 10),
  `STREAM_WINDOW` (default 256), `STREAM_ACK_EVERY` (default 16)
- `STREAM_SO_SNDBUF` / `STREAM_SO_RCVBUF` — pinned socket buffer sizes
  (defaults chosen at implementation time and echoed in `notes`; never left
  to kernel defaults on fleet runs, per defense 3 above)
- Jumbo is host configuration, not benchmark configuration: an ansible toggle
  sets the interface MTU to 9001 on both hosts (AWS VPCs support jumbo
  intra-VPC; verify with a do-not-fragment probe before the arm runs, so a
  path-MTU blackhole fails loudly instead of as mystery loss).

## Error handling

As the existing cells: config errors and unreachable peers exit FAILURE with
stderr diagnostics. High loss is **data, not an error** — a max-rate UDP flood
is expected to drop; the loss number is part of the result. A jumbo-arm rung
whose DF-probe fails aborts that arm with a distinct message rather than
running and reporting blackholed loss.

## Testing

- Loopback smoke per cell: window sustained, sequence accounting exact (a
  deliberately-dropped datagram in a harness shim shows up in `loss_ppm`,
  nothing else moves).
- Contract-line validation against `docs/result-contract.md` (existing
  precedent), including the `notes` throttled label path.
- window=1 cross-check: `network-throughput-udp` at window 1 **and
  `STREAM_ACK_EVERY=1`** must agree with `network-rtt` `udp` within noise —
  the unification claim, tested rather than asserted. Two corrections from
  review: the ack-every override must be stated (window 1 with the default
  ack-every-16 would wait on an ack the receiver hasn't sent yet), and the
  check runs at 64 B only — RTT echoes the payload both ways while the
  stream sends payload one way and a small ack back, so at larger payloads
  the two are structurally different numbers, not a noise-band comparison.

## Infra & docs changes

- Ansible matrix rows for the new artifacts and the ladder sweep; the ethtool
  snapshot task around each fleet rung; the MTU-9001 toggle + DF probe.
- `docs/result-contract.md`: add `network-throughput` to the focus-area list
  and the new metrics to "Current state".
- `README.md` / `RESULTS.md`: new focus-area section; the RTT-vs-streaming
  axes explanation (condensed from this spec's section) so the journal reader
  knows why both exist.
- Soak rungs cost fleet minutes; the run budget note in `bench-infra` gains a
  line.

## Findings from the uc2_net review (2026-08-03)

The draft was reviewed against Aeron's UDP flow-control model and then against
`ultima_cluster`'s actual transport (`uc2_net`), which turned out to already
be deliberately Aeron-shaped: STATUS messages carrying contiguous position +
receiver window, NAK-driven retransmit with the log ring as the retransmit
buffer (plus journal-replay and snapshot tiers below it), and a sender limit
taken as the **quorum-th order statistic** over follower adverts rather than
Aeron's min — so a slow follower never stalls what the quorum could commit.
What that meant for this spec:

- **The ack rule was wrong and is now fixed.** The draft's contiguous-sequence
  ack + no-retransmit combination deadlocks at the first lost datagram. The
  coherent pairings are: contiguous windowing **with** retransmit (Aeron,
  uc2_net) or highest-seen windowing **without** (this primitive). §B now
  specifies highest-seen, with loss accounted from the receiver's
  received-count — never inferred from unacked positions.
- **Two uc2_net fixes fell out of the review** and landed on
  `ultima_cluster` branch `fix/uc2-net-window-seed-send-errs` (commit
  9816256): the bootstrap flow window was seeded as an absolute 64 KiB limit,
  silently stalling a leader promoted mid-stream until its first STATUS (now
  seeded relative to the recovered `sent` position); and all six outgoing
  datagram sites swallowed `send_to` errors, making local ENOBUFS-class
  drops indistinguishable from wire loss (now counted in
  `SenderStats::send_errors`, with a seeded `send_err_per_million` fault
  knob to test it). A sustained nonzero `send_errors` rate at plateau is
  UC's pre-registered trigger for explicit `SO_SNDBUF` sizing; this spec's
  cells pin their buffers up front (defense 3) so the primitive never needs
  that inference after the fact.
- **UC's transport needs no Aeron adoption** — where it diverges (quorum-paced
  flow control, receiver credit gated on archive-fsync drain, three-tier
  repair) it diverges ahead of Aeron for the SMR workload. The technology
  cells this spec defers (§Out of scope) stay gated on the net-decomp decide
  rule exactly as drafted.

## Out of scope

- **Technology cells** — `udp_mmsg`, `udp_gso`, `udp_busypoll`, `io_uring`,
  `af_xdp`, `efa_srd`. These are gated on the K/L decide rule of
  `ultima_cluster`'s net-decomp brief (its §7 routes the verdict here); an EFA
  cell additionally needs an EFA-capable instance type this fleet does not
  use. Adding them before that verdict is the pre-measurement optimization
  this repo exists to prevent.
- Reliable delivery, retransmit, or any UC-shaped protocol logic — primitives
  only; composition is UC's own instruments' job.
- QUIC streaming (dependency-heavy; revisit on demand).
- Cross-region / WAN arms.

## Handoff state

- Written 2026-08-02 alongside two `ultima_cluster` briefs:
  `2026-08-02-uc2-net-decomp-brief.md` (wire-share decomposition, gates the
  technology cells) and `2026-08-02-uc2-envelope-map-brief.md` (the consumer
  of this spec's `goodput_sustained` reference lines).
- Open decisions for the implementing session: ratify the experiment-suffix
  naming vs a contract `params` extension; exact ladder sizes; soak duration;
  whether `tcp` streaming ships in the first wave or follows.
- Suggested first wave: `network-throughput-udp` + the `network-rtt` udp
  ladder, Rust only, one fleet session (both MTU arms, one soak each).
