# smr-collections — MVCC Variants + ultima_db Cell — Design

**Date:** 2026-07-26
**Status:** Proposed — awaiting review

## Purpose

The existing `smr-collections` cells measure a **stop-the-world** store: the
snapshot experiment serializes a frozen book with no writer anywhere in the
process (`&Book` in Rust, no locks in Go/Java). That leaves the central
snapshot-strategy trade-off unmeasured: **what does the writer pay while a
snapshot is in flight?** For the STW store the answer is "the entire serialize"
(~0.6 ms Rust, ~5 ms Go at the default 2.75 MB image); an MVCC store's answer
is "an O(1) root capture plus copy-on-write while the serializer runs
concurrently".

This extension adds:

1. An **MVCC variant of the LOB store** — a **chunked copy-on-write book**
   (`CowBook`) — hand-rolled with the same design in **Rust, Go, and Java**,
   mirroring how the STW store is hand-rolled per language.
2. A **`live` experiment family**: snapshot-under-live-writes, run against
   each store variant, measuring writer-observed latency (p50/p99/**max**)
   and snapshot duration concurrently.
3. An **ultima_db competitor cell** (Rust only): the same experiments driven
   through [ultima_db](https://github.com/PeterKnego/ultima_db), an MVCC store
   built on a persistent CoW B-tree, used in its **SMR pattern** —
   single-writer, explicit versions (op index = log position), snapshot =
   read-txn at a version. ultima_db is a *competitor in the grid*, not a
   building block for the other implementations.

The grid then spans three points in the design space: flat STW (pay nothing
steady-state, stall on snapshot), flat chunked-CoW (small steady-state tax,
near-zero stall), and tree-based engine MVCC (bigger steady-state tax, O(1)
snapshot by construction).

Out of scope: disk IO (the ultima_db cell serializes to a memory buffer, not
its checkpoint-to-disk path — durability cost is the `filesystem-write` focus
area's job), and workload churn (no cancel/remove op is added; the existing
bit-identical splitmix64 op stream is reused unchanged, so **version GC under
churn is deliberately unmeasured** — noted here so nobody reads the MVCC
numbers as including it).

## Grid

Nine new experiments, one artifact per experiment per the repo convention
(`smr-collections-<experiment>`):

| experiment | languages | measures |
|---|---|---|
| `mvcc_insert`, `mvcc_update`, `mvcc_snapshot` | rust, go, java | today's workloads + metrics on `CowBook` — steady-state MVCC overhead |
| `ultima_insert`, `ultima_update`, `ultima_snapshot` | rust | same, on ultima_db |
| `live_stw`, `live_mvcc` | rust, go, java | snapshot under live writes, per store variant |
| `live_ultima` | rust | same, on ultima_db |

The existing `insert`/`update`/`snapshot` cells are untouched and remain the
STW baseline. `mvcc_*`/`ultima_*` emit the **same metric names** as their STW
counterparts (`insert_p50/p99/mean`, …, `snapshot_bytes`,
`snapshot_throughput`) so a cross-variant comparison is a row-by-row read in
RESULTS.md.

## CowBook — the chunked copy-on-write LOB

Same flat layout as `Book` (per-side ladder, order pool, intrusive per-level
FIFO via u32 slot handles, cached best-bid/best-ask), with two changes:

**Chunked backing storage.** The order pool and each side's ladder are split
into fixed-size chunks referenced through a chunk table:

- Order pool: `SMRC_CHUNK` orders per chunk (default **4096** → 64 chunks at
  the default `SMRC_CAP` 262144). `SMRC_CAP` need not divide evenly; the last
  chunk is partial.
- Ladder: 256 levels per chunk (constant, not env-tunable) → 4 chunks per
  side at the default 1024 levels.
- Slot → chunk addressing is `slot / CHUNK`, `slot % CHUNK` (or shift/mask
  when the size is a power of two, which both defaults are).

A snapshot **root** is `{order-chunk refs, bid-chunk refs, ask-chunk refs,
hwm, bestBid, bestAsk}`. The id-map is **not versioned**: it is not serialized
today (restore rebuilds it by scanning slots), so the writer keeps its plain
map and the root omits it.

**Epoch-based copy-on-write.** Uniform design across languages:

- Each chunk carries a `born_epoch`. A global `snap_epoch` is bumped when a
  snapshot root is captured.
- Before its first write to a chunk with `born_epoch <= last snap_epoch`, the
  writer copies the chunk (memcpy of ≤ `SMRC_CHUNK` × ~48 B), installs the
  copy in its chunk table with `born_epoch = current`, and writes into the
  copy. Subsequent writes to that chunk are direct.
- Reclamation is language-native: chunks are `Arc<Chunk>` in Rust, plain
  object references in Go/Java (GC collects a chunk when the last root
  referencing it drops). The *copy decision* is the epoch — never
  `Arc::strong_count` — so the semantics are identical in all three
  languages.

**Op-boundary capture (SMR semantics).** A snapshot is taken at a log
position, so capture happens between ops, never mid-op:

- The snapshot requester sets an atomic `snap_requested` flag.
- The writer checks the flag once per op (single relaxed atomic load). On
  seeing it set, the writer clones the chunk-ref tables (~70 refs at
  defaults), snapshots the scalars, bumps `snap_epoch`, publishes the frozen
  root to a handoff slot, and clears the flag.
- No lock on the hot path, no torn roots: the serializer only ever sees a
  root assembled at an op boundary by the single writer.

In the single-threaded `mvcc_*` experiments the flag never fires, so they
measure the pure steady-state tax: chunk-table indirection + one epoch check
per write.

**Encoding.** `CowBook`'s encoder walks a frozen root in exactly the STW
encoder's order (occupied levels bids-then-asks ascending, orders 0..hwm),
producing the same `book_snapshot.xml` SBE image + crc32c trailer. **For the
same logical state, CowBook's snapshot bytes are byte-identical to Book's** —
the existing `golden_snapshot.bin` therefore verifies the new store with no
new golden artifact. Restore rehydrates fresh chunks and rebuilds the id-map,
mirroring the STW restore.

## The live experiments

Shape (identical across variants, deterministic):

1. Pre-build the steady book (`SMRC_STEADY` orders, untimed), as the update
   experiment does.
2. Writer applies `SMRC_LIVE_ITERS` updates (default **200000**) from the
   existing `next_update` stream, timing each op.
3. Every `SMRC_SNAP_EVERY` ops (default **20000** → 10 snapshots/run) a
   snapshot is triggered at that op boundary.

Per variant:

- **`live_stw`** — the writer serializes inline at the trigger (the Aeron
  Cluster model). The stalled op's measured latency *is* the stall.
- **`live_mvcc`** — a serializer thread encodes from the frozen root; the
  writer pays capture handoff plus CoW copies as it re-dirties chunks.
- **`live_ultima`** — the writer applies committed write-txns; the serializer
  thread does `begin_read` at the captured version and encodes from it.

A trigger fired while the previous serialize is still running is skipped and
counted (`snap_skipped`); at the default cadence (~20 000 × ~100 ns ≈ 2 ms
between triggers vs sub-ms serializes) skips should be zero on the bench
hosts.

Metrics (per line, existing vocabulary; latencies in ns):

| metric | meaning |
|---|---|
| `writer_p50`, `writer_p99`, `writer_max` | writer per-op latency over the whole run; **`writer_max` is the headline** — a 1-in-20000 stall is invisible at p99 |
| `snapshot_mean` | full serialize duration (trigger/capture → encoded+crc) |
| `snap_count`, `snap_skipped` | snapshots taken / triggers skipped (int) |
| `snapshot_bytes` | encoded image size (samples=1) |

Two threads maximum (writer + serializer), well within the c6id.2xlarge
node0. Thread handoff of the frozen root uses the simplest correct primitive
per language (Rust `Mutex<Option<Root>>` + condvar or channel; Go channel;
Java `SynchronousQueue`/exchange) — the handoff is off the per-op hot path
and its cost lands in `snapshot_mean`, not writer latency.

## The ultima_db cell

**Dependency.** Pinned git dependency, declared **only** in the ultima
artifacts (repo dependency-locality rule), added to
`[workspace.dependencies]`:

```toml
ultima_db = { git = "https://github.com/PeterKnego/ultima_db.git", rev = "b48295e9ad6ba6e54100a6e8ec9248c8e84e09c3" }
```

The repo is public — anonymous fetch works on the AWS nodes. The
`persistence` cargo feature is **not** enabled: with in-memory-only
measurement there is no checkpoint dir, so `Persistence` never enters the
picture and the dependency stays light. "SMR mode" here means the SMR *usage
pattern* ultima_db documents for consensus deployments: SingleWriter store,
explicit commit versions supplied by the caller (the op index, i.e. the log
position), snapshot = `begin_read` at a version.

**State mapping.** Two tables:

- `orders`: key `orderId: i64` → record `{slot: u32, price: i64, qty: i64,
  filled: i64, side: u8, next: u32, prev: u32}` — the flat record verbatim,
  intrusive links included (they are just replicated state bytes).
- `levels`: key `(side as i64) << 32 | tick` → record `{qtyTotal: i64,
  orderCount: u32, head: u32, tail: u32}`.
- Scalars (`hwm`, `bestBid`, `bestAsk`) in a one-row meta table, written in
  the same commit as the op that changes them.

Each insert/update is one write-txn touching its order + level (+ meta)
records, committed at version = op index.

**Encoding.** OrderIds are assigned monotonically (`orderId = i+1`), so
ascending-key iteration of `orders` reproduces slot order 0..hwm; the level
key sorts side-major then tick-ascending, matching the STW encoder's
bids-then-asks lane order. The ultima cell therefore also targets
**byte-identical golden bytes** through the same SBE encoder.

**Restore** (`ultima_snapshot`'s `restore_*` metrics): decode the SBE bytes
and rebuild a fresh store — orders/levels/meta written back via write-txns
(bulk-load path if it proves materially faster; either way the same work is
timed for every variant: bytes → queryable store).

**Caveat recorded in RESULTS when written up:** per-op cost includes tree
path-copying and txn commit — µs-scale against the flat store's tens of ns.
That gap *is* the finding (the steady-state price of engine MVCC), not an
implementation accident.

## Testing

- **Equivalence property test** (per language): same op stream applied to
  `Book` and `CowBook` → byte-identical snapshot bytes. Extends the existing
  determinism tests.
- **Golden test reuse**: `CowBook` in all three languages and the ultima
  adapter (Rust) encode the golden config
  (`cap=4096, levels=64, tick=1, priceMin=0, steady=2000`) and must match
  `rust/smr-collections/testdata/golden_snapshot.bin` exactly.
- **Concurrent-capture test** (per language, `live_mvcc` machinery): run the
  live loop with a snapshot at op *k* while writes continue; captured bytes
  must equal a single-threaded STW encode of a book replayed to exactly op
  *k*. This is the test that proves "without stopping writes" is *correct*,
  not just fast. Run it for the ultima adapter too (Rust).
- **Restore round-trip + crc-rejection** tests mirror the existing ones for
  `CowBook` restore.
- Go race detector (`go test -race`) over the live/concurrent tests; Rust
  under the existing clippy/fmt gates.

## Config

New env vars, parsed in the shared per-language bench libraries alongside the
existing `SMRC_*` set (same validation style — hard-error on malformed,
strictly-positive):

| var | default | notes |
|---|---|---|
| `SMRC_CHUNK` | 4096 | orders per CoW chunk |
| `SMRC_LIVE_ITERS` | 200000 | timed writer ops in `live_*` |
| `SMRC_SNAP_EVERY` | 20000 | ops between snapshot triggers; must be ≤ `SMRC_LIVE_ITERS` |

Existing vars (`SMRC_CAP`, `SMRC_LEVELS`, `SMRC_TICK`, `SMRC_PRICE_MIN`,
`SMRC_STEADY`, `SMRC_WARMUP`, `SMRC_ITERS`) apply unchanged; `live_*` uses
`SMRC_STEADY` for the pre-build and does not consume `SMRC_WARMUP`/
`SMRC_ITERS`.

## Infra & docs changes

- `bench-infra/ansible/group_vars/all.yml`: nine matrix rows (`kind: local`;
  the three ultima rows language-filtered to rust), params block gains
  `smrc_chunk`, `smrc_live_iters`, `smrc_snap_every`.
- `bench-infra/ansible/roles/run/tasks/local.yml`: export the three new vars.
- Registrations: Rust workspace members (9 crates: 5 CowBook/live + 4
  ultima), Go `cmd/` dirs (5), Java subprojects (5). Common code lands in the
  existing shared spots: `rust/smr-collections/common` (`cowbook.rs`,
  ultima adapter in its own crate `rust/smr-collections/ultima-common/` so
  the git dep stays out of `common`), `go/internal/smrcoll/cowbook.go`,
  `java/smr-collections-common` (`CowBook.java`).
- `CLAUDE.md`: artifact-name list + smr-collections status paragraph.
- **Drive-by docs fix**: `README.md` and `CLAUDE.md` claim "Rust/Go use a
  hand-rolled open-addressing id-map"; Rust actually uses std `HashMap` with
  an identity hasher — only Go hand-rolls it. Correct both lines.
- `docs/RESULTS.md` gains its smr-collections section only after a real AWS
  run is journaled (per the journaling rules — the 2026-07-15 run plus the
  first run including these cells).

## Open items deliberately deferred

- Version GC under churn (needs a cancel/remove op and a free-list — a
  workload-contract change touching all variants and the golden format).
- Incremental/dirty-region snapshots (the chunk table is the natural hook,
  but the repo's notes argue full snapshots + log suffix; revisit only with
  data).
- ultima_db checkpoint-to-disk (`Persistence::smr(dir)`) as a separate
  IO-inclusive metric.
