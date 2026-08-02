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
| [20260713T152911Z](../journal/runs/20260713T152911Z-23b9778538e9/entry.md) | First `serialization` run (sbe_gen/aeron_sbe/bincode); full matrix re-measured — **current baseline** |
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

Unless noted, tables below show the **current baseline** run (20260713T152911Z). The
July 15 – 27 runs are **scoped** (one focus area each, not a full-matrix
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
at a version). All cells encode the identical 2,751,256-byte SBE image,
golden-verified byte-identical across stores and languages. Numbers below are
from the scoped 20260727T134311Z run (all 12 cells re-measured in one run, so
within-table comparisons are same-fleet).

**Correction vs the first MVCC-grid run (20260727T004025Z):** that run's
ultima_db cells (~104–113 µs/op, "~1,000× the flat store") were dominated by a
harness workaround, not engine cost. The adapter retained **16,384 snapshots**
purely so a captured version number survived the writer→serializer handoff, and
on the then-pinned engine rev every commit paid an O(retained) auto-GC scan for
it. ultima_db `8ac858d` added `Store::pin_version` → `VersionPin` (a `Send`
handle that keeps one version alive); the harness now pins at capture and
retention stays at the store default. Same workload, same engine architecture —
the per-op cost below is what the transaction machinery actually costs.

**Correction — Rust chunked-CoW rows predate an `Arc::get_mut` removal:**
`order_mut`/`level_mut` no longer call `Arc::get_mut` on every write (see
`rust/smr-collections/common/src/cowbook.rs`); the redundant atomic
uniqueness check was replaced by trusting the epoch invariant the code
already establishes. Every Rust chunked-CoW figure below — the `insert`/
`update` CoW rows in the next table and the `live_mvcc` writer figures
further down — was measured before that change and is therefore slow by an
estimated 7–12 ns per mutable access. That correction is directional (it
comes from a different host than this table), but comparable in magnitude to
the published flat-vs-CoW gap itself: the insert row's flat-48/CoW-77 ns gap
is 29 ns, against an estimated ~30 ns correction on insert. See "Chunked
CoW's cancel penalty in Rust is `Arc::get_mut`, not copy-on-write" further
below for the full accounting and the six affected cells.

**Steady-state op cost** (mean, ns — the price you pay per applied command):

| op | store | rust | go | java |
|---|---|---|---|---|
| insert | flat (stw) | 48 | 100 | 142 |
| insert | chunked CoW | 77 | 97 | 293 |
| insert | ultima_db | 8,404 | — | — |
| update | flat (stw) | 89 | 104 | 132 |
| update | chunked CoW | 102 | 111 | 124 |
| update | ultima_db | 6,406 | — | — |

**Snapshot serialize / restore** (mean, single-threaded — the 2.75 MB image):

| store | rust ser | go ser | java ser | rust restore | go restore | java restore |
|---|---|---|---|---|---|---|
| flat (stw) | 611 µs | 5.01 ms | 790 µs | 1.34 ms | 9.20 ms | 10.7 ms |
| chunked CoW | 682 µs | 5.08 ms | 706 µs | 4.81 ms | 9.13 ms | 5.42 ms |
| ultima_db | 1.45 ms | — | — | 8.60 ms | — | — |

**Snapshot under live writes** (`live_*`: 200 K timed updates, a snapshot every
20 K ops; `writer_max` is the stall the write path actually observed):

| cell | writer p99 | **writer max** | serialize mean | skipped |
|---|---|---|---|---|
| live_stw / rust | 164 ns | **746 µs** | 639 µs | 0/10 |
| live_stw / go | 166 ns | **5.15 ms** | 4.90 ms | 0/10 |
| live_stw / java | 482 ns | **3.75 ms** | 1.97 ms | 0/10 |
| live_mvcc / rust | 239 ns | **258 µs** | 673 µs | 0/10 |
| live_mvcc / go | 297 ns | **233 µs** | 5.19 ms | 5/10 |
| live_mvcc / java | 580 ns | **3.04 ms** | 3.83 ms | 0/10 |
| live_ultima / rust | 8.26 µs | **196 µs** | 1.87 ms | 0/10 |

(`live_ultima`'s writer p50 is 6.6 µs — the per-op txn cost — so its p99/max
columns are on a different base than the ns-scale flat stores.)

**What we learned:**

- **For the STW store, the stall is exactly the serialize** — `writer_max` ≈
  snapshot mean in all three languages (746 µs / 5.2 ms / 3.8 ms). And it is
  invisible at p99: a 1-in-20,000 event never shows there, which is why
  `writer_max` is the headline metric for this grid.
- **Chunked CoW delivers what it promises in Rust and Go**: the writer's worst
  op drops 746 µs → 258 µs (2.9×) in Rust and 5.15 ms → 233 µs (22×) in Go,
  while the serialize runs concurrently. The residual max is the op-boundary
  capture plus first-touch chunk copies, not the encode. (The Rust/Go maxes
  moved 15–60 % between the two same-day runs on code-identical cells —
  single-event maxima carry exactly that much cross-instance noise; the
  orders-of-magnitude gap vs STW is the finding, not the third digit.)
- **In Java the new stall source is the collector, not the algorithm**:
  `live_mvcc` writer_max improves only ~1.2× (3.75 → 3.04 ms). CoW chunk copies
  create garbage, and a GC pause lands on the writer where the serialize used
  to. The same JVM keeps p99 at 580 ns — the design works; the runtime charges
  for it elsewhere. (Unverified attribution — GC logs would confirm.)
- **The steady-state CoW tax is small where it was feared**: Rust insert pays
  +62 % (48 → 77 ns, epoch check + chunk-table indirection on a 48 ns op), Go
  pays ~0–7 %, and Java's structure-of-arrays chunks still beat the
  pooled-object book on update (132 → 124 ns) and serialize (790 → 706 µs) —
  though Java's insert means swung between the two runs (JIT/run variance;
  its p50s moved far less), so treat Java means as band, not point.
- **ultima_db's true engine-MVCC trade is ~100×, not ~1,000×**: ~6.4–8.4 µs
  per applied command (a full begin-write/commit cycle per op, ~75–175× the
  flat store) buys free snapshots — capture is one `pin_version` call, the
  writer's worst op while a snapshot streams is 196 µs (**below the STW
  store's own 746 µs stall**), and old versions are first-class. The earlier
  ~104–113 µs figure was ~90 % harness: a 16k-snapshot retention window
  standing in for a version pin, charging every commit an O(retained) GC scan.
  For sub-µs-budget apply loops the flat+CoW design still wins by ~100×; where
  the loop tolerates single-digit µs, the engine's versioning now comes at no
  additional stall — and batching commands per txn amortizes to 2.3–2.7 µs/op
  (see the batched-apply subsection below).
- **The restore flag from this run (+11 %) is closed**: the per-record-insert
  restore path it sat on was replaced wholesale by `bulk_load_batch` in the
  follow-up run below (−72 %), mooting the question of whether the wiggle was
  real. The serialize cells, untouched across all three same-day runs, have
  now swung 1.67 → 1.45 → 2.26 ms (p50) — treat single-host serialize means
  on this grid as carrying a ±35 % cross-instance band.
- **Go skipped 5 of 10 snapshot triggers in `live_mvcc`** (2/10 in the prior
  run) — its ~5.2 ms serialize exceeds the ~4 ms trigger window at this
  cadence, so skip counts sit on a knife edge. `snap_skipped` surfacing that
  honestly is the point; snapshot cadence is a per-language tunable, not a
  constant.
- CoW restore is slower than STW restore in Rust (4.8 vs 1.3 ms — rebuilding
  through chunk mutators) but faster in Java (5.4 vs 10.7 ms); restore is off
  the SMR hot path either way (recovery-time only).
- Deliberately unmeasured: version GC under churn — the workload has no
  cancel/remove op (spec caveat), so the cost of reclaiming dead versions under
  delete-heavy load is future work. (The engine side of this got cheaper
  regardless: ultima_db `8ac858d` makes snapshot GC O(evicted) per commit
  instead of O(retained).)

### Batched apply + bulk_load restore (run 20260727T164805Z)

Two additions bracket the engine trade at its realistic end. A real SMR
applier commits a **consensus batch per txn**, not a txn per command — the new
`ultima_batch_*` cells apply 64 commands per explicit-version txn with
per-command work byte-identical to the unbatched cells (enforced by a
golden-equivalence test), so the difference is txn amortization (plus sub-1 %
timing/allocation asymmetries — quote ratios accordingly). And restore now
uses ultima_db's intended path (`bulk_load_batch`: one atomic O(N)
`from_sorted` install) instead of ~capacity per-record inserts.

| cell | per-op mean | batch mean (B=64) | vs same-run unbatched |
|---|---|---|---|
| ultima_batch_insert | **2.31 µs** | 148 µs | 8.47 µs → **3.7×** |
| ultima_batch_update | **2.67 µs** | 171 µs | 7.11 µs → **2.7×** |

| cell | this run | prior run (per-record path) |
|---|---|---|
| ultima_snapshot restore | **2.43 ms** | 8.60 ms (−72 %) |

**What we learned:**

- **Batching closes the engine trade from ~100× to ~30–50× the flat store.**
  Per-op cost lands at 2.3–2.7 µs (vs the flat store's 48–89 ns in the
  previous run — cross-run, so treat the flat side as a band). The unbatched
  cells re-measured within 1–11 % of the prior run in the same fleet run, so
  the 3.7×/2.7× amortization ratios are same-fleet and solid.
- **The batch txn's absolute cost barely moves with 64× the work** (148–171 µs
  per 64-command txn vs 7–8 µs per 1-command txn ≈ 19–24× for 64× commands):
  most of the unbatched per-op cost is txn machinery, exactly what the
  microbench-level `apply_sw_batch_throughput` predicted from the engine side.
- **Restore via `bulk_load_batch` is 3.5× faster than the per-record path**
  (2.43 ms for the 2.75 MB image; byte-identity round-trip preserved). Still
  ~1.8× the flat store's memcpy-style 1.34 ms restore — the price of building
  real B-trees — and comfortably under the CoW stores' 4.8–9.2 ms.
- The serialize means in this run flagged +16–56 % vs the prior run on
  untouched code; see the variance note above — the three same-day runs put a
  ±35 % band on single-host serialize means, and `journal/REGRESSIONS.md`
  stays empty.

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

### Cancel-heavy churn (run 20260802T132729Z)

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
| insert | flat (stw) | 56 | 92 | 247 |
| **cancel** | flat (stw) | **125** | **136** | **240** |
| fill | flat (stw) | 127 | 136 | 345 |
| insert | chunked CoW | 70 | 97 | 109 |
| **cancel** | chunked CoW | **258** | **133** | **164** ‡ |
| insert | ultima_db | 9,608 | — | — |
| **cancel** | ultima_db | **10,312** | — | — |

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
| `ultima_batch_insert` | 2,356 ns | — |
| `ultima_batch_update` | 3,111 ns | — |
| **`ultima_batch_churn`** | **4,456 ns** | **1.4–1.9×** |
| `ultima_insert` (unbatched) | 8,768 ns | — |
| **`ultima_churn`** (unbatched) | **10,312 ns** | **+18 %** |

**Snapshot under churn** (`writer_max` — the stall the write path observed):

| cell | rust | go | java |
|---|---|---|---|
| `live_stw` (no churn) | 574 µs | 5.22 ms | 3.90 ms |
| `live_stw_churn` | 684 µs | 5.37 ms | 6.04 ms |
| `live_mvcc` (no churn) | 124 µs | 244 µs | 2.88 ms |
| `live_mvcc_churn` | **204 µs** | **305 µs** | **239 µs** |
| `live_ultima_churn` | 199 µs | — | — |

**What we learned:**

- **Cancel costs about 2× an insert on the flat store** (125 vs 56 ns in Rust),
  and its `cancel_p99` of 410 ns is the O(levels) best-price rescan surfacing in
  the tail — exactly where it was designed to. The rescan is deliberately on the
  timed path: real books maintain the cached best, and hiding it would hide the
  worst-case cancel.
- **Cancellation makes the engine-MVCC trade worse by about a third, not by an
  order of magnitude.** Batched ultima_db costs 4,456 ns/op under churn against
  2,356/3,111 for insert/update on the same host — so ~36× the flat store's
  cancel, where the insert/update-only workload had put the trade at ~30–50×.
  The pessimistic reading (that deletes would collapse the engine's position)
  did not materialise; the optimistic one (that the earlier numbers were
  representative) was also wrong.
- **Chunked CoW's cancel penalty in Rust is `Arc::get_mut`, not copy-on-write.**
  `mvcc_churn` never calls `capture()`, so no chunk is ever copied. A local
  experiment (levels=8, ~1,000 orders per level, so the ladder rescan never
  does work) reproduces the gap at 1.88× — refuting the rescan hypothesis — and
  the gap scales with the number of **mutable accesses per op**: update (2
  accesses) +14 ns, insert (3) +30 ns, cancel (4) +47 ns, i.e. ~7–12 ns each.
  Rust's `order_mut`/`level_mut` call `Arc::get_mut(...).expect(...)` — an
  atomic uniqueness check plus a branch — on every write, to verify an
  invariant the epoch check immediately above already guarantees (a chunk with
  `born == gen` was created after the last capture, so no `Root` holds it,
  which is why that `expect` never fires). Go's equivalent is a plain pointer
  load that trusts the same invariant, which is why Go shows parity
  (133 vs 136 ns). Rust's `mvcc_*` cells have therefore been carrying an
  avoidable per-write cost in every run to date. Java's number does not bear on
  this at all (see ‡ above).
  **Since fixed** — `order_mut`/`level_mut` now trust the epoch invariant
  directly, matching Go. This changes six already-journaled Rust cells:
  `mvcc_insert`, `mvcc_update`, `mvcc_snapshot`, `live_mvcc`, `mvcc_churn`,
  `live_mvcc_churn`. Every figure for those cells on this page predates that
  change and is therefore slow by roughly 7–12 ns per mutable access
  (~14 ns/update, ~30 ns/insert, ~47 ns/cancel). The effect should be
  negligible on `mvcc_snapshot` and the `live_*` cells, since those are
  dominated by the serialize, not the per-write check. Quantifying it
  properly needs a same-host A/B — this grid's ±21–35 % cross-instance band
  would swamp the effect in any cross-run comparison — so the figures here
  stand until that run happens.

- **CoW's snapshot-stall advantage widens sharply under churn, and Java may
  finally see it.** Java's STW→CoW improvement is 1.35× without churn and 25×
  with it (6.04 ms → 239 µs) — which would invert the earlier finding that
  Java's CoW gains are eaten by GC. **Read this as suggestive, not
  established:** `writer_max` is a single-event maximum over 10 triggers, so
  whether a GC pause lands is close to a coin flip. It needs a repeat run.
- **`snapshot_bytes` shifted +4 bytes on every pre-existing cell**
  (2,751,256 → 2,751,260) — the schema-v2 `freeHead` field, a one-time
  change, not a regression. Churn cells read 2,751,305: a churned pool carries
  freed slots.
- **Eight cells flagged by `journal compare`, none confirmed.**
  `ultima_batch_insert` +20.5 % and `ultima_batch_update` +14.3 % against the
  scoped 20260729 A/B run — but that was a different instance draw, this grid
  carries a documented ~21 % cross-instance band on `batch_update` alone, and
  both cells changed composition this cycle (the adapter gained an
  `order_id → row id` map and `OrderRec` grew 4 bytes).
  [`journal/REGRESSIONS.md`](../journal/REGRESSIONS.md) stays empty.

**Caveats these numbers must carry:**

- **Java's `fill_mean` is not yet comparable.** It sits at 1.44× its own
  `cancel_mean` (345 vs 240 ns) where Rust and Go are at ~1.0×. `fill` is 0.5 %
  of ops by design, so it barely reaches HotSpot's compile thresholds; the JVM
  pre-run added for this cycle brought `fill_p50` into line (231 vs 215) but the
  mean still carries a cold tail.
- **Per-op splits inside the `live_*` cells are polluted by the trigger op** —
  Go's `live_stw_churn` reports `insert_mean` 642 ns against `insert_p50`
  133 ns, because the ops that trigger a snapshot absorb the whole serialize.
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
   if you can spin, all three reach ~200–300 ns, and a well-tuned Rust SPSC
   ring moves 400 M+ ops/s.
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
   free list applies a cancel in **125 ns** (Rust) — about 2× an insert, with the
   O(levels) best-price rescan showing up in `cancel_p99` at 410 ns. An MVCC
   B-tree engine driven in its SMR pattern costs **~36× that batched**
   (4,456 ns/op at B=64) and ~82× unbatched; cancellation worsens the engine's
   position by about a third versus the insert/update-only figure, which is real
   but far short of a collapse. For a sub-µs apply budget the flat store still
   wins decisively; where single-digit µs is affordable, the engine buys
   stall-free snapshots by construction. **The snapshot side is where the choice
   is starkest:** under churn, a stop-the-world serialize stalls the writer for
   0.7–6.0 ms depending on language, while chunked copy-on-write holds it to
   204–305 µs — and that gap is much wider under cancellation than without it.
   Choose stop-the-world only if you can snapshot on a non-voting replica.

6. **RPC framework vs hand-rolled stack:** for a mutating request/response on
   the replication path, a hand-rolled UDP + zero-copy SBE stack round-trips in
   ~26 µs; full gRPC (HTTP/2 + protobuf) costs **~4.8×** that (~126 µs) for its
   framing and call machinery, and a plain TCP + bebop stack lands in between
   (~35 µs). The transport+codec stack, not the language, sets the tier here.
