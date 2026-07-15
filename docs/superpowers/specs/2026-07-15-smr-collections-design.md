# smr-collections — Limit-Order-Book State Store Comparison — Design

**Date:** 2026-07-15
**Status:** Proposed — awaiting review

## Purpose

A new focus area, **`smr-collections`**, that measures the **insert**, **update**, and
**persistence (snapshot)** cost of the canonical hot-state collection in a crypto
exchange: a **limit order book (LOB)**. It is compared across **Rust, Go, and
Java** — Java on **Agrona**, Rust and Go on hand-rolled equivalents.

It is the state-store analog of `serialization`: the same "flat, pool-allocated
in-memory state whose snapshot degenerates into a linear scan" philosophy, but the
artifact is a live LOB and the comparison is three-language. Where `serialization`
isolates codec cost on one journal record, `smr-collections` isolates the cost of
maintaining and snapshotting the deterministic in-memory **state** an SMR state
machine replicates.

The comparison answers three questions for the SMR hot path:

1. **Insert:** cost of placing a resting order (ladder + pool + id-index update).
2. **Update:** cost of amending / partial-filling an existing order (id lookup +
   in-place mutate + level bookkeeping).
3. **Persistence:** cost of serializing the whole book to a preallocated buffer and
   rehydrating it — the stop-the-world snapshot budget the design rests on. The
   snapshot format is **SBE**, reusing the repo's already-tested `serialization`
   toolchain (see §"Persistence uses the repo's SBE toolchain").

Scope: **three languages** (`rust`, `go`, `java`), unlike the Rust-only
`serialization` focus area.

## The canonical collection — a fixed-capacity flat LOB

Each language implements **one** canonical collection: a preallocated, single-writer,
zero-steady-state-allocation limit order book. The three operation experiments all
drive this same structure. It is composed of three parts.

### Price ladder

Two flat arrays `bids[NLEVELS]` and `asks[NLEVELS]`, indexed **directly by tick**
`t = (price − PRICE_MIN) / TICK`. Best-bid and best-ask are tracked as cursor
indices, maintained on insert/remove; ordered iteration by price is an array walk
(no sorted map needed — array index *is* price order). Each slot:

```
PriceLevel { head: u32, tail: u32, qty_total: i64, count: u32 }
```

`head`/`tail` are order-pool handles forming an intrusive FIFO (time priority within
a level). An empty level uses a sentinel handle (`u32::MAX` / `-1`).

### Order pool

A preallocated `Order[CAP]` array plus an intrusive free-list. Fixed layout,
`#[repr(C)]` (Rust) / equivalently packed (Go/Java), ~48 bytes:

```
Order { orderId: i64, price: i64, qty: i64, filled: i64, side: u8, next: u32, prev: u32 }
```

`next`/`prev` are **`u32` pool handles**: they thread the price level's FIFO list and
double as the free-list link when the slot is free. They are **indices, not
pointers** — this is the position-independence property (see §"Intrusive links").

### Id index (orderId → order)

A primitive, zero-boxing, open-addressed map, per language:

- **Java:** Agrona `Long2ObjectHashMap<Order>` over a pooled `Order[]` — the idiomatic
  Agrona use in matching engines (O(1) cancel/amend by orderId, no boxing, linear
  probing, backward-shift compaction on remove).
- **Rust:** a hand-rolled open-addressing `u64 → u32` handle map with a **fixed** hash
  (`nohash`/`FxHash`, no `RandomState`/SipHash), over the slab.
- **Go:** a hand-rolled open-addressing `int64 → int32` map over the pool (avoids
  builtin-map interface boxing, rehash, and GC pressure).

**Deliberate idiom asymmetry (recorded, not hidden):** Java holds *pooled object
references* in the map; Rust and Go hold *integer handles* into a slab. This reflects
each language's real hot-path idiom and is called out in results notes rather than
forced into artificial uniformity.

### Intrusive links: hand-rolled, not `intrusive-collections`

The Rust cell **hand-rolls** the intrusive FIFO/free-list as `u32` index links over
the slab; it does **not** use the `intrusive-collections` crate. That crate is
pointer-based (`LinkedListLink`/`RBTreeLink` store raw machine pointers via
`UnsafeRef`/`Box` adapters), which collides with two non-negotiables of this design:

1. **Position-independence / snapshottability.** Links must be `u32` pool indices so
   the order pool serializes to a **position-independent** image and restores into a
   *different address space*. Raw pointers are meaningless after restore.
2. **Flat `repr(C)` / POD layout.** `Order` must be a fixed-layout POD with
   `next/prev: u32` so it encodes as an SBE group of fixed blocks (a linear pass);
   `LinkedListLink` is an opaque pointer cell — not `Pod`, and it would pull
   ASLR-dependent addresses into the layout, breaking determinism.

Two softer reasons: it keeps Rust **parallel** to the hand-rolled Java/Go intrusive
pools (comparing equivalent implementations, not "a fancy crate vs a hand-rolled
slab"), and an index-linked FIFO over a slab is ~30–40 lines anyway. The crate would
be right for an in-memory-only engine that never snapshots to a relocatable image —
the one constraint this design does not have.

### Deliberate simplification: no matching engine

Inserts are **non-crossing (post-only)**, so every order rests. The benchmark
exercises *collection maintenance* (insert / amend / cancel / snapshot), not trade
execution or order crossing. Matching would branch into trade-generation logic
irrelevant to the collection comparison. This is a non-goal (see §Non-goals).

## Experiments (the operation axis) and metrics

Three thin driver artifacts per language, each over a shared per-language
`smr-collections-common` that owns the LOB, the snapshot codec, and the workload
generator. Each artifact emits result-contract lines via the language's existing
`bench-common` (Stats, percentiles, env config, timed loop, emit), aligned on
`(focus_area, experiment, language, metric)`.

| experiment | drives | metrics (unit) |
|------------|--------|----------------|
| `insert`   | place resting orders into a steady-state book (ladder + pool + id-index) | `insert_p50`, `insert_p99`, `insert_mean` (ns) |
| `update`   | amend / partial-fill an existing order: id-index lookup → mutate fields → adjust level `qty_total` | `update_p50`, `update_p99`, `update_mean` (ns) |
| `snapshot` | serialize the whole book → preallocated buffer, and restore → fresh book | `snapshot_p50/p99/mean`, `restore_p50/p99/mean` (ns); `snapshot_bytes` (bytes); `snapshot_throughput` (bytes_per_sec) |

All lines carry `focus_area:"smr-collections"`, the `experiment`, the `language`, and
one line per metric.

## Persistence uses the repo's SBE toolchain

The snapshot format is **SBE**, driven from a single schema and generated per
language by the **already-vendored, already-tested** real-logic `sbe-all-1.38.1.jar`
(at `rust/serialization/aeron_sbe/vendor/`). The jar carries `RustGenerator`,
`GolangGenerator`, and the Java generator, so **one schema produces codecs for all
three languages**. This is the exact recipe the source note recommends —
"SBE-framed sections per pool with a schema version and a trailing checksum" — and it
reuses the `serialization` focus area's proven infrastructure rather than
hand-rolling a new framing.

Build integration mirrors the `serialization/aeron_sbe` cell precisely: the generated
codec is **committed** to the repo and regenerated one-shot via a `regen.sh` that
needs a JDK **only at regen time**. Normal `cargo build` / `go build` / `./gradlew
build` consume the committed output — **no build-time JDK dependency**. The Java SBE
generator emits flyweights over Agrona `DirectBuffer`/`MutableDirectBuffer`, a natural
pairing with the id-index already on Agrona.

The Rust snapshot cell uses the real-logic-generated Rust codec (as `aeron_sbe`
does). Because SBE output is byte-deterministic, the Rust cell could instead use the
faster pure-Rust `sbe_gen` zero-copy codec from the `serialization` focus area with
**byte-identical** output (the equality already asserted there); that swap is an
optional optimization, not required by this spec.

### Snapshot schema and the rebuild trick

`book_snapshot.xml` (little-endian, versioned via the SBE message header):

```
header block   PRICE_MIN, TICK, NLEVELS, CAP, high-water mark,
               free-list head, best-bid/ask cursors, order count
group levels   per occupied price level: tick, side, qty_total, count, head, tail
group orders   per live order (pool-slot order): slot, orderId, price, qty,
               filled, side, next, prev
trailer        crc32c checksum over the encoded message
```

**The id-index is not serialized — it is rebuilt on restore by scanning the decoded
orders.** The authoritative state is the flat ladder + pool (inherently ordered); the
orderId→order map is a *rebuildable index*. This is the design pivot that lets Go's
randomized builtin-map iteration order pose no determinism threat and lets each
language pick its id-map on performance grounds alone. Encoding orders in **pool-slot
order** with their `slot`/`next`/`prev` handles preserves free-list order and the
exact per-level FIFO (time priority), so handle identity survives the round trip.

Encoding walks the occupied levels and the live pool as SBE repeating groups — a
linear, cache-friendly pass close to a memcpy for these POD blocks. Snapshot writes
into a **reused, preallocated** buffer (no per-snapshot allocation). `snapshot_bytes`
is the memory metric; `snapshot_throughput` = `snapshot_bytes / snapshot_mean`.

Because the format is SBE, the same book produces a **byte-identical snapshot across
all three languages** — a tested guarantee (see §Determinism), and the reason
`snapshot_bytes` is directly comparable across the grid.

## Deterministic workload — identical across languages

The order stream (ids, prices, sizes, and the op sequence) is generated by
**splitmix64 seeded from a fixed constant**, reimplemented *identically* in all three
languages so the three books receive the **same** stream — making insert / update /
snapshot numbers directly comparable. No `Math.random` / `Date.now` (project
discipline); the stream is a pure function of the iteration index and is
byte-reproducible.

Env knobs (parsed via each language's `bench-common` config, with defaults):

| knob           | meaning                                   |
|----------------|-------------------------------------------|
| `SMRC_CAP`     | order-pool capacity (max resting orders)  |
| `SMRC_LEVELS`  | `NLEVELS` (ladder size per side, in ticks)|
| `SMRC_TICK`    | tick size                                 |
| `SMRC_STEADY`  | steady-state resting-order count          |
| warmup / iters | shared bench-common loop controls         |

Defaults are sized so the book is realistically dense (the ladder well-occupied and
the pool at a meaningful fraction of `CAP`) rather than sparse.

Phases per experiment:

- **`insert`:** warm up, then insert the measured stream into a book pre-grown near
  steady state, timing each placement.
- **`update`:** pre-build to `SMRC_STEADY`, then apply the measured amend/fill stream
  to deterministically-selected existing orders, timing each.
- **`snapshot`:** pre-build to `SMRC_STEADY`, then repeatedly snapshot→buffer and
  restore→fresh book, timing each and reporting `snapshot_bytes`.

## Determinism as a tested invariant

- **Reproducible snapshot:** the same op sequence produces a **byte-identical**
  snapshot (property test, per language).
- **Round-trip fidelity:** snapshot → restore → re-snapshot yields identical bytes;
  after restore, book queries (best bid/ask, per-level `qty_total`, id lookups) are
  identical, and free-list order is preserved.
- **Fixed hashing only:** no `RandomState`/SipHash/address-based hashing in the
  id-index.
- **Cross-language byte-identity (now a guarantee, not a non-goal):** because the
  snapshot is SBE — a deterministic, little-endian wire spec consumed from one schema
  by all three languages — the same book produces a **byte-for-byte identical**
  snapshot in Rust, Go, and Java. A golden-bytes test asserts this across the grid
  (the same fairness anchor the `serialization` focus area uses for its two SBE
  cells).

## Layout and build integration

The snapshot SBE codec is **generated once and committed** per language (mirroring
`serialization/aeron_sbe`): a shared `smr-collections/schema/book_snapshot.xml` plus a
`regen.sh` per language that shells the vendored `sbe-all-1.38.1.jar` with the
matching `-Dsbe.target.language={Rust,Golang,Java}`. Regen needs a JDK; ordinary
builds do not.

- **Rust** — new Cargo members under `rust/smr-collections/`:
  ```
  rust/smr-collections/
    schema/         # book_snapshot.xml (single source of truth for all 3 langs)
    common/         # smr-collections-common: LOB (ladder, pool, OA id-map, intrusive
                    # links), splitmix64 workload gen, SMRC_* config
    sbe/            # committed real-logic Rust codec + regen.sh (RustGenerator)
    insert/         # smr-collections-insert   binary
    update/         # smr-collections-update   binary
    snapshot/       # smr-collections-snapshot binary
  ```
  Members inherit `[workspace.package]`; the id-map hasher dep (`nohash`/`rustc-hash`)
  and checksum dep (`crc32c`) go in `[workspace.dependencies]`, scoped to this focus
  area. Workspace stays clippy- and rustfmt-clean (regen re-applies the workspace
  manifest + `cargo fmt`, as `aeron_sbe/regen.sh` does).
- **Go** — `go/internal/smrcoll` (the LOB package) + committed SBE Go codec (from
  `GolangGenerator`, e.g. `go/internal/smrcoll/sbe/`, regenerated via a script) +
  `go/cmd/smr-collections-{insert,update,snapshot}`, each thin over `internal/bench`.
- **Java** — `:smr-collections-common` (LOB + Agrona dependency + committed SBE Java
  flyweights from the Java generator, which target Agrona buffers) and
  `:smr-collections-{insert,update,snapshot}` subprojects over `:common`. Add the
  subprojects to `settings.gradle.kts`; add Agrona to `:smr-collections-common` only.

Each language reuses its shared `bench-common` for Stats / config / timed loop /
emit — no new emission or Stats code.

## Tests

- **LOB invariants:** after arbitrary insert/amend/cancel sequences, each level's
  `qty_total` equals the sum of its resting orders' `(qty − filled)`, best-bid <
  best-ask, and the id-index is consistent with the pool.
- **Snapshot round-trip:** restore reproduces every query result and the free-list
  order; re-snapshot is byte-identical.
- **Determinism property test:** same op sequence on two fresh instances ⇒ identical
  serialized bytes.
- **Cross-language SBE byte-identity:** a committed golden snapshot (for a fixed op
  sequence) is asserted equal in all three languages — the fairness anchor that keeps
  `snapshot_bytes` comparable across the grid.
- **Fuzz the deserializer (Rust):** corrupt / truncated images are rejected without
  UB (Miri over the `unsafe` layout code where applicable).
- **Stats** reuse from `bench-common` (already tested); no new Stats code.

## bench-infra

Add three rows to `bench-infra/ansible/group_vars/all.yml`'s `experiments` matrix —
`smr-collections-insert`, `smr-collections-update`, `smr-collections-snapshot` — all
**single-host on node0** (no responder, no NVMe requirement). Journaling follows the
project rule: only **real AWS single-host runs** are recorded via `tools/journal`;
local runs are fitness checks only.

## Result-contract compliance

- stdout carries **only** result lines; logs/progress/diagnostics go to stderr.
- One line per metric; every line carries `focus_area`, `experiment`, `language`, and
  `metric`.
- The `tools/journal` CLI aligns on `(focus_area, experiment, language, metric)` with
  no new per-language knowledge.

## Non-goals

- **No matching engine / trade execution.** Inserts are non-crossing; the benchmark
  measures collection maintenance, not order crossing.
- **No on-disk snapshot.** Serialize-to-buffer only; durability and the disk axis are
  `filesystem-write`'s job. The state store needs no WAL/fsync (the log owns
  durability).
- **No concurrency.** Single-writer `&mut`/single-thread ownership is the model; no
  atomics/locks on the hot structures.
- **No use of `intrusive-collections`** (pointer-based → not position-independent /
  snapshottable); intrusive links are hand-rolled `u32` indices.
- **No cross-language allocation metric.** Steady-state zero-allocation is asserted by
  tests; the only reported memory number is the uniform `snapshot_bytes`. A
  per-language `snapshot_alloc_bytes` is a possible future add, out of scope here.
