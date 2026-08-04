# smr-collections — background research notes

Q&A notes saved 2026-07-15, the research that seeded
`docs/superpowers/specs/2026-07-15-smr-collections-design.md`. Kept verbatim
(including the original question typos) as the design background for the
focus area: the snapshot-strategy survey maps to the `snapshot`/`mvcc_*`/
`ultima_*` cells, the Agrona tour to the Java implementation, and the
six-cluster requirements list was largely honored by the implemented grid
(`freeHead` capture, cross-language goldens, the churn replay byte-identity
test, Miri on the unsafe layout code). Unbuilt items that could seed future
cells: dirty-tracking/incremental snapshots, mmap-and-adopt restore, fuzzing
the deserializer.

---

question: descripe collections for storage/snapshotting of state machine 

Ah — for the state itself, the pattern is **flat, pool-allocated in-memory structures designed so that snapshotting degenerates into a linear scan**, with the snapshot mechanism chosen around one question: how do you get a consistent point-in-time image without stalling the hot path?

**Memory layout comes first.** The business state (order books, accounts, positions) is kept in pre-allocated pools/arenas with index-based references instead of pointers — in Java-land this means Agrona primitive maps, object pools, and zero steady-state allocation; in Rust it's slabs/arenas with `u32` handles. This serves latency (cache locality, no GC/allocator activity), but it also makes snapshots cheap: state that lives in a few contiguous pools serializes as a handful of near-memcpy passes rather than a pointer-chasing graph traversal. Serialization is flat binary (SBE or hand-rolled), versioned, and position-independent so a snapshot restores by rehydrating the pools directly.

**Snapshot strategies, roughly in order of prevalence:**

*Inline stop-the-world on the log position.* The state machine pauses log consumption at a known position, serializes state, resumes. This is Aeron Cluster's model: the snapshot is taken at an exact log index, written as a stream to the archive, and correctness is trivial because nothing mutates during the write. The trick that makes it viable is that serializing a few hundred MB of flat pools takes tens to low hundreds of milliseconds — acceptable if snapshots are infrequent (minutes/hours) and, crucially, if you take them on a **follower or a dedicated snapshot replica** rather than the leader. Since every replica is deterministic and at the same state for a given log position, any node's snapshot is valid for all. Exchanges commonly run a non-voting replica whose whole job is snapshotting and reporting.

*fork() + copy-on-write.* Redis BGSAVE style: fork, the child serializes a frozen image while the parent keeps processing, the kernel COWs dirtied pages. Zero logical pause, but the page-fault storms on the parent add latency jitter, memory can transiently double under write-heavy load, and it's awkward with threads. Fine for mid-tier systems, generally avoided when you're chasing single-digit-microsecond determinism.

*Persistent/MVCC data structures.* If the state lives in versioned structures (CoW B-trees, functional maps), a snapshot is an O(1) root capture and serialization proceeds concurrently against the frozen version. This is elegant — and it's essentially what an MVCC engine like UltimaDB gives you for free: `begin_read_txn()` at the log position *is* the snapshot handle. The cost is the steady-state overhead of versioning (indirection, allocation, GC of old versions) on every mutation, which pure-array designs refuse to pay. So you see this where state is naturally tree-shaped or where the engine already exists, less in hand-tuned matching engines.

*Double-buffer/shadow state.* Maintain two copies, atomically flip, serialize the quiesced one. Simple and pause-free but 2× memory and 2× write cost; rare beyond small state.

**Incremental snapshots** (persisting only dirty regions since the last snapshot) exist but are uncommon in this space — they reintroduce dependency chains at recovery and complicate the "any replica can bootstrap from snapshot + log suffix" property. The prevailing view is: keep state compact enough that full snapshots stay cheap, and let the log do the incremental work.

**Recovery** is then snapshot load (rehydrate pools, ideally at near-sequential-read speed from NVMe — single-digit seconds for a few GB) plus deterministic replay of the log suffix. Snapshot cadence is tuned to bound replay time, not to bound data loss — durability is the log's job, which is why the state store itself needs no WAL, no fsync discipline, and no crash consistency of its own. It's a cache with a serialization method.

For a matching engine specifically, the practical recipe is: books and orders in pooled arrays with intrusive free-lists, snapshot on a follower at a log position, SBE-framed sections per pool with a schema version and a trailing checksum, and a hard budget on total state size so the whole cycle stays boring.


-----------
Question: expand on Agrona primitive maps, object pools, et.. and it's features


Agrona is Real Logic's foundation library — the substrate under Aeron, SBE, and Artio — and it's essentially a catalog of "what the JDK would look like if it were designed for zero-allocation, mechanical-sympathy code." Worth knowing piece by piece, because most of it maps directly to patterns you'd replicate in Rust anyway.

**Primitive collections.** The core offering: `Int2ObjectHashMap`, `Long2ObjectHashMap`, `Int2IntHashMap`, `Long2LongHashMap`, `Object2IntHashMap`, `IntHashSet`, `LongHashSet`, plus counter maps. What distinguishes them from `HashMap<Long, Order>`:

- *No boxing.* Keys (and values in the primitive-primitive variants) are stored in raw `int[]`/`long[]` arrays. A JDK `HashMap<Long, Long>` allocates a `Long` box per key, per value, plus an `Entry` node per mapping — three heap objects and three pointer hops per entry. Agrona's maps allocate nothing per entry, ever.
- *Open addressing with linear probing* instead of chained buckets. Entries live inline in the backing arrays, so a lookup is: hash, jump to slot, scan forward. The probe sequence walks contiguous memory, which is exactly what the prefetcher wants. Chained maps chase a pointer per collision — each one a potential cache miss.
- *Power-of-two capacity with mask-based indexing* (`hash & mask` rather than `hash % capacity` — no integer division on the hot path).
- *A "missing value" sentinel* instead of null/boxing for primitive-valued maps: you designate e.g. `-1` or `Long.MIN_VALUE` as "absent," and `get()` returns it directly. No `Optional`, no nullable box.
- *Reusable iterators.* This is subtle but critical: `map.values().iterator()` on Agrona collections returns a cached iterator instance that's reset on each call, not a fresh allocation. JDK iteration allocates an iterator object every loop — invisible in normal code, a steady GC drip at a few million iterations per second. (The trade: iteration isn't reentrant or thread-safe, which the single-writer principle makes irrelevant.)
- *Compaction on remove.* Open addressing with removal normally needs tombstones; Agrona instead re-slots displaced entries on remove, so probe chains never rot and lookup cost stays flat over time.

For a matching engine, the canonical use is `Long2ObjectHashMap<Order>` for orderId → order lookup, with the `Order` objects themselves coming from a pool — so the map holds references into the pool and steady-state operation touches the allocator never.

**Object pools and the flyweight discipline.** Agrona itself is light on prescriptive pool classes — the idiom is more important than the class. The pieces it gives you: `ArrayDeque`-style free lists you build over its collections, and *flyweights*, which are the deeper pattern. An SBE/Agrona flyweight is a stateless view object that wraps a buffer at an offset — `orderFlyweight.wrap(buffer, offset)` — so "objects" in messages or in the state store are never materialized; you have one flyweight instance per thread reused across millions of records. Combined with pooling of the genuinely stateful domain objects (acquire on order entry, release on fill/cancel, intrusive free-list via an index field), the steady-state allocation rate is zero, GC pauses vanish because there's nothing to collect, and — the point relevant to your snapshot question — the live state sits in a bounded set of arrays you can serialize linearly.

**Buffers.** `DirectBuffer`/`MutableDirectBuffer`/`AtomicBuffer` interfaces with `UnsafeBuffer` as the workhorse implementation: a uniform API over heap arrays, direct ByteBuffers, or raw addresses (mmap'd files), with unchecked-or-checked bounds (toggleable via system property), explicit byte order, and ordered/volatile/CAS accessors on `AtomicBuffer`. This is what lets the same codec run over a network buffer, a journal page, and an IPC segment. `ExpandableArrayBuffer` and `ExpandableDirectByteBuffer` for the rare grow-on-demand cases, `MarkFile` for mmap'd liveness/metadata files (how Aeron drivers and clients discover each other's heartbeats).

**Concurrent structures.** `OneToOneRingBuffer` and `ManyToOneRingBuffer` — Disruptor-lineage MPSC/SPSC queues over an `AtomicBuffer`, so they work across processes via shared memory, with variable-length records and a claim/commit protocol. `BroadcastTransmitter`/`BroadcastReceiver` for one-to-many where slow readers lose (telemetry, market data fan-out to non-critical consumers). Plus padded/cache-line-aware primitives to kill false sharing.

**The Agent framework.** `Agent` (a `doWork()` duty cycle), `AgentRunner` (binds an agent to a thread), `CompositeAgent` (several agents share one thread), and `IdleStrategy` implementations — `BusySpinIdleStrategy`, `YieldingIdleStrategy`, `BackoffIdleStrategy`, `SleepingIdleStrategy` — encoding the latency-vs-CPU spectrum you've looked at with SO_BUSY_POLL: spin for lowest wakeup latency, back off progressively when you can afford it. An entire exchange core is typically three or four agents (ingress, engine, egress, archiver) on pinned cores.

**Odds and ends that earn their keep:** `CachedEpochClock`/`CachedNanoClock` (read time once per duty cycle instead of per event — `System.nanoTime()` is ~20-25ns, and determinism wants log-injected time anyway), `CountersManager` (counters in a shared mmap'd file, so monitoring reads process stats without touching the process — Aeron's stat mechanism), `BitUtil`/`BufferUtil` alignment helpers, and `RingBufferDescriptor`-style layout conventions.

The Rust translation, since that's where UltimaDB lives: the primitive maps' role is played by `nohash`/`FxHashMap` over `u32` handles or just direct slab indexing; flyweights become zero-copy views (`zerocopy`, `bytemuck`) over arenas; pools become slabs with generational indices; `UnsafeBuffer` is roughly `&[u8]` + explicit-endian codecs over mmap. The interesting part is that half of Agrona exists to *escape* Java's object model — boxing, headers, pointer graphs, GC — and Rust doesn't need escaping. What transfers unchanged is the layout discipline: index-based references, contiguous pools, single-writer ownership, and cached iteration state.


-----------
Question: if we would be creating specialized collections for state machine storage/snapshotting in Rust, what would be a list of required features


Good framing — treat it as a spec. The requirements fall into six clusters, and the interesting ones are where "fast collection" and "deterministic, snapshottable collection" diverge from what crates.io gives you.

## 1. Determinism (the non-negotiable cluster)

- **Deterministic iteration order** — identical across replicas and across snapshot/restore cycles, regardless of insert/remove history. Either index-ordered or insertion-ordered iteration; the arbitrary "wherever probing put it" order of a plain open-addressed map leaks nondeterminism into anything that iterates.
- **Fixed hashing** — no `RandomState`/SipHash seeding, no address-based hashing, no ASLR sensitivity. Hash of a key must be a pure function of its bytes, ideally trivial (`nohash` on integer keys/handles).
- **Deterministic allocation** — the pool's free-list must hand out slots in an order that's a pure function of operation history, because handles become order IDs, book references, etc., and any divergence there is state divergence. This includes after restore: rebuilding a pool from a snapshot must reproduce the exact free-list order, not just the live entries.
- **No hidden time, randomness, or capacity-triggered behavior differences** — an op must behave identically whether the map is 10% or 90% full (modulo latency).

## 2. Memory & latency behavior

- **Preallocated, fixed capacity** — sized at startup, fail-fast (return `Err`, trigger backpressure/reject) on exhaustion rather than grow. No rehash, no realloc, ever; this also kills the amortized-O(1) latency spike problem at the root.
- **Zero steady-state allocation** — inserts, removes, lookups, and iteration touch the allocator never; iterators are by-value cursors, not boxed.
- **Contiguous, cache-friendly layout** — open addressing with linear probing, slot data inline; optional SoA splitting so hot fields (price, qty) scan without dragging cold fields through cache.
- **Tombstone-free removal** — backward-shift deletion so probe chains never degrade and lookup cost is flat over uptime (the Agrona compaction property).
- **Single-writer, no interior synchronization** — no atomics, no locks, no `Sync` obligations on the hot structures; `&mut` ownership by the engine thread is the concurrency model.

## 3. Identity & references

- **Handle-based, position-independent references** — `u32` indices into pools instead of pointers, so the entire state graph is meaningful after memcpy into a different address space. This is what makes snapshots trivially relocatable.
- **Generational indices (debug-configurable)** — catch use-after-free of handles in test builds; optionally compiled out to bare `u32` in release if the 4 bytes matter.
- **Handle stability across snapshot/restore** — a handle valid at log position N must resolve to the same logical entity after restore from a snapshot at N. Falls out naturally if identity is the index, but must be a tested invariant.
- **Intrusive linkage** — free-list next-pointers and secondary-structure links (e.g., price-level order queue) stored as index fields inside the slot, so pool + links serialize as one array.

## 4. Snapshot & serialization

- **`Pod`/`zerocopy`-compatible slot layout** — `#[repr(C)]`, explicit endianness, no niches/padding surprises, so serialization of a pool is one bounds-checked memcpy (or a per-slot fixup pass at worst).
- **Serialize from `&self`** — snapshotting takes a shared borrow at a log position; Rust's borrow checker then *proves* the stop-the-world consistency (nothing can mutate while the snapshot borrow lives). This is the cheap, honest alternative to CoW.
- **Composite atomic snapshot** — the API must snapshot a *set* of collections (id map + price tree + pools) at one point, since cross-structure consistency is the actual invariant; per-collection snapshots are a footgun.
- **Framed, versioned format** — per-collection section headers (type tag, schema version, capacity, element count, element size), trailing checksum (xxh3/crc32c), total-length framing so the archiver can stream it.
- **Full-state capture, not just live entries** — free-list heads, high-water marks, and any cached counters are state too; omitting them breaks determinism cluster #1.
- **Optional dirty tracking hooks** — per-region dirty bitmaps for future incremental snapshots, designed in as a feature flag even if v1 only does full snapshots (retrofitting is painful).

## 5. Recovery

- **Single-pass, near-sequential-bandwidth restore** — target: rehydrate GBs at NVMe read speed. Ideally support both copy-restore (read into fresh pools) and mmap-and-adopt (zero-copy load with a validation pass), the latter for fast follower bootstrap.
- **Validation on load** — checksum verify, schema-version check with explicit migration hooks, capacity compatibility check (restoring into a smaller-capacity build must fail loudly).
- **Bit-identical resumption** — the acceptance test: snapshot at position N, restore on a fresh process, replay entries N+1..M, and the resulting state (and every emitted output) is byte-identical to the replica that never restarted. This single property test subsumes half the spec.

## 6. Operational & verification

- **Occupancy/high-water-mark counters** exposed cheaply (ideally into an external counters file, Aeron-style) — capacity exhaustion in a fixed-size world is an ops event you must see coming.
- **Property-based determinism tests** — same op sequence on two instances ⇒ identical serialized bytes; arbitrary interleavings of insert/remove ⇒ identical iteration order. Run under Miri for the `unsafe` layout code.
- **Fuzz the deserializer** — it's the one component that parses untrusted-ish bytes (corrupt disk, truncated archive); it must reject garbage without UB.
- **Snapshot latency budget as a tested invariant** — e.g., "full serialize of max-capacity state ≤ X ms" as a benchmark gate, since the whole stop-the-world design rests on it.

The two requirements that most distinguish this from "arena + hashmap, done": **deterministic free-list order surviving restore** (nearly no off-the-shelf slab guarantees this) and **composite point-in-time snapshot across structures with the format/versioning machinery attached**. Those two are where I'd expect to write the most careful code — the rest is assembling known patterns (`slab`-like pools, `nohash` open addressing, `zerocopy` views) under one disciplined `repr(C)` roof.

Given UltimaDB already has MVCC, worth deciding explicitly per-structure: hot matching state gets these flat collections + stop-the-world serialize; anything naturally versioned or query-shaped can ride the engine's read-txn-as-snapshot instead. Both can coexist behind the same "snapshot at log position N" interface.
