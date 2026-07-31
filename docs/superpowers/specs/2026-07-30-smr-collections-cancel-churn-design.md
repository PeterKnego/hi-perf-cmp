# smr-collections — Cancel Op + Churn Workload — Design

**Date:** 2026-07-30
**Status:** Approved — ready for implementation planning

## Purpose

Every `smr-collections` cell measured so far runs a workload in which **nothing
is ever removed**. The order pool is bump-allocated (`hwm++`), there is no free
list, and there is no cancel op. The MVCC design
([2026-07-26](2026-07-26-smr-collections-mvcc-design.md)) named this explicitly
as out of scope: "no cancel/remove op is added … **version GC under churn is
deliberately unmeasured**".

That gap sits directly under the grid's headline finding. `ultima_batch_*`
lands at 2.3–2.7 µs/op against the flat store's 48–89 ns — a ~30–50× trade that
buys stall-free snapshots. But an insert/update-only stream is the friendliest
possible workload for an MVCC engine: a delete does not free anything, it writes
a new version and the old one lives until no reader can reach it. The measured
trade is therefore an upper bound on the engine's attractiveness, and the
cancel-heavy case — where it is most likely to lose — has never been run.

Real order flow is cancel-dominated. Roughly **1 % of orders end in a trade**
(≈100:1 order-to-trade), so ~99 % of orders leave the book by cancellation.
Order-to-trade ratios of that order are routine on equity and futures venues and
are monitored by regulators (MiFID II among others). A state store for a
matching engine spends its life recycling slots, not filling them.

This extension adds a cancel op and a **churn workload at that ratio** across
the same three store designs the grid already compares, so the engine-MVCC trade
can be quoted for the workload that actually occurs rather than only the one
that flatters it.

This is an extension of the existing `smr-collections` focus area — new
`smr-collections-<experiment>` artifacts, new rows in the ansible matrix, a new
subsection under the existing RESULTS.md heading. No new focus area.

Out of scope: disk checkpointing (the `filesystem-write` focus area's job),
matching/trade logic beyond a full fill, and partial-fill-then-cancel sequences
beyond what the op stream generates.

## Grid

Seven new experiments — 15 artifacts, one per experiment per language, per the
repo convention (`smr-collections-<experiment>`):

| experiment | languages | store |
|---|---|---|
| `churn` | rust, go, java | flat STW `Book` + free list |
| `mvcc_churn` | rust, go, java | chunked CoW `CowBook` + free list |
| `ultima_churn` | rust | ultima_db, one txn per command |
| `ultima_batch_churn` | rust | ultima_db, `SMRC_APPLY_BATCH` commands per txn |
| `live_stw_churn` | rust, go, java | churn while an STW snapshot serialises |
| `live_mvcc_churn` | rust, go, java | churn while a CoW root snapshot serialises |
| `live_ultima_churn` | rust | churn while a snapshot holds a `VersionPin` |

Existing cells are untouched apart from the `require_bump_capacity()` refactor
(see [Config](#config)) and the schema-v2 golden regeneration (see [Snapshot
format](#snapshot-format-schema-v2)).

The three `live_*_churn` cells mirror the existing `live_stw` / `live_mvcc` /
`live_ultima` trio exactly — same structure, same metrics, same
`SMRC_LIVE_ITERS` / `SMRC_SNAP_EVERY` cadence — with the churn stream in place
of the update-only one. That makes the snapshot-under-load comparison
same-workload across all three store designs, so `live_ultima_churn`'s
writer-stall number has a like-for-like baseline rather than being read against
a non-churn reference. It also puts the two most interesting stall questions on
one row: whether the STW store's stall grows once the serialiser must walk a
pool with holes in it, and whether the CoW store's first-touch chunk copies get
materially worse when cancels scatter writes across chunks instead of appending
to the newest one.

## The op stream

One shared derivation, byte-identical across all three languages and all seven
experiments — the same discipline as today's `next_insert` / `next_update`.

**Pre-build.** Insert `SMRC_STEADY` orders using the *existing* `next_insert`
stream, so the warm book is bit-identical to the one the current cells start
from. Each inserted order ID is appended to a dense `live[]` array.

**Timed loop.** For op index `i`:

- **`i` even → insert.** `next_insert(rng, i, levels, tick, price_min)`; push the
  new order ID onto `live[]`.
- **`i` odd → departure.**
  1. `v = rng.next() % live.len()` — uniform victim index.
  2. `is_fill = (rng.next() % 10_000) < SMRC_OTR_BPS`.
  3. `is_fill` → fill to completion; else → cancel.
  4. `live.swap_remove(v)`.

**Fill and cancel share one removal path.** A fill first sets `filled = qty` and
decrements the level's `qty_total` by the remaining quantity (the existing
`update` accounting, driven to completion); a cancel decrements by
`qty - filled` directly. Both then run the identical unlink / id-map-remove /
free-list-push sequence. The two ops differ only in that accounting step, so
`fill_*` and `cancel_*` measure the same structural work plus that difference —
which is the point: it isolates what removal costs.

Order IDs are **never reused**. `next_insert` derives `order_id = i + 1` from
the op index, and only even `i` are inserts, so IDs are sparse (1, 3, 5, …) but
unique for the life of the run. This matters because `order_id == 0` is the
freed-slot marker.

Alternating insert/departure holds the live set **exactly** constant — no drift
over a long run, and `hwm` stays near `steady` rather than climbing. At the
default 100 bps the mix is 50 % insert / 49.5 % cancel / 0.5 % fill, i.e. 1 % of
orders depart via a trade.

**Victim selection is uniform over the live set** (approach A of three
considered). FIFO victim selection — always cancel the oldest — is cheaper (no
RNG, no side array) but makes slot reuse perfectly cyclic, flattering allocator
and cache behaviour and understating exactly the cost under test. Recency-biased
selection is closer to real market-maker behaviour but needs a skew parameter we
would be inventing. Uniform draw over a dense array with O(1) swap-remove is
deterministic, cheap, and does not flatter the subject.

Op **generation** (the RNG draws and the `live[]` swap-remove) sits **outside**
the timed region: the driver produces the next op, the clock starts, the store
applies it, the clock stops. So the per-op numbers are store work only, and are
directly comparable with the existing `insert`/`update` cells, whose timed
region is likewise just the store call.

## Store changes

### `Book` — flat STW (rust, go, java)

Add `free_head: u32` (initialised `NIL`) and `cancel(order_id)`:

1. Look up the slot in the id-map.
2. Unlink from the level's intrusive FIFO via `prev`/`next`, fixing the level's
   `head`/`tail` when the victim is at either end.
3. `qty_total -= (qty - filled)`; `count -= 1`.
4. Remove from the id-map.
5. Set `order_id = 0` as the freed marker (order IDs start at 1), then push the
   slot onto the free list by threading it through the slot's existing `next`
   field: `pool[slot].next = free_head; free_head = slot`.
6. If the level is now empty **and** was the best on its side, rescan the ladder
   for the new best.

`insert` pops from `free_head` when non-`NIL`, falling back to `hwm++`.

The best-bid/ask **rescan is deliberately in the timed path**. Real books
maintain it; leaving it stale would be wrong and would hide the worst-case
cancel, which is precisely the tail this cell exists to expose. It is O(levels)
worst case (1024 at the default `SMRC_LEVELS`) and rare, so it should surface in
`cancel_p99`/`cancel_max` rather than the mean.

### `CowBook` — chunked copy-on-write (rust, go, java)

Same cancel semantics, plus **`free_head` joins `Root`**. A snapshot root that
omits it produces a restored replica whose allocation order diverges from the
original — the determinism requirement the whole design rests on. Cancel dirties
the order chunk and the level chunk, so it pays the same first-touch copy cost
inserts do; the ladder rescan may touch several level chunks.

### `UltimaBook` — ultima_db (rust)

The existing adapter derives the slot from the order ID — `let slot = (order_id
- 1) as u32`, with an assert that the table id `orders.insert()` returns equals
`order_id` (`ultima-common/src/lib.rs:167,186`) — and `encode_at` rests on the
same invariant: *"order ids are sequential from 1 in insertion order: id order
IS slot order 0..hwm"* (`lib.rs:465`). **Cancel invalidates all of it**: slots
are recycled, order IDs are sparse, and `delete()` removes the row entirely.

**ultima does not recycle slots.** A B-tree has no pool; "slot" exists in this
grid only because the snapshot format is the flat store's pool layout. Making
ultima emulate a free list — so its bytes line up with an array-shaped image —
would mean benchmarking the emulation, which is the distortion this repo exists
to avoid. So the adapter keeps `slot = order_id - 1` as a **monotone handle**,
never reused, and the changes are small:

- **Insert** switches from `orders.insert()` (auto-increment id, asserted equal
  to `order_id`) to `insert_with_id(order_id as u64, rec)`. The churn stream's
  order IDs are sparse (1, 3, 5, …), which the auto-increment id could not
  match; an explicit id makes that a non-issue and the assert goes away.
- **Cancel** = `orders.delete(order_id as u64)`, update the `levels` row, update
  `meta` (`best_bid`/`best_ask` after a rescan). That is the whole op — deletion
  is what generates the dead versions this spec exists to measure.
- **Batched cancel** uses `delete_batch`, and `ultima_batch_churn` opens its
  tables once per batch under `SMRC_MULTI_TABLE`, matching
  `ultima_batch_insert`.
- **`encode_at`** no longer assumes `0..hwm` is dense: it counts live rows and
  emits each with its own slot.

`ultima_db` supplies the primitives: `delete`, `delete_batch`, and
`insert_with_id` (`table.rs:375,546,600`).

**Consequence to report, not discover.** Because slots are never reused,
ultima's key space grows with *total ops* while the flat store's pool stays
bounded by the live set. Its tree therefore grows over a churn run where the
flat store's memory is flat. That is the honest behaviour of an append-keyed
store under churn and belongs in the results, but it must be stated up front so
`rss_growth_bytes` is not read as pure version-GC lag.

## Snapshot format (schema v2)

`rust/smr-collections/schema/book_snapshot.xml` gains one field:

```xml
<field name="freeHead" id="8" type="uint32"/>
```

in the `BookSnapshot` fixed block, and the schema `version` goes 1 → 2. That is
the whole format change.

**Flat and CoW keep the dense-pool image.** The `orders` group still serialises
every slot `0..hwm`, freed slots included and marked `order_id == 0`. Because a
freed slot stays in the pool, the free list already rides in the image threaded
through those slots' own `nextSlot` fields — so capturing `freeHead` is
sufficient to reproduce allocation order exactly. No group changes at all.

**ultima emits the same message, populated sparsely.** It has no freed slots to
carry, so its `orders` group holds live rows only and its `freeHead` is `NIL`.
Same schema, two population strategies — the format is capable of both, and the
difference is a genuine difference in what the two stores *are*.

Consequences, all one-time and taken in a single commit:

- `blockLength` changes; every image grows 4 bytes (2,751,256 → 2,751,260 at the
  default config). Existing cells' `snapshot_bytes` shifts by that much; the
  journal `compare` diff is expected and should be noted in the run entry.
- `testdata/golden_snapshot.bin` is regenerated (via the existing
  `SMRC_WRITE_GOLDEN` path) and re-verified by rust, go, and java.
- All three codecs are regenerated from the schema by the committed scripts:
  `rust/smr-collections/booksnap-sbe/regen.sh`,
  `go/internal/smrcoll/regen-booksnap.sh`,
  `java/smr-collections-common/regen-booksnap.sh` (each needs a JDK).
- Restore rejects a schema version other than 2.
- On a churn run, ultima's `snapshot_bytes` is **smaller** than the flat stores'
  — live rows only, versus a pool with holes. Expected, and reported as such.

### Cross-store equivalence: logical, not byte-wise

Raw byte-identity across *all* stores does not survive the introduction of
removal, and should not: with slots recycled in one store and monotone in
another, slot numbering is an internal representation detail rather than shared
state. Insisting the three agree on it would force a tree to pretend to be an
array.

So the churn cells split the check:

- **Flat vs CoW: byte-identical**, unchanged from today. Same layout, same
  allocation policy — the existing golden pattern applies verbatim.
- **ultima: logically equivalent**, checked against a canonical digest rather
  than the golden bytes. The canonical form is representation-free:
  - `best_bid`, `best_ask`, and the live-order count;
  - for each occupied level in `(side, tick)` ascending order: `qty_total`,
    `order_count`, and the level's FIFO as a sequence of **order IDs**
    (head → tail), not slot handles;
  - every live order as `(order_id, price, qty, filled, side)`, sorted by
    `order_id`.

  Serialised in that order and compared byte-for-byte across stores. It proves
  what the golden was always there to prove — that every variant computes the
  same book — without demanding they agree on where things sit in memory.

The rejected alternative was having ultima **emulate** the flat store's free
list (a `free` table plus `insert_with_id` by allocated slot) to preserve raw
byte-identity. It works, but the `ultima_cancel` number would then partly
measure the emulation of an allocator the engine has no other use for.

## Metrics

The ops are heterogeneous, so a single mixed mean would be near-meaningless.
Each cell emits **separate distributions per op type from one run**:

| metric | unit | cells |
|---|---|---|
| `cancel_p50` / `cancel_p99` / `cancel_mean` | ns | all |
| `insert_p50` / `insert_p99` / `insert_mean` | ns | all |
| `fill_p50` / `fill_p99` / `fill_mean` | ns | all |
| `rss_growth_bytes` | bytes | all |
| `writer_p99` / `writer_max` | ns | the three `live_*_churn` |
| `rss_peak_bytes` | bytes | the three `live_*_churn` |
| `snapshot_mean` / `snap_skipped` | ns / count | the three `live_*_churn` |

The `live_*_churn` cells emit the per-op distributions **as well as** the writer
metrics: `writer_p99`/`writer_max` are the aggregate stall over all op types
(the headline metric, matching the existing `live_*` cells), while the per-op
split shows which op absorbs the stall — a cancel that triggers a ladder rescan
while a serialiser is mid-flight is the plausible worst case, and the split is
what would surface it. `snapshot_mean` and `snap_skipped` follow the existing
`live_*` convention; `snap_skipped` matters especially for Go, whose ~5 ms
serialize already sat on a knife edge against the trigger window in the
non-churn `live_mvcc` run.

Reusing the existing per-op metric names means the new cancel number drops
straight into the RESULTS.md per-op tables beside insert and update.

`rss_growth_bytes` (RSS delta across the timed loop, via `/proc/self/statm` on
Linux) is the metric that answers the version-GC question: if reclamation keeps
up it is ~0; if it does not, it climbs. `rss_peak_bytes` on the `live_*_churn`
cells measures memory while a snapshot is in flight: for `live_ultima_churn`
that is reclamation blocked behind a pinned version — the specific failure mode
this spec exists to test — and for `live_mvcc_churn` it is the transient cost of
first-touch chunk copies, which the same metric captures for free.

`fill_*` samples are ~0.5 % of ops (≈500 samples at the default 100 K iters), so
its p99 is thin. Reported as-is with that caveat rather than dropped: the cell
still demonstrates the op runs.

## Config

One new env knob, parsed in each language's shared bench library:

| var | default | meaning |
|---|---|---|
| `SMRC_OTR_BPS` | `100` | order-to-trade ratio in basis points (100 = 1 %); validated `0..=10000` |

All other `SMRC_*` vars are reused unchanged.

**Validation refactor.** `SmrConfig::from_env` currently enforces
`warmup + iters <= cap`, which is a *bump-allocator* constraint: it exists
because today's pool never frees. Churn recycles slots, so a long churn run
(`SMRC_ITERS=1000000`) would be rejected for no reason — and long runs are
exactly what is wanted when measuring whether reclamation keeps up.

That single check moves out of `from_env` into an explicit
`require_bump_capacity()` (Rust; equivalent method in Go and Java), called by the
cells that actually bump-allocate: `insert`, `mvcc_insert`, `ultima_insert`,
`ultima_batch_insert`, and the snapshot/live cells that pre-build by insert
only. `steady <= cap` stays universal in `from_env`. Churn cells do not call it.

## Error handling

- **Cancel of an unknown or already-freed ID** cannot occur by construction
  (victims are drawn from `live[]`). The store fails fast rather than degrading,
  matching the existing `idmap[&order_id]` panic-on-missing behaviour.
- **Insert with an empty free list and `hwm == cap`** fails loudly with a
  capacity message. Fixed capacity never grows — no rehash, no realloc.
- **Restore** rejects a wrong schema version, a capacity mismatch (an image
  restored into a smaller-capacity build must fail loudly, not truncate), or a
  bad crc32c trailer.

## Testing

In rough order of what each is worth:

1. **Bit-identical resumption.** Run N churn ops → snapshot → restore into a
   fresh store → replay ops N+1..M on both the restored store and the
   never-restarted one → re-snapshot → byte-identical. This proves free-list
   order survives a snapshot, and it is the entire justification for the
   `freeHead` field.
2. **Cross-store and cross-language equivalence**, split by what each store
   actually is:
   - `Book` and `CowBook` run the identical churn stream and produce
     **identical bytes**, matching one new golden file verified by rust, go,
     and java — the existing golden pattern, unchanged.
   - `UltimaBook` matches the same run's **canonical digest** (defined above),
     not its bytes. A digest mismatch is a real state divergence; a byte
     mismatch against the flat golden is expected and is not a failure.
3. **LOB invariants after arbitrary insert/cancel/fill sequences.** Per level:
   `qty_total` equals the summed remaining qty of live orders at that level;
   `count` equals their number; walking `head → next → tail` visits exactly
   `count` orders and terminates at `NIL`; `best_bid`/`best_ask` equal the
   highest/lowest occupied tick.
4. **Free-list determinism.** Two instances on the same op stream agree on
   `free_head` and on which slot the next insert receives.
5. **Capacity exhaustion** returns an error rather than corrupting state.
6. **Existing suites stay green:** `cargo clippy --all-targets`,
   `cargo fmt --check`, `cargo test`, `go vet ./...`, `go test ./...`,
   `./gradlew build`.

## Infra & docs changes

- Seven rows in `bench-infra/ansible/group_vars/all.yml`'s `experiments` matrix
  (one per experiment, not per artifact — the matrix fans out over languages),
  `kind: local`, with `languages: [rust]` on `ultima_churn`,
  `ultima_batch_churn`, and `live_ultima_churn`.
- `smrc_otr_bps: 100` in the same file's smr-collections params block.
- `CLAUDE.md`: add the new artifact names to the Build & run list and note the
  churn workload in the smr-collections status paragraph.
- `docs/RESULTS.md`: a new subsection under the existing `## smr-collections`
  heading — written **only after a real AWS `bench-infra` run**. Local/loopback
  smoke runs are fitness checks and are never journaled.

## Open items deliberately deferred

- Cancel-heavy behaviour under a *growing* live set (churn plus net inflow),
  which would stress capacity limits rather than reclamation.
- Recency-biased victim selection, which would model market-maker quote-pulling
  more closely at the cost of an invented skew parameter.
