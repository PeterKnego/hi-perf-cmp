# ultima_db batched-apply cells + bulk_load restore — design

Date: 2026-07-27
Status: approved (batch default 64 and post-land fleet run confirmed by Peter)

## Motivation

Two gaps left by the VersionPin re-measure (run 20260727T134311Z, PR #1):

1. **Restore is measured through the slowest ultima_db path.** `restore_ultima`
   seeds a store via `UltimaBook::new` (a txn inserting 2·levels empty rows)
   and then applies ~capacity per-record `insert`/`update` calls in a second
   txn — 8.6 ms for the 2.75 MB image. ultima_db's intended restore path is
   `Store::bulk_load_batch` (atomic multi-table `BTree::from_sorted`, O(N)).
2. **The grid only measures the pessimal txn amortization.** One command per
   write txn is the worst case; a real SMR applier commits a consensus batch
   per txn. Our engine-side microbench (`apply_sw_batch_throughput` ≈ 233 k
   ops/s at rev b48295e) suggests batching closes most of the remaining
   ~75–175× gap vs the flat store. The grid should bracket the trade at both
   ends.

## Design

### A. `restore_ultima` via `bulk_load_batch`

- New private `UltimaBook::empty(cfg)`: constructs the store (same
  `StoreConfig`) and the config fields with **no seeding txn**; version 0.
  `UltimaBook::new` keeps its current seeded behavior for the apply cells.
- `restore_ultima` (CRC check unchanged) decodes into three vecs:
  - `levels`: length 2·levels, ids `1..=2*levels`, initialized to the empty
    `LevelRec` (head/tail = NIL) and overwritten from the SBE levels group;
  - `orders`: `(slot as u64 + 1, OrderRec)` in group order, with the existing
    "orders group not in slot order" check enforced while building (ids must
    be strictly ascending, exactly `1..=hwm`);
  - `meta`: `[(1, MetaRec)]`.
- One atomic install:
  `store.bulk_load_batch()` → `add("levels"/"orders"/"meta",
  BulkLoadInput::Replace(BulkSource::sorted_vec(...)), AddOptions)` →
  `commit(BulkLoadOptions { create_if_missing: true, checkpoint_after: false })`.
  Set `ub.version` to the returned version. An empty orders group (hwm 0)
  still adds an empty vec so the table exists for later `open_table`.
- Correctness pinned by the existing golden round-trip test
  (`restore_round_trips_and_rejects_corruption`) — byte-identity catches any
  id / next_id / ordering drift introduced by the new path.

### B. Batched-apply cells (`ultima_batch_insert`, `ultima_batch_update`)

Rust-only, mirroring the existing `ultima_insert` / `ultima_update` pair.

- **Refactor for comparability:** extract the current per-command bodies into
  `fn apply_insert(&self, wtx: &mut WriteTx, ...)` / `apply_update(...)`
  helpers (same table opens, same per-command work). `insert()` / `update()`
  become txn-wrap-one-command around the helper — behavior identical, so the
  batched and unbatched cells differ **only** in txn amortization.
- New `insert_batch_txn` / `update_batch_txn`: one
  `begin_write(Some(version))`, loop the helper over the batch, one commit.
  Deliberately NOT `Table::insert_batch` — that benchmarks ultima's bulk API,
  not txn amortization.
- **Config:** new `SmrConfig.apply_batch` (env `SMRC_APPLY_BATCH`, default
  **64**). `SMRC_ITERS` keeps meaning commands; a cell runs
  `iters / apply_batch` timed batches (remainder commands dropped), warmup
  likewise `warmup / apply_batch` untimed batches. Existing validation
  (`warmup + iters <= cap`) is unchanged and stays sufficient. New
  validation: `apply_batch >= 1` and `apply_batch <= iters`.
- **Emission** (per cell): `emit_latency(exp, "batch", &batch_ns)` →
  `batch_mean/p50/p99` over per-batch wall times; `emit_float(exp,
  "per_op_mean", batch_total_ns / ops_applied, "ns", ops_applied)`;
  `emit_int(exp, "batch_size", apply_batch)`. Per-op cost is compared against
  the unbatched cells via `per_op_mean` vs `insert_mean`/`update_mean`.
- **Golden equivalence test:** applying the steady insert workload via
  `insert_batch_txn` (batch 64, remainder as a final short batch — the test
  applies ALL commands, unlike the timed cell) encodes byte-identical to the
  per-op path; same for a mixed insert+update sequence vs the STW book.
- **Wiring:** two new workspace member packages
  (`smr-collections/ultima_batch_insert`, `.../ultima_batch_update`) with
  mains mirroring `ultima_insert`/`ultima_update`; two rust-only rows in
  `bench-infra/ansible/group_vars/all.yml`; `smrc_apply_batch: 64` param
  alongside the other `smrc_*` vars and exported in the run role's env (match
  how `SMRC_CHUNK` flows); artifact-name list in root `CLAUDE.md` gains
  `ultima_batch_insert,ultima_batch_update`.

## Error handling

- Restore: malformed group order / hwm mismatch → `Err(String)` as today;
  bulk_load errors map through `.map_err(|e| format!(...))` to the existing
  `Result<UltimaBook, String>` contract.
- Config: `SMRC_APPLY_BATCH=0` or `> SMRC_ITERS` → config error at startup
  (same style as existing SmrConfig validations).

## Measurement plan (authorized)

After local verification lands: one scoped fleet run covering the six
ultima cells (`ultima_insert`, `ultima_update`, `ultima_snapshot`,
`live_ultima`, `ultima_batch_insert`, `ultima_batch_update`), journaled and
folded into RESULTS.md. Standard teardown discipline.

## Out of scope

- Batched cells for the flat/CoW stores (no txn machinery to amortize — their
  per-op cost IS the batched cost).
- Go/Java ultima cells; `Table::insert_batch`-based cells.
- Changing `SMRC_ITERS` semantics or any existing cell's emission.
