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

Nine new experiments, one artifact per experiment per the repo convention
(`smr-collections-<experiment>`):

| experiment | languages | store |
|---|---|---|
| `churn` | rust, go, java | flat STW `Book` + free list |
| `mvcc_churn` | rust, go, java | chunked CoW `CowBook` + free list |
| `ultima_churn` | rust | ultima_db, one txn per command |
| `ultima_batch_churn` | rust | ultima_db, `SMRC_APPLY_BATCH` commands per txn |
| `live_ultima_churn` | rust | churn while a snapshot holds a `VersionPin` |

Existing cells are untouched apart from the `require_bump_capacity()` refactor
(see [Config](#config)) and the schema-v2 golden regeneration (see [Snapshot
format](#snapshot-format-schema-v2)).

`live_ultima_churn` deliberately has **no churn-mode counterpart for the flat
and CoW stores**. Its writer-stall number will have only the existing
non-churn `live_stw`/`live_mvcc` cells as a reference — a stated limitation, not
an oversight. Adding `live_stw_churn` / `live_mvcc_churn` is two more
experiments × three languages if the comparator turns out to be needed.

## The op stream

One shared derivation, byte-identical across all three languages and all four
store variants — the same discipline as today's `next_insert` / `next_update`.

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

The `live[]` bookkeeping runs inside the timed loop and costs a few ns per op.
It is charged **identically to every cell**, so cross-store and cross-language
comparisons remain fair; absolute per-op numbers carry it and should be read as
"op + harness bookkeeping", consistent with how the existing cells carry their
own workload derivation.

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

Cancel = delete the `orders` row, update the `levels` row, update `meta` (free
head), within the existing one-txn-per-command / batched-txn structure.
`ultima_batch_churn` opens its tables once per batch via `open_tables3` under
`SMRC_MULTI_TABLE`, matching `ultima_batch_insert`. This is the path that
generates dead versions, and the reason the focus area needs this spec.

## Snapshot format (schema v2)

`rust/smr-collections/schema/book_snapshot.xml` gains one field:

```xml
<field name="freeHead" id="8" type="uint32"/>
```

in the `BookSnapshot` fixed block, and the schema `version` goes 1 → 2.

Freed slots thread the free list through their existing `nextSlot` field, so the
list itself already rides in the orders group — **no group field changes**, and
the orders group still serialises slots `0..hwm` verbatim. Restore reads
`freeHead` directly and skips `orderId == 0` slots when rebuilding the id-map.

Consequences, all one-time and taken in a single commit:

- `blockLength` changes; every image grows 4 bytes (2,751,256 → 2,751,260 at the
  default config). Existing cells' `snapshot_bytes` shifts by that much; the
  journal `compare` diff is expected and should be noted in the run entry.
- `testdata/golden_snapshot.bin` is regenerated (via the existing
  `SMRC_WRITE_GOLDEN` path) and re-verified by rust, go, and java.
- Restore rejects a schema version other than 2.

The alternatives considered were a second schema used only by churn cells (two
formats and two goldens to keep in sync across three languages) and
reconstructing the free list by ascending-slot scan at restore time (which
forces the writer to allocate in ascending order — a heap or bitmap scan, slower
on the hot path, and it would change what the cancel cell measures). Full-state
capture in one schema is what the design requires: free-list heads are state,
and omitting them breaks bit-identical resumption.

## Metrics

The ops are heterogeneous, so a single mixed mean would be near-meaningless.
Each cell emits **separate distributions per op type from one run**:

| metric | unit | cells |
|---|---|---|
| `cancel_p50` / `cancel_p99` / `cancel_mean` | ns | all nine |
| `insert_p50` / `insert_p99` / `insert_mean` | ns | all nine |
| `fill_p50` / `fill_p99` / `fill_mean` | ns | all nine |
| `rss_growth_bytes` | bytes | all nine |
| `writer_p99` / `writer_max` | ns | `live_ultima_churn` only |
| `rss_peak_bytes` | bytes | `live_ultima_churn` only |
| `snapshot_mean` / `snap_skipped` | ns / count | `live_ultima_churn` only |

`live_ultima_churn` emits the per-op distributions **as well as** the writer
metrics: `writer_p99`/`writer_max` are the aggregate stall over all op types
(the headline metric, matching the existing `live_*` cells), while the per-op
split shows which op absorbs the stall. `snapshot_mean` and `snap_skipped`
follow the existing `live_*` convention.

Reusing the existing per-op metric names means the new cancel number drops
straight into the RESULTS.md per-op tables beside insert and update.

`rss_growth_bytes` (RSS delta across the timed loop, via `/proc/self/statm` on
Linux) is the metric that answers the version-GC question: if reclamation keeps
up it is ~0; if it does not, it climbs. For `live_ultima_churn`, `rss_peak_bytes`
measures memory while reclamation is blocked behind a pinned version — the
specific failure mode this spec exists to test.

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
   order survives a snapshot, and it is the entire justification for putting
   `freeHead` in the schema.
2. **Cross-store and cross-language golden.** `Book`, `CowBook`, and
   `UltimaBook` run the identical churn stream and produce identical bytes,
   matching one new golden file verified by rust, go, and java — extending the
   existing golden pattern.
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

- Nine rows in `bench-infra/ansible/group_vars/all.yml`'s `experiments` matrix,
  `kind: local`, with `languages: [rust]` on `ultima_churn`,
  `ultima_batch_churn`, and `live_ultima_churn`.
- `smrc_otr_bps: 100` in the same file's smr-collections params block.
- `CLAUDE.md`: add the new artifact names to the Build & run list and note the
  churn workload in the smr-collections status paragraph.
- `docs/RESULTS.md`: a new subsection under the existing `## smr-collections`
  heading — written **only after a real AWS `bench-infra` run**. Local/loopback
  smoke runs are fitness checks and are never journaled.

## Open items deliberately deferred

- `live_stw_churn` / `live_mvcc_churn` — the flat/CoW comparators for
  `live_ultima_churn` (see [Grid](#grid)).
- Cancel-heavy behaviour under a *growing* live set (churn plus net inflow),
  which would stress capacity limits rather than reclamation.
- Recency-biased victim selection, which would model market-maker quote-pulling
  more closely at the cost of an invented skew parameter.
