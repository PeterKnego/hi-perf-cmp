# Results so far

A human-readable summary of the experiments run to date and what they showed.
All numbers come from **real AWS benchmark runs** recorded in [`journal/`](../journal/INDEX.md)
(never loopback/dev-box smoke runs). Raw per-metric data lives in each run's
`results.jsonl`; the current reference values are in `journal/baselines.json`.

**Test rig (all runs):** `c6id.2xlarge` (8 vCPU, local NVMe), us-east-1,
same-AZ cluster placement group for the 2-node network runs. Uniform harness:
64-byte payload, 10,000 warmup + 100,000 measured iterations, identical stats
code in each language.

**Runs recorded** (June 26 – July 22, 2026):

| run | what it measured |
|---|---|
| [20260626T103635Z](../journal/runs/20260626T103635Z-39abe130d644/entry.md) | First cross-host `network-rtt` run (tcp/udp/quic × rust/go/java) |
| [20260626T213457Z](../journal/runs/20260626T213457Z-deef392a8445/entry.md) | First `filesystem-write` run on local NVMe (fsync/fdatasync/prealloc/batch) |
| [20260627T071950Z](../journal/runs/20260627T071950Z-07a4b9a872fc/entry.md) | First `thread-handoff` run (spin/condvar/channel/ring); network + filesystem re-measured |
| [20260627T193417Z](../journal/runs/20260627T193417Z-003926ca6c91/entry.md) | Optimized SPSC ring (Rust + Go); full matrix re-measured |
| [20260713T152911Z](../journal/runs/20260713T152911Z-23b9778538e9/entry.md) | First `serialization` run (sbe_gen/aeron_sbe/bincode); full matrix re-measured — **current baseline** |
| [20260716T100733Z](../journal/runs/20260716T100733Z-16a158ef9fd2/entry.md) | Go `serialization` cells added (`bebop`, `protobuf`) alongside the Rust codecs (scoped run) |
| [20260720T120209Z](../journal/runs/20260720T120209Z-79706160a45d/entry.md) | First `rpc-roundtrip` run (sbe_udp/grpc/bebop_tcp) — mutating cross-host round-trip (scoped run) |
| [20260722T131646Z](../journal/runs/20260722T131646Z-cd050b70cc78/entry.md) | `serialization` grid extended with Go SBE flyweight (`aeron_sbe`), Go SBE struct (`sbe_struct`), and `flatbuffers` (scoped run) |

Unless noted, tables below show the **current baseline** run (20260713T152911Z). The
two July 16 / July 20 runs are **scoped** (one focus area each, not a full-matrix
re-measure), so the baseline pointer is unchanged; their sections cite their own run.

---

## network-rtt — leader↔follower round trip (cross-host)

Strict ping-pong (one request outstanding) between two hosts, measuring the full
leader→follower→leader RTT for a 64 B message over TCP (`TCP_NODELAY`),
connected UDP, and QUIC (long-lived bidi stream).

| transport | rust p50 | go p50 | java p50 | rust p99 | go p99 | java p99 |
|---|---|---|---|---|---|---|
| tcp | 35.8 µs | 39.2 µs | 34.8 µs | 45.2 µs | 51.4 µs | 44.1 µs |
| udp | 35.0 µs | 35.8 µs | 34.3 µs | 45.3 µs | 46.7 µs | 43.2 µs |
| quic | 69.2 µs | 94.2 µs | 160.7 µs | 117.2 µs | 141.3 µs | 195.9 µs |

**What we learned:**

- **On a real network, TCP ≈ UDP and the languages are a wash** (~35 µs p50
  everywhere, 34–39 µs). The physical link + kernel round trip dominates;
  the large per-language differences seen on loopback were kernel-parking
  artifacts, which is exactly why loopback numbers are never reported.
- **QUIC carries a fixed per-RTT premium**: roughly 2× TCP for Rust (quinn) and
  Go (quic-go), but ~5× for Java — the Kwik library adds ~125 µs per round trip
  and is the clear outlier. For an SMR hot path, tcp-vs-udp is not a
  performance decision; adopting QUIC costs ~2× RTT in Rust/Go and is expensive
  in Java today.
- Absolute RTTs moved ~15–25 % between fleet instantiations (e.g. tcp/rust p50
  36.0 → 34.6 → 28.5 → 35.8 µs across the four runs) with unchanged code —
  that's cross-instance cloud variance, and it's why comparisons are made
  against a journaled baseline rather than across arbitrary runs.

## filesystem-write — durable command-log appends (local NVMe)

Appending 64 B records to a log with durability, four strategies forming a
ladder: `fsync` (write + fsync each record), `fdatasync` (data-only sync),
`prealloc` (preallocated + fdatasync, no metadata updates), and `batch`
(group-commit: many records per sync).

| experiment | rust ops/s | go ops/s | java ops/s | sync p50 (rust) |
|---|---|---|---|---|
| fsync | 7,814 | 7,983 | 7,915 | 123 µs |
| fdatasync | 7,633 | 7,666 | 7,947 | 123 µs |
| prealloc | 25,749 | 25,408 | 25,392 | 36 µs |
| batch | 388,247 | 360,670 | 348,310 | 42 µs |

(The Rust-prealloc slow-tail anomaly of the prior baseline did not recur — all
three languages sit at ~25.4–25.7 K ops/s this run.)

**What we learned:**

- **The strategy matters ~50×; the language barely matters.** Per-record
  fsync/fdatasync costs ~125–135 µs of device sync time and caps out at
  ~7–8 K appends/s in every language. Preallocating the file (so syncs don't
  touch metadata) cuts sync latency to ~37 µs (~25 K ops/s). Group-commit
  batching amortizes the sync and reaches **~350–390 K durable appends/s** —
  this is the technique an SMR log wants.
- All three languages sit within a few percent of each other in every cell:
  the NVMe device, not the runtime, is the bottleneck.
- One anomaly on the first run (Java fdatasync slower than its own fsync) did
  not reproduce — noted in the journal as a single-run JIT/GC artifact.

## thread-handoff — thread-to-thread data passing (single host)

Ping-pong handoff of a value between two threads, measuring round-trip latency
for `spin` (busy-wait), `condvar` (mutex + condition variable park/unpark), and
`channel` (each language's standard channel), plus sustained **throughput** for
`ring` (pipelined SPSC ring buffer).

| experiment | rust | go | java |
|---|---|---|---|
| spin p50 | 256 ns | 202 ns | 298 ns |
| condvar p50 | 281 ns | 389 ns | 287 ns |
| channel p50 | 394 ns | 323 ns | 451 ns (mean 6.8 µs) |
| ring throughput | **421.6 M ops/s** | 43.2 M ops/s | 7.8 M ops/s |

> **Regime note (condvar/channel).** These p50s are from a run whose Rust/Java
> threads mostly **did not park** — the handoff stayed hot at ~280–450 ns, on
> par with Go. When OS threads genuinely sleep, the cost is ~80× higher: the
> prior baseline (20260627) measured Rust/Java condvar/channel at **~22–24 µs**
> (futex syscall + scheduler round trip) vs Go's ~300–380 ns userspace park.
> Whether threads park is scheduler/load-sensitive, so treat these two cells as
> the *no-park* floor and the ~22 µs figure as the *parking* cost — the number
> the focus area exists to expose. Java's channel still shows the split within
> this run (p50 451 ns, mean 6.8 µs, p99 23.7 µs).

**What we learned:**

- **Busy-wait spin is a ~200–300 ns floor everywhere** — with no scheduler
  involved, the three runtimes converge (Go ~200 ns, Rust/Java ~260–300 ns).
- **The sleep/wakeup cost is bimodal, and this run mostly caught the no-park
  side.** condvar/channel handoff only pays the OS-park price when the woken
  thread actually sleeps. Here the Rust/Java threads stayed hot, so those
  handoffs ran in ~280–450 ns — on par with Go. **When they do park, Go is
  ~50–60× cheaper**: the prior journaled baseline measured Rust/Java at
  ~22–24 µs (futex syscall + kernel scheduler round trip) against Go's
  ~300–380 ns userspace goroutine park. That parking penalty — not the no-park
  floor — is the central sleep/wakeup story the focus area was built to expose;
  whether a given run triggers it is scheduler/load-sensitive.
- **Java's channel stays visibly bimodal even in this run**: `SynchronousQueue`
  hands off without parking at the median (p50 451 ns) but parks on a heavy
  tail (p99 23.7 µs), so its mean (~6.8 µs) sits far above its median — the
  parking cost leaking through, and why the median alone misleads for
  wakeup-sensitive paths. Rust's `mpsc` rendezvous parked every time in the
  prior run but mostly stayed hot here (p50 394 ns, p99 1.1 µs).
- **The SPSC ring optimization (cache-line padding + LMAX-style cached
  opposite index) was the project's first optimization win**, graduated via
  the journal: Rust 28.1 M → **421.6 M ops/s (+1400 %)**, Go 9.8 M → **43.2 M
  ops/s (+341 %)**. Java kept its baseline (~7.8 M); the same pattern regressed
  its JIT'd `AtomicLong` path and was discarded.
- **Against an external yardstick** (same box, median-of-5): the optimized Rust
  ring hit ~367.6 M ops/s vs ~148.0 M for the `disruptor` crate v4.3 (BusySpin
  SPSC) — ~2.5× the full Disruptor framework for a bare `u64` handoff, since it
  skips handler dispatch, sequence barriers, and multi-consumer machinery.
  (Follow-up burst-mode comparison found disruptor faster at large bursts;
  both far exceed standard channels.)

## serialization — command-log record encode/decode (single host)

Encode and decode of one ~500 B state-machine-replication journal record — a
mixed block of fixed fields plus a repeating group of variable-length command
payloads — across three Rust codecs: `sbe_gen` (zero-copy SBE via the
`zerocopy` crate), `aeron_sbe` (the reference real-logic `sbe-tool` emitting
Rust — the same codec Aeron itself uses), and `bincode` (serde + bincode v2,
the ergonomic derive baseline). Five Go codecs were added later, measuring the
same logical record: `bebop` (200sc/bebop safe API), `protobuf` (canonical
google.golang.org/protobuf, `sfixed`-typed schema), `aeron_sbe` (the SBE tool's
zero-copy Golang **flyweight** — the Go twin of the Rust `aeron_sbe`),
`sbe_struct` (the same tool's default owned-struct Golang codec), and
`flatbuffers` (zero-copy accessors). The harness
encodes a stream of records into an in-memory journal then replays (decodes)
them, timing each operation and — via a counting global allocator (Rust) /
`ReadMemStats` TotalAlloc delta (Go) — reporting heap bytes allocated per decode.
100,000 measured iterations per codec.

Rust codecs (baseline run 20260713T152911Z):

| codec | encode p50 | decode p50 | decode p99 | encoded bytes | decode alloc |
|---|---|---|---|---|---|
| sbe_gen   | 46 ns | 408 ns | 440 ns | 502 | **0 B** |
| aeron_sbe | 57 ns | 409 ns | 443 ns | 502 | **0 B** |
| bincode   | 85 ns | 947 ns | 1034 ns | 482 | **536 B** |

Go codecs (run 20260722T131646Z — five codecs, two of them zero-copy):

| codec | encode p50 | decode p50 | decode p99 | encoded bytes | decode alloc |
|---|---|---|---|---|---|
| aeron_sbe (flyweight) | 126 ns | 892 ns  | 958 ns  | 502 | **0 B**   |
| flatbuffers           | 572 ns | 1041 ns | 1295 ns | 608 | **0 B**   |
| bebop                 | 83 ns  | 1063 ns | 4013 ns | 494 | **544 B** |
| sbe_struct            | 338 ns | 1450 ns | 5229 ns | 502 | **544 B** |
| protobuf              | 480 ns | 1718 ns | 5701 ns | 514 | **888 B** |

`aeron_sbe` (go) is the SBE tool's zero-copy Golang **flyweight** (the Go twin of
the Rust `aeron_sbe`); `sbe_struct` is the same tool's default owned-struct
Golang codec (same 502 B wire, materializes on decode); `flatbuffers` reads via
zero-copy accessors.

p50/p99 in ns; a `_mean` is also emitted per op (Rust decode means: sbe_gen 415,
aeron_sbe 416, bincode 961 ns; Go decode means: aeron_sbe 909, flatbuffers 1063,
bebop 1265, sbe_struct 1677, protobuf 1988 ns). Uniform record builder and
iteration count across codecs and languages. The Rust table is the 20260713
baseline; its cells re-measured within noise on the 20260722 run (sbe_gen decode
408, aeron_sbe 409, bincode 933 ns), so Go-vs-Rust reads directly.

**What we learned:**

- **Zero-copy decode wins on both latency and memory.** Both SBE codecs decode
  by viewing the buffer in place — **0 bytes allocated per decode** — and
  materialize every field in ~408 ns; `bincode` rebuilds an owned struct (the
  record plus its `Vec` of command blobs) every decode, costing **536 B and
  ~947 ns, ~2.3× the SBE decode**. This is a structural difference (proven by
  the counting allocator, not a tuning result), and over a journal replay of
  millions of records the allocation is the dominant cost: SBE adds no
  allocator pressure, `bincode` adds one owned object graph per record.
- **The two SBE toolchains are wire-identical and perform within noise.**
  `sbe_gen` and `aeron_sbe` consume the same SBE schema and produce
  **byte-for-byte identical** encoded bodies (conformance test over 64
  records); decode is tied (408 vs 409 ns) and `sbe_gen`'s encode is marginally
  cheaper (46 vs 57 ns p50). The choice between the pure-Rust generator and the
  reference Java tool's Rust output is ergonomic, not a performance or
  wire-format decision.
- **`bincode` is the ergonomic baseline, not the fast path — but it is compact.**
  Its record is actually the smallest on the wire (482 B vs SBE's 502 B)
  because bincode varint-encodes integers; the cost is paid on the way back, in
  the owned-graph allocation and ~2.3× decode time.
- **The gap is a level shift, not a tail.** On the quiet benchmark host every
  codec's decode p99 sits under 10 % above its p50 (SBE ~1.08×, bincode
  ~1.09×) — the SBE↔bincode difference is the constant per-record allocation
  cost at every percentile, not an allocator tail. Encode is so cheap
  (~50–100 ns) that its p99 (~340–350 ns for all three) is dominated by timer
  and scheduling noise rather than the codec; decode is the meaningful
  differentiator.
- **Go has zero-copy decode too, and it splits the field into two tiers.** The
  two zero-copy Go cells — SBE `aeron_sbe` (flyweight) and `flatbuffers` — decode
  at **0 B allocated**, versus 544–888 B for the owned-decode cells (`bebop`,
  `sbe_struct`, `protobuf`). The allocation axis, not the language, is the primary
  divider: the zero-copy cells also have tight decode tails (p99 ~1.07–1.24× p50),
  while every owned-decode Go cell carries a GC-visible tail (p99 3.5–4.9× p50,
  up to 5.7 µs) — the honest cost of rebuilding an owned object graph per record
  on a garbage-collected runtime.
- **FlatBuffers is zero-copy but NOT the fastest decode here — zero-copy ≠ fast
  reads.** FlatBuffers decodes at 1041 ns, *slower* than the Go SBE flyweight
  (892 ns) and ~2.5× the Rust SBE cells (~408 ns), despite all four allocating
  0 B. The difference is field access: SBE reads fixed byte offsets, while
  FlatBuffers chases a vtable + offset indirection per field. Zero-copy removes
  the *allocation*, not the per-field read cost. FlatBuffers also pays the most on
  encode (572 ns — its Builder constructs the buffer bottom-up) and the largest
  wire (608 B, from vtables + offsets). (The `kcchu/buffer-benchmarks` "fastest
  decode" claim did not reproduce on this record/harness.)
- **Same SBE tool, two Go modes: flyweight ~1.6× faster to decode than struct,
  and zero-alloc.** `aeron_sbe` (flyweight) and `sbe_struct` are the identical
  real-logic sbe-tool output — flyweight vs its default owned-struct codegen —
  over the byte-identical 502 B wire. The flyweight decodes in 892 ns at 0 B; the
  owned `sbe_struct` streams through its `SbeGoMarshaller` into a fresh struct at
  1450 ns and 544 B. The choice of codegen mode, not the format, sets the cost.
- **The Rust SBE flyweight is ~2.2× faster than the Go one at identical 0-alloc
  and identical 502 B wire.** Rust `aeron_sbe`/`sbe_gen` decode at ~408 ns vs the
  Go flyweight's 892 ns — a pure language/codegen gap (fixed-offset reads with no
  bounds-check overhead), since both are zero-copy over the same bytes. Among the
  owned-decode Go cells, `bebop` (1063 ns / 544 B) leads, then `sbe_struct`
  (1450 ns / 544 B), then `protobuf` (1718 ns / 888 B); `protobuf`'s 514 B wire
  uses `sfixed` fields deliberately (full-width ids would otherwise pay varint
  pathology).

## rpc-roundtrip — mutating request/response across whole stacks (cross-host)

A new focus area that fuses `serialization` and `network-rtt`: unlike the byte
echo in `network-rtt`, the responder here does **real codec work** — it
deserializes the request, increments a `hop` field, and re-serializes the reply.
The client serializes a ~250 B record, sends it cross-host (node0→node1),
receives the mutated reply, deserializes it, and verifies `hop+1` / `seq`
preserved. Three cells compare whole realistic stacks (transport **and** codec
differ per cell, by design — this is not an isolated-variable matrix): `sbe_udp`
(Rust, hand-rolled UDP + zero-copy SBE), `bebop_tcp` (Go, length-prefixed TCP +
bebop safe API), `grpc` (Go, unary gRPC over HTTP/2 + protobuf). Run
20260720T120209Z; 100,000 measured iterations.

| cell | stack | rtt p50 | rtt mean | rtt p99 | encoded bytes |
|---|---|---|---|---|---|
| sbe_udp   | Rust · UDP · zero-copy SBE   | 26.1 µs  | 26.8 µs  | 38.5 µs  | 252 |
| bebop_tcp | Go · TCP · bebop            | 34.6 µs  | 35.7 µs  | 57.1 µs  | 252 |
| grpc      | Go · HTTP/2 · protobuf      | 126.1 µs | 130.3 µs | 189.3 µs | 247 |

**What we learned:**

- **Full gRPC costs ~4.8× a hand-rolled zero-copy datagram round-trip.** `grpc`
  round-trips a mutate-and-return in ~126 µs p50 vs `sbe_udp`'s ~26 µs — the
  HTTP/2 framing, unary-call machinery, and reflection-based protobuf marshalling
  are the price of the framework, exactly the whole-stack overhead this focus
  area exists to surface. `bebop_tcp` sits between them (~35 µs, ~1.3× sbe_udp):
  a plain TCP ping-pong with a fast codec is close to raw network RTT.
- **The gRPC tail is the widest.** Its p99 is 189 µs (~1.5× its p50); the two
  hand-rolled cells stay tighter (sbe_udp 1.48×, bebop_tcp 1.65× p50). Read
  against the `network-rtt` baseline (~35 µs TCP p50, byte echo), `bebop_tcp`'s
  ~35 µs shows the added encode+decode+mutate work is nearly free at this size,
  while gRPC's stack dominates the number.
- **Read the sizes and the sbe_udp lead honestly.** `grpc`'s 247 B reflects
  proto3 omitting the two zero-valued fields (`hop`/`seq`) of the index-0
  request; a non-zero request encodes ~260–275 B. And `sbe_udp`'s lead is partly
  a zero-copy story, not transport alone: it mutates `hop` in place and is
  genuinely zero-allocation on the timed path, whereas `bebop_tcp` pays the
  bebop safe-API decode allocation every round trip (as the `serialization`
  section quantifies) and gRPC allocates throughout its call path.
- First run of this focus area — these are the reference values; no prior run to
  compare against.

## shared-memory-ipc — planned

Scaffolded in Rust only (`spsc`, `mpsc`: real cross-process IPC over a
`/dev/shm` mapping with peer-death detection). No Go/Java artifacts or
bench-infra rows yet, so no measured results — it is not yet a cross-language
cell.

---

## Regressions

None confirmed. `journal compare` has flagged cells across runs, but every flag
so far traced to cross-instance cloud variance on code-identical cells (flags
moved in both directions); [`journal/REGRESSIONS.md`](../journal/REGRESSIONS.md)
remains empty.

## Big-picture takeaways for the SMR hot path

1. **Replication transport:** TCP or UDP are equivalent (~28–31 µs cross-host
   RTT, any language); QUIC costs ~2× in Rust/Go and ~5× in Java.
2. **Log durability:** don't sync per record — group-commit batching turns
   ~7 K durable appends/s into ~350–390 K, in every language.
3. **In-process handoff:** if threads may sleep, Go's runtime is ~50× cheaper
   at wakeup than OS-thread parking in Rust/Java **when parking actually
   happens** (~22 µs vs ~380 ns; whether it triggers is scheduler-sensitive);
   if you can spin, all three reach ~200–300 ns, and a well-tuned Rust SPSC
   ring moves 400 M+ ops/s.
4. **Language choice matters least where the kernel or device dominates**
   (network RTT, disk sync) and most where the runtime owns scheduling
   (thread parking) or the compiler owns the inner loop (SPSC ring).
5. **Log record codec:** for the replicated-log record itself, a zero-copy SBE
   codec decodes with **no per-record allocation** and **~2.3× faster**
   (~408 ns vs ~947 ns) than serde+bincode, which rebuilds an owned graph
   (536 B) every decode — the memory and latency difference an SMR replay path
   cares about. The two SBE toolchains (pure-Rust `sbe_gen` vs the reference
   Aeron `sbe-tool`) are wire-identical, so that choice is ergonomic. In Go the
   same split holds: the SBE-tool flyweight and FlatBuffers decode at 0 B, the
   owned codecs (bebop/protobuf/sbe-struct) at 544–888 B with GC tails. But
   **zero-copy is not automatically the fastest decode** — SBE's fixed-offset
   reads beat FlatBuffers' per-field vtable indirection (892 vs 1041 ns in Go,
   both 0-alloc), so for a hot replay path prefer a fixed-layout zero-copy codec
   (SBE) over an offset-table one (FlatBuffers).
6. **RPC framework vs hand-rolled stack:** for a mutating request/response on
   the replication path, a hand-rolled UDP + zero-copy SBE stack round-trips in
   ~26 µs; full gRPC (HTTP/2 + protobuf) costs **~4.8×** that (~126 µs) for its
   framing and call machinery, and a plain TCP + bebop stack lands in between
   (~35 µs). The transport+codec stack, not the language, sets the tier here.
