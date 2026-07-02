# Results so far

A human-readable summary of the experiments run to date and what they showed.
All numbers come from **real AWS benchmark runs** recorded in [`journal/`](../journal/INDEX.md)
(never loopback/dev-box smoke runs). Raw per-metric data lives in each run's
`results.jsonl`; the current reference values are in `journal/baselines.json`.

**Test rig (all runs):** `c6id.2xlarge` (8 vCPU, local NVMe), us-east-1,
same-AZ cluster placement group for the 2-node network runs. Uniform harness:
64-byte payload, 10,000 warmup + 100,000 measured iterations, identical stats
code in each language.

**Runs recorded** (June 26–27, 2026):

| run | what it measured |
|---|---|
| [20260626T103635Z](../journal/runs/20260626T103635Z-39abe130d644/entry.md) | First cross-host `network-rtt` run (tcp/udp/quic × rust/go/java) |
| [20260626T213457Z](../journal/runs/20260626T213457Z-deef392a8445/entry.md) | First `filesystem-write` run on local NVMe (fsync/fdatasync/prealloc/batch) |
| [20260627T071950Z](../journal/runs/20260627T071950Z-07a4b9a872fc/entry.md) | First `thread-handoff` run (spin/condvar/channel/ring); network + filesystem re-measured |
| [20260627T193417Z](../journal/runs/20260627T193417Z-003926ca6c91/entry.md) | Optimized SPSC ring (Rust + Go); full matrix re-measured — **current baseline** |

Unless noted, tables below show the **current baseline** run (20260627T193417Z).

---

## network-rtt — leader↔follower round trip (cross-host)

Strict ping-pong (one request outstanding) between two hosts, measuring the full
leader→follower→leader RTT for a 64 B message over TCP (`TCP_NODELAY`),
connected UDP, and QUIC (long-lived bidi stream).

| transport | rust p50 | go p50 | java p50 | rust p99 | go p99 | java p99 |
|---|---|---|---|---|---|---|
| tcp | 28.5 µs | 31.3 µs | 28.5 µs | 36.7 µs | 43.6 µs | 38.0 µs |
| udp | 27.9 µs | 28.6 µs | 27.5 µs | 37.5 µs | 39.7 µs | 36.1 µs |
| quic | 68.2 µs | 84.3 µs | 154.5 µs | 100.7 µs | 139.0 µs | 193.5 µs |

**What we learned:**

- **On a real network, TCP ≈ UDP and the languages are a wash** (~28–31 µs p50
  everywhere). The physical link + kernel round trip (~28–35 µs) dominates;
  the large per-language differences seen on loopback were kernel-parking
  artifacts, which is exactly why loopback numbers are never reported.
- **QUIC carries a fixed per-RTT premium**: roughly 2× TCP for Rust (quinn) and
  Go (quic-go), but ~5× for Java — the Kwik library adds ~125 µs per round trip
  and is the clear outlier. For an SMR hot path, tcp-vs-udp is not a
  performance decision; adopting QUIC costs ~2× RTT in Rust/Go and is expensive
  in Java today.
- Absolute RTTs moved ~15–25 % between fleet instantiations (e.g. tcp/rust p50
  36.0 → 34.6 → 28.5 µs across the three runs) with unchanged code — that's
  cross-instance cloud variance, and it's why comparisons are made against a
  journaled baseline rather than across arbitrary runs.

## filesystem-write — durable command-log appends (local NVMe)

Appending 64 B records to a log with durability, four strategies forming a
ladder: `fsync` (write + fsync each record), `fdatasync` (data-only sync),
`prealloc` (preallocated + fdatasync, no metadata updates), and `batch`
(group-commit: many records per sync).

| experiment | rust ops/s | go ops/s | java ops/s | sync p50 (rust) |
|---|---|---|---|---|
| fsync | 7,315 | 7,170 | 7,163 | 130 µs |
| fdatasync | 7,225 | 7,138 | 7,285 | 130 µs |
| prealloc | 22,181* | 25,203 | 24,831 | 37 µs |
| batch | 388,019 | 359,295 | 346,374 | 43 µs |

\* the Rust prealloc cell caught a slow tail this run (p99 198 µs vs ~46 µs for
go/java); earlier runs had all three at ~25 K ops/s.

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
| spin p50 | 248 ns | 194 ns | 220 ns |
| condvar p50 | 22.2 µs | **376 ns** | 24.0 µs |
| channel p50 | 22.8 µs | **301 ns** | 446 ns (mean 9.4 µs) |
| ring throughput | **410.8 M ops/s** | 41.9 M ops/s | 7.1 M ops/s |

**What we learned:**

- **Busy-wait spin is a ~200 ns floor everywhere** — with no scheduler
  involved, the three runtimes converge.
- **The headline is parking cost: Go is ~50–60× faster at sleep/wakeup.**
  Go parks goroutines in userspace (~300–400 ns per handoff); Rust and Java
  park real OS threads via futex, paying a syscall + kernel scheduler round
  trip of ~22–24 µs. This is the central thread sleep/wakeup story the focus
  area was built to expose.
- **Java's channel is bimodal**: `SynchronousQueue` often hands off without
  parking (p50 446 ns) but parks on a heavy tail (p99 ~25 µs), so its mean
  (~9 µs) sits far above its median. Rust's `mpsc` rendezvous parks every time.
- **The SPSC ring optimization (cache-line padding + LMAX-style cached
  opposite index) was the project's first optimization win**, graduated via
  the journal: Rust 28.1 M → **410.8 M ops/s (+1360 %)**, Go 9.8 M → **41.9 M
  ops/s (+327 %)**. Java kept its baseline (~7 M); the same pattern regressed
  its JIT'd `AtomicLong` path and was discarded.
- **Against an external yardstick** (same box, median-of-5): the optimized Rust
  ring hit ~367.6 M ops/s vs ~148.0 M for the `disruptor` crate v4.3 (BusySpin
  SPSC) — ~2.5× the full Disruptor framework for a bare `u64` handoff, since it
  skips handler dispatch, sequence barriers, and multi-consumer machinery.
  (Follow-up burst-mode comparison found disruptor faster at large bursts;
  both far exceed standard channels.)

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
   at wakeup than OS-thread parking in Rust/Java; if you can spin, all three
   reach ~200 ns, and a well-tuned Rust SPSC ring moves 400 M+ ops/s.
4. **Language choice matters least where the kernel or device dominates**
   (network RTT, disk sync) and most where the runtime owns scheduling
   (thread parking) or the compiler owns the inner loop (SPSC ring).
