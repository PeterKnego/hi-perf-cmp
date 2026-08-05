# Results so far

A human-readable summary of the experiments run to date and what they showed.
All numbers come from **real AWS benchmark runs** recorded in [`journal/`](../journal/INDEX.md)
(never loopback/dev-box smoke runs). Raw per-metric data lives in each run's
`results.jsonl`; the current reference values are in `journal/baselines.json`.

**Test rig (all runs):** `c6id.2xlarge` (8 vCPU, local NVMe), us-east-1,
same-AZ cluster placement group for the 2-node network runs. Uniform harness:
64-byte payload, 10,000 warmup + 100,000 measured iterations, identical stats
code in each language.

**Runs recorded** (June 26 – July 27, 2026):

| run | what it measured |
|---|---|
| [20260626T103635Z](../journal/runs/20260626T103635Z-39abe130d644/entry.md) | First cross-host `network-rtt` run (tcp/udp/quic × rust/go/java) |
| [20260626T213457Z](../journal/runs/20260626T213457Z-deef392a8445/entry.md) | First `filesystem-write` run on local NVMe (fsync/fdatasync/prealloc/batch) |
| [20260627T071950Z](../journal/runs/20260627T071950Z-07a4b9a872fc/entry.md) | First `thread-handoff` run (spin/condvar/channel/ring); network + filesystem re-measured |
| [20260627T193417Z](../journal/runs/20260627T193417Z-003926ca6c91/entry.md) | Optimized SPSC ring (Rust + Go); full matrix re-measured |
| [20260713T152911Z](../journal/runs/20260713T152911Z-23b9778538e9/entry.md) | First `serialization` run (sbe_gen/aeron_sbe/bincode); full matrix re-measured — **baseline for network-rtt / filesystem-write / serialization** |
| [20260715T111653Z](../journal/runs/20260715T111653Z-9f707777cae2/entry.md) | First `smr-collections` run (LOB insert/update/snapshot, stop-the-world store — scoped run) |
| [20260716T100733Z](../journal/runs/20260716T100733Z-16a158ef9fd2/entry.md) | Go `serialization` cells added (`bebop`, `protobuf`) alongside the Rust codecs (scoped run) |
| [20260720T120209Z](../journal/runs/20260720T120209Z-79706160a45d/entry.md) | First `rpc-roundtrip` run (sbe_udp/grpc/bebop_tcp) — mutating cross-host round-trip (scoped run) |
| [20260722T131646Z](../journal/runs/20260722T131646Z-cd050b70cc78/entry.md) | `serialization` grid extended with Go SBE flyweight (`aeron_sbe`), Go SBE struct (`sbe_struct`), and `flatbuffers` (scoped run) |
| [20260723T081721Z](../journal/runs/20260723T081721Z-95af18f1353d/entry.md) | `serialization` re-measured on the **field-heavy typed-command record** (int/float/bool/string replaces the opaque blob) — all 8 cells, one run (scoped) |
| [20260727T004025Z](../journal/runs/20260727T004025Z-9aed7e218abe/entry.md) | First `smr-collections` MVCC-grid run: STW vs chunked-CoW vs ultima_db, incl. the `live_*` snapshot-under-writes cells — all 12 cells, one run (scoped) |
| [20260727T134311Z](../journal/runs/20260727T134311Z-bebcffe49a4d/entry.md) | `smr-collections` re-measure after the ultima_db `VersionPin` patch (pin-at-capture replaces the 16k-retention workaround) — all 12 cells, one run (scoped) |
| [20260727T164805Z](../journal/runs/20260727T164805Z-ddb09a5d0ff1/entry.md) | ultima cells only: `ultima_batch_insert`/`ultima_batch_update` debut (one txn per 64-command batch) + `bulk_load`-based restore — 6 cells, one run (scoped) |
| [20260729T202653Z](../journal/runs/20260729T202653Z-8956f783de54/entry.md) / [20260729T202913Z](../journal/runs/20260729T202913Z-6d82089e1182/entry.md) | ultima_db `open_table` handle-caching A/B (engine rev 8ac858d → 2907f56, #19) — same-host before/after, 5 ultima cells (scoped) |
| [20260729T214946Z](../journal/runs/20260729T214946Z-7f0f0cf5ee6b/entry.md) / [20260729T215021Z](../journal/runs/20260729T215021Z-7f0f0cf5ee6b/entry.md) | multi-table writer A/B (`SMRC_MULTI_TABLE` 0 → 1, `open_tables3`/`open_tables2`, #20) on the #20 engine — same-host, 2 batch cells (scoped) |
| [20260802T132729Z](../journal/runs/20260802T132729Z-7ab2574456c8/entry.md) | First **cancel-heavy churn** run: cancel op + ~1 % order-to-trade workload across all three languages (15 churn cells) — full 89-cell matrix, one run |
| [20260804T222844Z](../journal/runs/20260804T222844Z-c80faa7ab8ea/entry.md) | `smr-collections` full re-measure — first journaled run carrying the Rust CoW `Arc::get_mut` removal and the Go flyweight-codec fix; all 21 cells, one run (scoped). The smr section's tables quote this run |
| [20260805T182442Z](../journal/runs/20260805T182442Z-f6c13200cda8/entry.md) | `thread-handoff` re-measure + first **backoff**/**backoff_yield** cells: the Aeron ladder's timed-park cost per language (Go `time.Sleep` collapse vs the aeron-go yielding fix vs nanosleep/parkNanos) — 16 cells, one run (scoped) |

The regression baseline is **per focus area** (each cell in
`journal/baselines.json` names its reference run): `network-rtt`,
`filesystem-write`, and `serialization` reference 20260713T152911Z;
`smr-collections` references 20260804T222844Z; `thread-handoff` references
20260805T182442Z; `rpc-roundtrip` references 20260720T120209Z. Scoped runs (one
focus area, not a full-matrix re-measure) move only their own cells' baseline;
each section below cites the run its tables quote.

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
for `spin` (busy-wait), `condvar` (mutex + condition variable park/unpark),
`channel` (each language's standard channel), and the paced `backoff` cells
(Aeron-ladder idle strategy — see below), plus sustained **throughput** for
`ring` (pipelined SPSC ring buffer). Latency/throughput cells re-measured in
run 20260805T182442Z (one fleet), which also debuts `backoff`/`backoff_yield`:

| experiment | rust | go | java |
|---|---|---|---|
| spin p50 | 232 ns | 98 ns | 307 ns |
| condvar p50 | 383 ns | 396 ns | 281 ns |
| channel p50 | 407 ns | 305 ns | 20.2 µs (parked) |
| **backoff p50** (gap 100 µs) | **25.1 µs** | **969 µs** | **26.2 µs** |
| **backoff_yield p50** (go only) | — | **33.7 µs** | — |
| ring throughput | **379.5 M ops/s** | 54.7 M ops/s | 6.9 M ops/s |

> **Regime note (condvar/channel).** Whether OS threads actually park is
> scheduler/load-sensitive, and the cells are bimodal across runs: this run's
> Rust/Go condvar/channel stayed hot (~300–410 ns, the *no-park* floor) while
> Java's `SynchronousQueue` parked at the median (p50 20.2 µs — the prior run
> had it hot at 451 ns with the parking only in the tail). The earlier
> 20260627 baseline caught Rust/Java parking at ~22–24 µs (futex + scheduler
> round trip) vs Go's ~300–380 ns userspace goroutine park. Read the no-park
> floor and the ~20–24 µs parking cost as the two modes; which one a run
> catches is the scheduler's choice, and that parking penalty is the number
> this focus area exists to expose.

**What we learned:**

- **Busy-wait spin is a ~100–300 ns floor everywhere** — with no scheduler
  involved, the three runtimes converge (Go ~100–200 ns, Rust/Java
  ~230–310 ns).
- **The Aeron backoff ladder's cost is set by the platform's timed-park
  granularity, and Go's naive port collapses.** The `backoff` cells run a
  paced ping-pong (requester busy-waits 100 µs between round trips so the
  responder's spin → yield → park ladder ramps; park doubles 1 µs → 1 ms,
  aeron-go defaults). Rust (`thread::sleep`/nanosleep) and Java (Agrona's
  real `BackoffIdleStrategy` on `parkNanos`) wake in **25–26 µs at p50** —
  honest short rungs plus tens-of-µs overshoot. Go's `time.Sleep` overshoots
  sub-millisecond requests so badly (a 6 µs request costs ~425 µs; ≥ 8 µs
  costs ~1 ms — aeron-go `1ce3720`'s measurement) that the ladder is at its
  1 ms top rung by the first park: **p50 969 µs, p99 986 µs** — the wakeup
  *is* the sleep remainder, ~37× Rust/Java and ~10⁴× the spin floor.
- **The aeron-go yielding fix recovers Go almost entirely**: `backoff_yield`
  (parks under the 1 ms floor served by yielding to a deadline, aeron-go
  `1ce3720`) lands at **33.7 µs p50 — 29× better than the naive ladder** and
  in the same band as Rust/Java's naive ports, without dedicating a core the
  way `spin` does. This is the grid-methodology confirmation of the
  cluster-level result that motivated the fix (backoff 793 µs → yielding
  224 µs → spin 184 µs median ack in a 3-node Aeron cluster).
- **Seven cross-run flags on untouched cells, none confirmed**: the
  re-measured legacy cells moved in both directions vs the 20260713 baseline
  (Go spin −51 %, Go ring +27 % *improved*; Rust condvar +36 %, Java ring
  −11 % *flagged*) — instance-draw and scheduler variance on code-identical
  binaries; [`journal/REGRESSIONS.md`](../journal/REGRESSIONS.md) stays
  empty.
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

Encode and decode of one ~300 B state-machine-replication journal record — a
fixed header plus a repeating group of entries, each carrying a **typed command**
(`cmdQty` int64, `cmdPrice` float64, `cmdFlag` bool, `cmdText` string; the string
a short ~12-char field). This replaced an earlier record whose per-entry command
was an opaque ~78-byte byte blob — for a codec comparison a blob is close to dead
weight (a length-prefix + memcpy for everyone) and it drowned out the
field-encoding machinery. The typed command makes each codec's float/varint/bool/
string handling the dominant cost. Eight codecs, all in one run: three Rust —
`sbe_gen` (zero-copy SBE via `zerocopy`), `aeron_sbe` (the reference real-logic
`sbe-tool` Rust output), `bincode` (serde + bincode v2) — and five Go — `aeron_sbe`
(the SBE tool's zero-copy Golang **flyweight**, the Go twin of Rust `aeron_sbe`),
`sbe_struct` (the same tool's default owned-struct Golang codec), `bebop`
(200sc/bebop safe API), `protobuf` (canonical google.golang.org/protobuf,
`sfixed`-typed), `flatbuffers` (zero-copy accessors). The harness encodes a stream
of records into an in-memory journal then replays (decodes) them, timing each op
and — via a counting global allocator (Rust) / `ReadMemStats` TotalAlloc delta
(Go) — reporting heap bytes allocated per decode. 100,000 measured iterations.

Field-heavy record, all eight codecs (run 20260723T081721Z, sorted by decode p50):

| codec | lang | encode p50 | decode p50 | decode p99 | encoded bytes | decode alloc |
|---|---|---|---|---|---|---|
| sbe_gen              | rust | 42 ns  | 120 ns  | 194 ns  | 306 | **0 B**   |
| aeron_sbe            | rust | 56 ns  | 120 ns  | 193 ns  | 306 | **0 B**   |
| aeron_sbe (flyweight)| go   | 125 ns | 238 ns  | 354 ns  | 306 | **0 B**   |
| bincode              | rust | 60 ns  | 360 ns  | 439 ns  | 290 | **336 B** |
| bebop                | go   | 112 ns | 404 ns  | 912 ns  | 298 | **352 B** |
| flatbuffers          | go   | 817 ns | 459 ns  | 766 ns  | 472 | **0 B**   |
| sbe_struct           | go   | 404 ns | 941 ns  | 4932 ns | 306 | **384 B** |
| protobuf             | go   | 658 ns | 1192 ns | 5173 ns | 326 | **696 B** |

p50/p99 in ns; a `_mean` is also emitted per op. Uniform record builder and
iteration count across all codecs and languages, all measured in one run, so
Go-vs-Rust and codec-vs-codec read directly. (Earlier runs 20260713/20260722
measured the blob-dominated record and are not comparable to these figures.)

**What we learned:**

- **Removing the blob widened the spread ~2.5× and sped decode up.** Decode p50
  now ranges 120 ns (Rust SBE) to 1192 ns (Go protobuf) — ~10×, vs ~4× on the
  blob record. SBE decode itself dropped from ~408 ns to **120 ns**: the old
  78-byte command's byte-by-byte checksum fold (identical busywork for every
  codec) is gone, so what remains is genuine field materialization. This is
  exactly the effect the record change was made to expose.
- **The four zero-copy cells still decode at 0 allocation** — Rust `sbe_gen`/
  `aeron_sbe`, Go SBE flyweight, and `flatbuffers` view fields in the buffer;
  the owned decoders allocate 336–696 B (protobuf highest, rebuilding an owned
  message + string). Over a journal replay of millions of records that allocation
  is the dominant cost — the axis the focus area exists to expose.
- **FlatBuffers is now revealed as the most expensive to ENCODE (817 ns).** The
  blob record hid this (its FB encode was 572 ns). With more typed fields plus a
  nested string, FB's bottom-up builder — a `CreateString` before each table and
  a vtable constructed per entry — dominates, and its wire is the largest (472 B)
  because per-table vtable overhead is a bigger fraction when fields are small.
- **Zero-copy ≠ fastest reads: SBE beats FlatBuffers on decode, both at 0 alloc.**
  The SBE flyweight decodes in 238 ns (Go) / 120 ns (Rust) vs FlatBuffers' 459 ns,
  because SBE reads fixed byte offsets while FlatBuffers chases vtable + offset
  indirection per field. Zero-copy removes the allocation, not the per-field read
  cost. (The `kcchu/buffer-benchmarks` "flatbuffers has the fastest decode" claim
  did not reproduce on this record/harness.)
- **protobuf is slowest at both ends** (encode 658 ns, decode 1192 ns) — varint
  decoding of many typed scalars is costly, and it allocates the most (696 B) —
  though its wire (326 B) stays compact. `bincode`, by contrast, looks far better
  here (decode 360 ns) than when the blob dominated (947 ns): its owned field
  decode is cheap once the big byte copy is gone, and it is the smallest wire
  (290 B, varints), still at 336 B/decode allocation.
- **Same SBE tool, two Go modes: flyweight ~3.9× faster to decode than struct,
  and zero-alloc.** Go `aeron_sbe` (flyweight, 238 ns, 0 B) and `sbe_struct`
  (941 ns, 384 B) are the identical real-logic sbe-tool output over the
  byte-identical 306 B wire — flyweight vs the default owned-struct codegen. The
  gap widened on the field-heavy record because owned materialization of the extra
  typed fields (through the `SbeGoMarshaller`) costs more than folding them in
  place. Codegen mode, not format, sets the cost.
- **Rust SBE remains the champion and ~2× the Go flyweight at identical 0-alloc /
  306 B wire.** Rust `sbe_gen`/`aeron_sbe` decode at 120 ns vs the Go flyweight's
  238 ns — a pure language/codegen gap (fixed-offset reads, no bounds-check
  overhead) over the same bytes. The two Rust SBE toolchains stay wire-identical
  (byte-for-byte, conformance test) and within noise of each other (decode tied at
  120 ns; `sbe_gen` encode marginally cheaper, 42 vs 56 ns).
- **The owned-decode cells carry GC-visible tails; the zero-copy cells don't.**
  `sbe_struct` and `protobuf` show decode p99 ~4.3–5.2× their p50 (up to ~5.2 µs),
  the honest cost of rebuilding an owned object graph per record on a garbage-
  collected runtime; the four zero-copy cells stay tight (p99 ~1.5–1.7× p50).

## smr-collections — LOB state store: stop-the-world vs copy-on-write vs MVCC engine (single host)

The in-memory state an SMR state machine replays commands into — a fixed-capacity
limit-order-book — measured across three store designs on the same deterministic
workload: the original **flat STW store** (snapshot = serialize a frozen book),
a **chunked copy-on-write store** (`mvcc_*`: snapshot = O(#chunks) root capture
at an op boundary, serialize proceeds while writes continue), and **ultima_db**
(`ultima_*`, Rust only: an MVCC persistent-B-tree engine driven in its SMR
pattern — one explicit-version write-txn per applied command, snapshot = read-txn
at a version). All cells encode the identical SBE image (2,751,260 bytes; churned pools
carry freed slots and read 2,751,305), golden-verified byte-identical across
stores and languages. Every number in this section is from the
**20260804T222844Z** run — all 21 cells, all three languages, one fleet — so
every within-section comparison is same-run, same-host. This run is also the
first journaled one carrying two code fixes whose absence skewed earlier
figures: the Rust CoW store's `Arc::get_mut` removal and the Go snapshot
codec's flyweight mode (both discussed under "What we learned").

**Steady-state op cost** (mean, ns — the price you pay per applied command):

| op | store | rust | go | java |
|---|---|---|---|---|
| insert | flat (stw) | 48 | 111 | 159 |
| insert | chunked CoW | 59 | 91 | 227 |
| insert | ultima_db | 8,754 | — | — |
| update | flat (stw) | 90 | 108 | 138 |
| update | chunked CoW | 96 | 113 | 126 |
| update | ultima_db | 6,800 | — | — |

**Snapshot serialize / restore** (mean, single-threaded — the 2.75 MB image):

| store | rust ser | go ser | java ser | rust restore | go restore | java restore |
|---|---|---|---|---|---|---|
| flat (stw) | 684 µs | 978 µs | 796 µs | 1.70 ms | 3.68 ms | 11.0 ms |
| chunked CoW | 656 µs | 1.04 ms | 705 µs | 4.74 ms | 3.86 ms | 5.36 ms |
| ultima_db | 1.33 ms | — | — | 2.66 ms | — | — |

**Snapshot under live writes** (`live_*`: 200 K timed updates, a snapshot every
20 K ops; `writer_max` is the stall the write path actually observed):

| cell | writer p99 | **writer max** | serialize mean | skipped |
|---|---|---|---|---|
| live_stw / rust | 162 ns | **653 µs** | 570 µs | 0/10 |
| live_stw / go | 165 ns | **1.05 ms** | 966 µs | 0/10 |
| live_stw / java | 429 ns | **3.94 ms** | 1.94 ms | 0/10 |
| live_mvcc / rust | 242 ns | **137 µs** | 720 µs | 0/10 |
| live_mvcc / go | 332 ns | **265 µs** | 1.43 ms | 0/10 |
| live_mvcc / java | 532 ns | **2.91 ms** | 3.21 ms | 1/10 |
| live_ultima / rust | 8.11 µs | **153 µs** | 1.77 ms | 0/10 |

(`live_ultima`'s writer p50 is 6.7 µs — the per-op txn cost — so its p99/max
columns are on a different base than the ns-scale flat stores.)

**What we learned:**

- **For the STW store, the stall is exactly the serialize** — `writer_max` ≈
  snapshot mean in Rust and Go (653 µs / 1.05 ms vs 570 µs / 966 µs); Java's
  3.94 ms max tracks its serialize p99 plus collector time. And it is
  invisible at p99: a 1-in-20,000 event never shows there, which is why
  `writer_max` is the headline metric for this grid.
- **Chunked CoW delivers what it promises in Rust and Go**: the writer's worst
  op drops 653 µs → 137 µs (4.8×) in Rust and 1.05 ms → 265 µs (4.0×) in Go,
  while the serialize runs concurrently. The residual max is the op-boundary
  capture plus first-touch chunk copies, not the encode. (Single-event maxima
  carry 15–60 % cross-instance noise on this grid; the multiples are the
  finding, not the third digit.)
- **In Java the stall source is the collector, not the algorithm — now
  GC-log-verified.** `live_mvcc` writer_max improves only ~1.35×
  (3.94 → 2.91 ms). CoW chunk copies create garbage, and a GC pause lands on
  the writer where the serialize used to; the same JVM keeps p99 at 532 ns —
  the design works, the runtime charges for it elsewhere. A labelled
  GC-attribution diagnostic (2026-08-05, fleet, 3 repeats per live cell:
  `SMRC_GC_DIAG` stall-event stamps lined up against `-Xlog:gc*` wall-clock
  pause logs; never journaled as grid figures) confirmed it exactly: in all
  three `live_mvcc` repeats the run's **single** G1 young pause (2.6–2.8 ms)
  overlaps the writer_max op (2.8–3.1 ms) — the max *is* the pause. The STW
  control double-confirms the mechanism from the other side: `live_stw`
  logged **zero GC events in the entire run** (the serialize writes into a
  reused buffer and allocates nothing — the chunk copies are what makes the
  garbage), and its maxima sit on the snapshot-trigger iterations, i.e. the
  serialize, as always claimed.
- **The steady-state CoW tax in Rust is now small — and most of what the grid
  used to charge it was a bug.** Rust insert pays +23 % (48 → 59 ns) and
  update +7 % (90 → 96 ns). Earlier runs showed +62 % on insert: that was the
  `Arc::get_mut(...).expect(...)` pair in `order_mut`/`level_mut` re-verifying,
  with an atomic refcount check on every write, an invariant the epoch check
  already establishes (`capture()` bumps `gen` after cloning, so a
  `born == gen` chunk is never shared). The accessors now trust the invariant
  (with debug asserts on strong and weak counts), matching what Go always did.
  Go's CoW insert reads *faster* than its flat store this run (91 vs 111) —
  Go and Java means swing run to run; treat per-language means as a band.
- **The Go snapshot codec fix delivered on the fleet what the dev-box A/B
  predicted**: Go flat serialize 5.01 ms → 978 µs (5.1×) and restore
  9.20 → 3.68 ms (2.5×) vs the last journaled pre-fix run. The cause was
  codegen mode, not language: `booksnap` was generated owned-struct while
  Rust used flyweights — the same "codegen mode, not format, sets the cost"
  effect the `serialization` focus area isolates, amplified by ~486 K field
  writes per image. Go's serialize-driven pathologies went with it:
  `live_mvcc / go` skipped 5/10 snapshot triggers pre-fix, 0/10 now.
- **Java's restore cost is the pooled-object design, not the codec**:
  `new Book(cfg)` allocates 262,144 `Order` and 2,048 `Level` objects before
  decoding a byte, where Go's `NewBook` makes one contiguous slice — hence
  11.0 ms flat-store restore against Go's 3.68 ms over identical bytes. Its
  structure-of-arrays CoW store restores in half the time (5.36 ms) and still
  beats the pooled book on update (138 → 126 ns) and serialize (796 → 705 µs).
- **ultima_db's true engine-MVCC trade is ~100×, not ~1,000×**: ~6.8–8.8 µs
  per applied command (a full begin-write/commit cycle per op, ~75–180× the
  flat store) buys free snapshots — capture is one `pin_version` call, the
  writer's worst op while a snapshot streams is 153 µs (**below the STW
  store's own 653 µs stall**), and old versions are first-class. The
  ~104–113 µs figure the first MVCC-grid run published was ~90 % harness: a
  16k-snapshot retention window standing in for a version pin, charging every
  commit an O(retained) GC scan; `Store::pin_version` ended that. For
  sub-µs-budget apply loops the flat+CoW design still wins by ~100×; where the
  loop tolerates single-digit µs, the engine's versioning now comes at no
  additional stall — and batching commands per txn amortizes to 2.4–2.9 µs/op
  (see the batched-apply subsection below).
- CoW restore is slower than STW restore in Rust (4.7 vs 1.7 ms — rebuilding
  through chunk mutators), about equal in Go (3.9 vs 3.7 ms), and faster in
  Java (5.4 vs 11.0 ms); restore is off the SMR hot path either way
  (recovery-time only).
- Single-host serialize means on this grid carry a documented ±35 %
  cross-instance band; within-run comparisons are the trustworthy ones, which
  is why this section is now a single run.

### Batched apply + bulk_load restore

Two additions bracket the engine trade at its realistic end. A real SMR
applier commits a **consensus batch per txn**, not a txn per command — the
`ultima_batch_*` cells apply 64 commands per explicit-version txn with
per-command work byte-identical to the unbatched cells (enforced by a
golden-equivalence test), so the difference is txn amortization (plus sub-1 %
timing/allocation asymmetries — quote ratios accordingly). And restore uses
ultima_db's intended path (`bulk_load_batch`: one atomic O(N) `from_sorted`
install) instead of ~capacity per-record inserts.

| cell | per-op mean | batch mean (B=64) | vs same-run unbatched |
|---|---|---|---|
| ultima_batch_insert | **2.37 µs** | 152 µs | 8.75 µs → **3.7×** |
| ultima_batch_update | **2.86 µs** | 183 µs | 6.80 µs → **2.4×** |

**What we learned:**

- **Batching closes the engine trade from ~100× to ~30–50× the flat store**,
  all same-run this time: 2.37 µs vs the flat store's 48 ns insert (49×),
  2.86 µs vs its 90 ns update (32×).
- **The batch txn's absolute cost barely moves with 64× the work** (152–183 µs
  per 64-command txn vs 6.8–8.8 µs per 1-command txn ≈ 17–27× for 64×
  commands): most of the unbatched per-op cost is txn machinery, exactly what
  the microbench-level `apply_sw_batch_throughput` predicted from the engine
  side.
- **Restore via `bulk_load_batch` is 2.66 ms** for the 2.75 MB image
  (byte-identity round-trip preserved; the per-record path it replaced
  measured 8.60 ms). ~1.6× the flat store's memcpy-style 1.70 ms restore —
  the price of building real B-trees — and under the CoW stores' 3.9–5.4 ms.

#### Engine-side follow-ups (ultima_db #19 and #20, both fleet-measured)

Two ultima_db optimizations the batched cells exposed have since landed on
ultima_db `main`. **The numbers above are the pre-optimization baseline** (engine
rev `ddb09a5`'s dep, which pins `8ac858d`); this note records what changed.

- **#19 — `open_table` handle caching (fleet-measured, −7 to −13 % on the batch
  cells).** `WriteTx::open_table` re-derived a metrics-registry handle (RwLock
  read + hash lookup) and a name allocation on *every* call; a batched applier
  opens each of its 3 tables 64 times per txn, so this was pure per-command
  overhead. Caching the handle per table per transaction was measured
  **same-host, before/after** (engine `8ac858d` → `2907f56`, runs
  20260729T2026/2029): `ultima_batch_update` −13.1 % mean / −17.2 % p99,
  `ultima_batch_insert` −7.3 % mean / −23.6 % p99. Single-command cells moved
  within noise (nothing to amortize). The tail moving more than the mean is the
  signature of dropping an allocation and a lock from the path. (Absolute
  numbers not shown: that A/B ran on a different instance than the 20260727 run
  above, and cross-instance variance on `batch_update` alone is ~21 %; the
  trustworthy figure is the same-host delta.)
- **#20 — multi-table writer (fleet-measured, −12 to −13 % on the batch cells).**
  `open_tables2`/`open_tables3` let a transaction hold several table writers at
  once, so the batched applier opens its 3 tables **once per batch** (via
  `open_tables3`, insert) / 2 tables (`open_tables2`, update) instead of once per
  command. The cells now take that path under `SMRC_MULTI_TABLE=1`; a **same-host
  A/B** on the post-#19 engine (`8831c4e`, runs 20260729T2149/2150, per-command
  vs multi-table, per-command work byte-identical by golden test) measured:
  `ultima_batch_insert` −12.4 % mean / −8.8 % p99, `ultima_batch_update` −12.6 %
  mean / **−21.7 % p99**.
  - **Correction to an earlier estimate.** A synthetic table-major *probe* had
    put this at ~35–40 %; the real number is ~12–13 %. The probe overstated for
    two reasons: it ran a small config (~550 ns/op) where 3 fixed table-opens are
    a large fraction, whereas the real cell is ~2.2 µs/op (LEVELS=1024, larger
    records) so the same 3 opens are ~12 %; and it reordered access table-major,
    folding in a cache-locality gain that `open_tables3` alone — which keeps
    command-major access — does not deliver. #19 also already made each open
    cheap, so removing them entirely on top of #19 saves less than removing the
    old expensive opens would have. ~12 % same-fleet is the honest figure.

Net trajectory of the engine-MVCC trade vs the flat store, batched: ~100× before
batching, ~30–50× with batching (above). #19 (~7–13 %) and #20 (~12 %) each shave
the batched per-op further — real, same-fleet, and smaller than the headline
amortization step, as second-order engine wins tend to be.

### Cancel-heavy churn

Everything above was measured on a workload where **nothing is ever removed**.
That is the friendliest possible case for an MVCC engine: a delete frees
nothing, it writes a new version that lives until no reader can reach it. Real
order flow is the opposite — roughly **1 % of orders end in a trade**, so ~99 %
leave the book by cancellation, and a matching engine's state store spends its
life recycling slots rather than filling them.

The `churn` cells add a third command — remove — and make it dominant:
alternating insert / departure, where a departure is a cancel 99 % of the time
and a full fill 1 % (`SMRC_OTR_BPS`, default 100 bps). The live set stays
exactly constant while turnover runs continuously. All figures below are from
one run on one fleet, so within-table comparisons are same-host.

**Steady-state op cost** (mean, ns):

| op | store | rust | go | java |
|---|---|---|---|---|
| insert | flat (stw) | 50 | 69 | 182 |
| **cancel** | flat (stw) | **105** | **99** | **175** |
| fill | flat (stw) | 108 | 98 | 258 |
| insert | chunked CoW | 61 | 82 | 112 |
| **cancel** | chunked CoW | **118** | **119** | **174** ‡ |
| insert | ultima_db | 9,662 | — | — |
| **cancel** | ultima_db | **10,541** | — | — |

‡ **Java's flat-vs-CoW delta is not a CoW measurement — do not read it as one.**
Java is the only language whose two stores differ structurally: `Book` uses
`Long2ObjectHashMap<Order>` over a pool of 262,144 `Order` *objects*, while
`CowBook` uses `Long2LongHashMap` over primitive fields inside `OrderChunk`
arrays. So Java's "flat" store does key → object reference → pointer chase,
and its "CoW" store does key → primitive slot → primitive array index. The
CoW store has strictly better memory layout in Java, independent of any
copy-on-write machinery, which is what the 240 → 164 ns move measures. Rust
and Go hold both variables constant (identical id-map and order representation
across their two stores), so only their deltas isolate CoW.

**The engine trade under cancellation** (same-run, per applied command):

| cell | per-op mean | vs same-run insert/update |
|---|---|---|
| `ultima_batch_insert` | 2,370 ns | — |
| `ultima_batch_update` | 2,858 ns | — |
| **`ultima_batch_churn`** | **4,670 ns** | **1.6–2.0×** |
| `ultima_insert` (unbatched) | 8,754 ns | — |
| **`ultima_churn`** (unbatched) | **10,541 ns** | **+20 %** |

**Snapshot under churn** (`writer_max` — the stall the write path observed):

| cell | rust | go | java |
|---|---|---|---|
| `live_stw` (no churn) | 653 µs | 1.05 ms | 3.94 ms |
| `live_stw_churn` | 703 µs | 1.10 ms | 3.99 ms |
| `live_mvcc` (no churn) | 137 µs | 265 µs | 2.91 ms |
| `live_mvcc_churn` | **226 µs** | **420 µs** | **259 µs** |
| `live_ultima_churn` | 188 µs | — | — |

**What we learned:**

- **Cancel costs about 2× an insert on the flat store** (105 vs 50 ns in Rust),
  and its `cancel_p99` of 243 ns is the O(levels) best-price rescan surfacing in
  the tail — exactly where it was designed to. The rescan is deliberately on the
  timed path: real books maintain the cached best, and hiding it would hide the
  worst-case cancel.
- **Cancellation leaves the engine-MVCC trade in the same band, not an order
  of magnitude worse.** Batched ultima_db costs 4,670 ns/op under churn against
  2,370/2,858 for insert/update on the same host (1.6–2.0×) — ~44× the flat
  store's cancel, where the insert/update-only workload puts the trade at
  ~32–49×. The pessimistic reading (that deletes would collapse the engine's
  position) did not materialise.
- **Chunked CoW's cancel penalty in Rust was `Arc::get_mut`, not
  copy-on-write — found, fixed, and now fleet-confirmed.** The pre-fix run
  showed a 2.06× flat→CoW cancel gap (+133 ns); this run shows **1.13×
  (+14 ns)**, with insert at +11 and fill at +17 — the same low-tens-of-ns
  band Go's gap occupies (+20 ns this run, −3 ns the run before). The
  diagnosis that got there: `mvcc_churn` never calls `capture()`, so no chunk
  is ever copied — the gap instead scaled with mutable accesses per op,
  because `order_mut`/`level_mut` paid `Arc::get_mut`'s atomic uniqueness
  check on every write to re-verify what the epoch check already guarantees.
  Cancel and fill were hit hardest because the O(levels) rescan makes hundreds
  of `level_mut` calls per op (~250 ns/op, far beyond the ~7–12 ns-per-access
  first estimate). The accessors now trust the epoch invariant directly
  (debug-asserted on strong and weak counts), matching what Go always did.
  Java's number does not bear on this at all (see ‡ above).
- **CoW's snapshot-stall advantage widens sharply under churn, and Java does
  see it — but the GC diagnostic shows *why*, and it tempers the claim.**
  Java's STW→CoW `writer_max` improvement is ~1.35× without churn but **15×
  with it** (3.99 ms → 259 µs; the prior run showed 25×). The 2026-08-05
  GC-attribution runs explain the mechanism: each `live_mvcc_churn` repeat
  still incurs exactly one ~2.3 ms G1 young pause, but it fires **before the
  timed loop** — churn's heavier setup (pool + free-list fill) empties the
  young gen right at loop entry, and the ~60–100 ms timed window then
  completes without a collection, so writer_max stays µs-scale. The
  structural claim stands: CoW removes the *serialize* from the writer
  unconditionally, and the residual exposure is GC cadence, not the
  algorithm. But the µs-scale churn maxima are a
  measurement-window-vs-pause-cadence outcome, not a guarantee — a
  production-length run would land periodic young pauses on the CoW writer
  (~2–3 ms each at this allocation rate) unless the collector is tuned for
  it; a ZGC/Shenandoah arm is the natural follow-up if that stall budget
  matters.
- **`journal compare` vs the previous smr run flags nothing confirmable:**
  untouched flat-store cells moved up to ±30 % in both directions across the
  two instance draws — the documented cross-instance band — which is exactly
  why every table in this section now quotes a single run.
  [`journal/REGRESSIONS.md`](../journal/REGRESSIONS.md) stays empty.

**Caveats these numbers must carry:**

- **Java's `fill_mean` is not yet comparable.** It sits at 1.47× its own
  `cancel_mean` (258 vs 175 ns) where Rust and Go are at ~1.0×. `fill` is 0.5 %
  of ops by design, so it barely reaches HotSpot's compile thresholds; the JVM
  pre-run keeps `fill_p50` in line (166 vs 151) but the mean still carries a
  cold tail.
- **Per-op splits inside the `live_*` cells are polluted by the trigger op** —
  Go's `live_stw_churn` reports `insert_mean` 176 ns against `insert_p50`
  67 ns, because the ops that trigger a snapshot absorb the whole serialize.
  `writer_max` remains the headline for those cells; read the split for *which*
  op absorbed the stall, not as a latency.
- **Java's RSS figures are a JVM artifact** — heap, metaspace and code cache,
  not store growth. Java-vs-Java trend only.
- **Churn per-op means read ~5–8 % fast** against the older `insert`/`update`
  cells, which time their own op generation where the churn cells do not.
  Adjacent rows in the tables above; systematic, not noise.
- **Still unmeasured:** version GC under a *growing* live set. This workload
  holds the live set exactly constant by construction, so reclamation is
  measured at steady state only.

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
   if you can spin, all three reach ~100–300 ns, and a well-tuned Rust SPSC
   ring moves 380 M+ ops/s. For the middle ground real duty cycles run — the
   Aeron spin→yield→park backoff ladder — the cost is the platform's
   timed-park granularity: Rust/Java wake in ~25 µs, but Go's `time.Sleep`
   overshoot collapses the ladder to ~1 ms wakeups; the aeron-go
   yield-below-floor fix recovers Go to ~34 µs (29×) without pinning a core.
4. **Language choice matters least where the kernel or device dominates**
   (network RTT, disk sync) and most where the runtime owns scheduling
   (thread parking) or the compiler owns the inner loop (SPSC ring).
5. **Log record codec** (measured on a field-heavy ~300 B record — typed int/
   float/bool/string fields, not an opaque blob, so field encoding is what's
   compared): a zero-copy SBE codec decodes with **no per-record allocation** and
   **~3× faster** (~120 ns Rust / 238 ns Go vs 360–1192 ns) than the owned
   decoders (bincode/bebop/sbe-struct/protobuf), which rebuild an owned graph
   (336–696 B) every decode — the memory and latency difference an SMR replay path
   cares about. The two Rust SBE toolchains (pure-Rust `sbe_gen` vs the reference
   Aeron `sbe-tool`) are wire-identical, so that choice is ergonomic. But
   **zero-copy is not automatically the fastest decode** — SBE's fixed-offset
   reads beat FlatBuffers' per-field vtable indirection (238 vs 459 ns in Go, both
   0-alloc), and FlatBuffers is also the priciest to *encode* (817 ns, bottom-up
   builder) with the largest wire — so for a hot replay path prefer a fixed-layout
   zero-copy codec (SBE) over an offset-table one (FlatBuffers).
7. **FSM state store, measured on the workload that actually occurs** (~99 %
   cancellations, not insert/update-only): a flat pooled store with an intrusive
   free list applies a cancel in **105 ns** (Rust) — about 2× an insert, with the
   O(levels) best-price rescan showing up in `cancel_p99` at 243 ns. Chunked
   copy-on-write now costs only low-tens of ns more per op in every language
   (Rust's former 2× cancel penalty was an `Arc::get_mut` bug, since fixed).
   An MVCC B-tree engine driven in its SMR pattern costs **~44× the flat
   cancel batched** (4,670 ns/op at B=64) and ~100× unbatched — cancellation
   leaves the batched trade inside the same ~30–50× band as insert/update,
   far short of a collapse. For a sub-µs apply budget the flat store still
   wins decisively; where single-digit µs is affordable, the engine buys
   stall-free snapshots by construction. **The snapshot side is where the choice
   is starkest:** under churn, a stop-the-world serialize stalls the writer for
   0.7–4.0 ms depending on language, while chunked copy-on-write holds it to
   230–420 µs — and that gap is much wider under cancellation than without it
   (in Java, 15–25× across the two runs measured; GC-log-verified caveat:
   Java's residual CoW stall exposure is young-GC cadence — ~2–3 ms pauses
   from chunk-copy garbage — so over production-length windows the Java CoW
   writer eats periodic GC stalls unless the collector is tuned for it).
   Choose stop-the-world only if you can snapshot on a non-voting replica.

6. **RPC framework vs hand-rolled stack:** for a mutating request/response on
   the replication path, a hand-rolled UDP + zero-copy SBE stack round-trips in
   ~26 µs; full gRPC (HTTP/2 + protobuf) costs **~4.8×** that (~126 µs) for its
   framing and call machinery, and a plain TCP + bebop stack lands in between
   (~35 µs). The transport+codec stack, not the language, sets the tier here.
