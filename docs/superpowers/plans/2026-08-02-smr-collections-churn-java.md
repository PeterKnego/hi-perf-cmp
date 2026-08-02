# smr-collections Churn — Java Parity + Infra Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring Java to parity with the merged Rust and Go cancel/churn work, then land the `bench-infra` rows and `CLAUDE.md` updates that make the whole grid runnable on the fleet.

**Architecture:** Mirrors the two merged implementations op-for-op, because all three must produce byte-identical snapshot images. Java's advantages and constraints differ: Agrona's `Long2ObjectHashMap` already does compaction-on-remove (so no equivalent of Go's `idMap.del` work), but `SmrConfig` is a **record** whose component list is a breaking change, cells are **Gradle subprojects** rather than directories, and the codebase's allocation-free idiom (reusable out-params) applies to the churn driver.

**Tech Stack:** Java 21 (toolchain), Gradle 8.10.2 via the checked-in wrapper, Agrona, real-logic `sbe-tool` 1.38.1 (committed generated codec).

**Spec:** [`docs/superpowers/specs/2026-07-30-smr-collections-cancel-churn-design.md`](../specs/2026-07-30-smr-collections-cancel-churn-design.md)

## Scope

**Plan 3 of 3, and the last one before the grid can be run.** Plans 1 (Rust) and 2 (Go) are merged: schema v2 landed, both golden files exist, and Java's codec was already regenerated for v2 with both encoders writing `freeHead = NIL` as a placeholder. This plan replaces those placeholders with real values, adds the ops and the driver, ships four cells, and then — in the final task — adds the ansible matrix rows and `smrc_otr_bps` that let any churn cell run on the fleet at all.

Until Task 8 lands, **no churn cell in any language runs on the fleet**, so there is still nothing to journal.

Not in this plan: any ultima cell (Rust-only — Java has no MVCC-engine adapter), the canonical digest (only needed to compare against ultima), and any change under `rust/` or `go/`.

## Global Constraints

- Java **21** toolchain; Gradle **8.10.2** via the checked-in wrapper. Always invoke `./gradlew`, never a system `gradle`. `cd java && ./gradlew build` must pass before any commit.
- **stdout is result-contract JSON lines only.** Logs, progress and diagnostics go to `System.err` — a stray `System.out.println` breaks the downstream journal tooling silently.
- Result lines come **only** from `net.knego.hiperf.common.SmrCollections` helpers, never hand-rolled JSON. Every line carries `focus_area: "smr-collections"` and the cell's `experiment`, which must exactly match the Gradle subproject-name suffix.
- **Determinism is the top requirement.** Java, Go and Rust must produce byte-identical images from the same op stream, on any host, and across snapshot/restore. Never let `HashMap` iteration order reach output.
- Order IDs start at **1**; `orderId == 0` is the freed-slot marker. `Book.NIL == -1` (as unsigned: `0xFFFFFFFF`).
- Fixed capacity **never grows** — no rehash, no realloc. Exhaustion fails loudly.
- `SMRC_OTR_BPS` is the order-to-trade ratio in basis points, default **100** (= 1 %), valid range **0..=10000**.
- The two golden files under `rust/smr-collections/testdata/` are **read-only** here. A mismatch is a real finding — report it, never regenerate.
- Churn cells recycle slots and must NOT call `requireBumpCapacity()`.
- Op **generation** sits outside the timed region: generate the op, start the clock, apply, stop. (This differs from the older `insert`/`update` cells, which time their own generation — a known, documented asymmetry.)
- Follow the codebase's **allocation-free idiom**: `Workload` uses reusable out-param objects (`Workload.Insert`, `Workload.Update`) rather than returning new objects per call. The churn driver does the same.
- Do NOT run any AWS benchmark or `terraform`, and do not run `make bench`. Editing `bench-infra` config files is in scope; executing anything under `bench-infra/` is not.
- Do not touch `rust/` or `go/`.

## File Structure

**Modified:**
- `java/common/src/main/java/net/knego/hiperf/common/SmrConfig.java` — `otrBps` record component, `requireBumpCapacity()`, relocated capacity check
- `java/common/src/main/java/net/knego/hiperf/common/Env.java` — `readNonNegativeInt`
- `java/common/src/main/java/net/knego/hiperf/common/SmrCollections.java` — `rssBytes()`
- `java/smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/Book.java` — free list, `cancel`, `fill`, `repairBest`
- `.../Snapshotter.java` — real `freeHead`; version and capacity validation on restore
- `.../CowBook.java`, `.../CowRoot.java`, `.../CowSnapshotter.java` — the same for the CoW store
- `java/smr-collections-{insert,mvcc_insert}/.../Main.java` — call `requireBumpCapacity()`
- `java/settings.gradle.kts` — four new subprojects
- `bench-infra/ansible/group_vars/all.yml` — experiment rows + `smrc_otr_bps`
- `CLAUDE.md` — artifact names and status paragraph

**Created:**
- `.../Churn.java` — `Churn.Store`, `Churn.Op`, the driver, `runChurn`, `emitChurn`
- `java/smr-collections-common/src/test/java/.../ChurnTest.java`
- `java/smr-collections-{churn,mvcc_churn,live_stw_churn,live_mvcc_churn}/` — four subprojects, each a `build.gradle.kts` plus a `Main.java`

---

### Task 1: Config — `SMRC_OTR_BPS`, capacity-check refactor, RSS helper

**Files:**
- Modify: `java/common/src/main/java/net/knego/hiperf/common/SmrConfig.java`
- Modify: `java/common/src/main/java/net/knego/hiperf/common/Env.java`
- Modify: `java/common/src/main/java/net/knego/hiperf/common/SmrCollections.java`
- Modify: `java/smr-collections-insert/.../Main.java`, `java/smr-collections-mvcc_insert/.../Main.java`
- Test: `java/common/src/test/java/net/knego/hiperf/common/` (add a `SmrConfigTest.java` if none exists; otherwise extend it)

**Interfaces:**
- Produces: `SmrConfig.otrBps()` (record accessor); `void SmrConfig.requireBumpCapacity()`; `static long SmrCollections.rssBytes()`; `static int Env.readNonNegativeInt(String, int)`

**`SmrConfig` is a record**, so adding a component changes the canonical constructor. Every `new SmrConfig(...)` call site must gain the new argument or the build breaks. Find them all first:

```sh
cd java && grep -rn "new SmrConfig(" --include=*.java . | grep -v /build/
```

- [ ] **Step 1: Write the failing tests**

Create or extend a test in `java/common/src/test/java/net/knego/hiperf/common/SmrConfigTest.java`. Environment variables cannot be set from inside a JVM test, so these tests exercise the *validation* surface directly rather than `fromEnv()`:

```java
package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class SmrConfigTest {

    private static SmrConfig cfg(int warmup, int iters, int cap) {
        return new SmrConfig(cap, 64, 1L, 0L, 100, warmup, iters, 256, 200_000, 20_000, 100);
    }

    @Test
    void defaultsCarryOnePercentOtr() {
        assertEquals(100, cfg(10, 10, 4096).otrBps(), "default OTR is 1% = 100 bps");
    }

    @Test
    void churnSizedRunFailsBumpCapacityButIsOtherwiseLegal() {
        // warmup+iters > cap is legal for a slot-recycling churn cell and
        // illegal for a bump-allocating insert cell.
        SmrConfig c = cfg(1000, 10_000, 1024);
        assertThrows(IllegalArgumentException.class, c::requireBumpCapacity);
    }

    @Test
    void bumpSizedRunPassesBumpCapacity() {
        assertDoesNotThrow(cfg(10, 100, 4096)::requireBumpCapacity);
    }

    @Test
    void rssBytesIsReadable() {
        org.junit.jupiter.api.Assertions.assertTrue(
                SmrCollections.rssBytes() > 0, "RSS must be readable from /proc/self/statm");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd java && ./gradlew :common:test --tests '*SmrConfigTest*'
```

Expected: compilation failure — the 11-argument constructor does not exist, `requireBumpCapacity` and `rssBytes` are undefined.

- [ ] **Step 3: Implement the config changes**

In `SmrConfig.java`, add `int otrBps` as the final record component:

```java
public record SmrConfig(
        int cap, int levels, long tick, long priceMin, int steady, int warmup, int iters,
        int chunk, int liveIters, int snapEvery, int otrBps) {
```

In `fromEnv()`, parse and validate it next to the other knobs:

```java
        int otrBps = Env.readNonNegativeInt("SMRC_OTR_BPS", 100);
        if (otrBps > 10000) {
            throw new IllegalArgumentException("SMRC_OTR_BPS must be in 0..=10000, got " + otrBps);
        }
```

**Delete** this block from `fromEnv()` — it is a bump-allocator constraint, not a universal one:

```java
        if ((long) warmup + iters > cap) {
            throw new IllegalArgumentException("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP");
        }
```

Add `otrBps` to the `return new SmrConfig(...)` argument list, and add the method to the record body:

```java
    /**
     * Cells that bump-allocate (no free list) need a pool slot for every op they will ever run.
     * Churn cells recycle slots and must NOT call this.
     */
    public void requireBumpCapacity() {
        if ((long) warmup + iters > cap) {
            throw new IllegalArgumentException("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP");
        }
    }
```

In `Env.java`, alongside `readPositiveInt`:

```java
    /** readPositiveInt but admits zero, for knobs where zero is a meaningful setting. */
    static int readNonNegativeInt(String name, int def) {
        String raw = System.getenv(name);
        if (raw == null || raw.isEmpty()) {
            return def;
        }
        int value;
        try {
            value = Integer.parseInt(raw.trim());
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException(name + " must be a non-negative integer, got: " + raw);
        }
        if (value < 0) {
            throw new IllegalArgumentException(name + " must be a non-negative integer, got: " + raw);
        }
        return value;
    }
```

If `Env`'s members are package-private and `SmrConfig` is in the same package, no visibility change is needed — match whatever `readPositiveInt` already is.

In `SmrCollections.java`:

```java
    /**
     * Resident set size in bytes, from Linux /proc/self/statm field 2 (resident pages), or 0
     * where unreadable. The bench hosts are x86-64 Linux with 4 KiB pages, which is the only
     * case that must be right. Allocates, so callers must keep it out of timed regions.
     */
    public static long rssBytes() {
        try {
            String s = java.nio.file.Files.readString(java.nio.file.Path.of("/proc/self/statm"));
            String[] f = s.trim().split("\\s+");
            if (f.length < 2) {
                return 0L;
            }
            return Long.parseLong(f[1]) * 4096L;
        } catch (Exception e) {
            return 0L;
        }
    }
```

- [ ] **Step 4: Fix every `new SmrConfig(...)` call site**

Use the grep from the task header. Each site gains a trailing `, 100` (the 1 % default) unless the test's intent calls for something else. The build will not compile until all are updated.

- [ ] **Step 5: Guard the bump-allocating cells**

In `java/smr-collections-insert/.../Main.java` and `java/smr-collections-mvcc_insert/.../Main.java`, immediately after `SmrConfig cfg = SmrConfig.fromEnv();`, insert:

```java
            cfg.requireBumpCapacity();
```

It throws `IllegalArgumentException`, which those `Main`s already catch and report to `System.err` before exiting non-zero. Leave `update`, `snapshot`, `mvcc_update`, `mvcc_snapshot`, `live_stw` and `live_mvcc` alone — they pre-build `steady` orders then only mutate, so the universal `steady <= cap` check already covers them. Java has no ultima cells.

- [ ] **Step 6: Run the build**

```sh
cd java && ./gradlew build
```

Expected: BUILD SUCCESSFUL. If a pre-existing test relied on `fromEnv()` rejecting `warmup + iters > cap`, move its expectation to `requireBumpCapacity()` rather than restoring the old check — and say so in your report.

- [ ] **Step 7: Commit**

```sh
git add java/common java/smr-collections-insert java/smr-collections-mvcc_insert
git commit -m "feat(smrcoll,java): SMRC_OTR_BPS + requireBumpCapacity() + rssBytes()

Moves warmup+iters<=cap out of fromEnv into an explicit check the
bump-allocating cells call, so slot-recycling churn cells can run longer
than SMRC_CAP. Mirrors the merged Rust and Go sides."
```

---

### Task 2: `Book` — free list, `cancel`, `fill`, best-price rescan

**Files:**
- Modify: `java/smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/Book.java`
- Test: `java/smr-collections-common/src/test/java/net/knego/hiperf/smrcollections/BookTest.java`

**Interfaces:**
- Consumes: `SmrConfig` from Task 1
- Produces: `Book.freeHead` (public int); `void Book.cancel(long orderId)`; `void Book.fill(long orderId)`

Semantics must match the merged Rust and Go `Book`s exactly — all three produce byte-identical images, so any divergence in operation order, in what the withdrawn quantity is computed from, or in the link fixups is a real defect.

**Java gets one thing free:** `ids` is an Agrona `Long2ObjectHashMap<Order>`, whose `remove` already does compaction-on-remove. There is no equivalent of the backward-shift work Go needed.

**Java's pool holds objects, not values.** `pool[slot]` is a reusable `Order` instance carrying its own `slot` field, and `ids` maps order ID → that instance. So `cancel` can read everything it needs from the instance `ids.remove()` returns, without a separate slot lookup.

- [ ] **Step 1: Write the failing tests**

Append to `BookTest.java` (follow the existing file's `SmrConfig` literal style, adding the trailing `otrBps` argument):

```java
    private static SmrConfig churnCfg() {
        return new SmrConfig(1024, 16, 1L, 0L, 100, 0, 0, 256, 200_000, 20_000, 100);
    }

    @Test
    void cancelUnlinksMiddleOfLevelFifo() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.insert(2, 5, 7, (byte) 0);
        b.insert(3, 5, 3, (byte) 0);
        b.cancel(2);
        assertEquals(13, b.levelQty((byte) 0, 5), "middle order's qty leaves the level");
        assertEquals(2, b.bids[5].count);
        assertEquals(0, b.bids[5].head, "head unchanged");
        assertEquals(2, b.bids[5].tail, "tail unchanged");
        assertEquals(2, b.pool[0].next, "head now links past the cancelled slot");
        assertEquals(0, b.pool[2].prev);
    }

    @Test
    void cancelHeadAndTailFixLevelEnds() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.insert(2, 5, 7, (byte) 0);
        b.cancel(1); // head
        assertEquals(1, b.bids[5].head, "head advances to the survivor");
        assertEquals(Book.NIL, b.pool[1].prev);
        b.cancel(2); // tail; level now empty
        assertEquals(Book.NIL, b.bids[5].head);
        assertEquals(Book.NIL, b.bids[5].tail);
        assertEquals(0, b.bids[5].count);
        assertEquals(0, b.levelQty((byte) 0, 5));
    }

    @Test
    void cancelEmptyingBestLevelRescans() {
        Book b = new Book(churnCfg());
        b.insert(1, 3, 10, (byte) 0);
        b.insert(2, 9, 10, (byte) 0); // best bid = 9
        b.insert(3, 4, 10, (byte) 1);
        b.insert(4, 2, 10, (byte) 1); // best ask = 2
        b.cancel(2);
        assertEquals(3, b.bestBid(), "best bid falls back to the next occupied below");
        b.cancel(4);
        assertEquals(4, b.bestAsk(), "best ask rises to the next occupied above");
        b.cancel(1);
        assertEquals(-1, b.bestBid(), "no bids left");
    }

    @Test
    void cancelledSlotsAreReusedLifo() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0); // slot 0
        b.insert(2, 5, 10, (byte) 0); // slot 1
        b.insert(3, 5, 10, (byte) 0); // slot 2
        b.cancel(1); // free: 0
        b.cancel(3); // free: 2 -> 0
        assertEquals(2, b.freeHead);
        b.insert(4, 5, 10, (byte) 0);
        assertEquals(2, b.getSlot(4), "LIFO: most recently freed slot first");
        b.insert(5, 5, 10, (byte) 0);
        assertEquals(0, b.getSlot(5));
        b.insert(6, 5, 10, (byte) 0);
        assertEquals(3, b.getSlot(6), "free list empty -> bump hwm");
        assertEquals(4, b.hwm());
    }

    @Test
    void freedSlotIsMarkedWithZeroOrderId() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.cancel(1);
        assertEquals(0, b.pool[0].orderId, "freed marker for the snapshot walk");
    }

    @Test
    void fillCompletesThenFreesTheSlot() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.update(1, 4); // partial: remaining 6
        assertEquals(6, b.levelQty((byte) 0, 5));
        b.fill(1);
        assertEquals(0, b.levelQty((byte) 0, 5), "remaining 6 leaves the level");
        assertEquals(0, b.bids[5].count);
        assertEquals(0, b.freeHead, "slot recycled like a cancel");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd java && ./gradlew :smr-collections-common:test --tests '*BookTest*'
```

Expected: compilation failure — `cancel`, `fill` and `freeHead` do not exist.

- [ ] **Step 3: Implement**

Add the field and initialise it:

```java
    /**
     * Head of the intrusive LIFO free list (NIL when empty). Freed slots chain through their own
     * `next` field. This is state a snapshot must capture — restore reproduces allocation order
     * from it.
     */
    public int freeHead = NIL;
```

Replace `insert`'s `int slot = hwm++;` with `int slot = allocSlot();`, and add:

```java
    private int allocSlot() {
        if (freeHead != NIL) {
            int slot = freeHead;
            freeHead = pool[slot].next;
            return slot;
        }
        if (hwm == pool.length) {
            throw new IllegalStateException("order pool exhausted: SMRC_CAP=" + pool.length + " reached");
        }
        return hwm++;
    }

    private void freeSlot(int slot) {
        Order o = pool[slot];
        o.orderId = 0; // freed marker: the snapshot walk skips these
        o.next = freeHead;
        o.prev = NIL;
        freeHead = slot;
    }

    /** Unlink slot from its level's intrusive FIFO and debit rem from the level's remaining qty. */
    private void unlink(int slot, byte side, int t, long rem) {
        Order o = pool[slot];
        int prev = o.prev;
        int next = o.next;
        if (prev != NIL) {
            pool[prev].next = next;
        }
        if (next != NIL) {
            pool[next].prev = prev;
        }
        Level lvl = lane(side)[t];
        if (lvl.head == slot) {
            lvl.head = next;
        }
        if (lvl.tail == slot) {
            lvl.tail = prev;
        }
        lvl.qtyTotal -= rem;
        lvl.count--;
    }

    /**
     * Restore the cached best for side after a removal emptied level t. O(levels) worst case and
     * deliberately on the timed path — real books maintain this, and hiding it would hide the
     * worst-case cancel.
     */
    private void repairBest(byte side, int t) {
        if (side == 0) {
            if (bestBid != t || bids[t].head != NIL) {
                return;
            }
            int nb = -1;
            for (int i = t; i >= 0; i--) {
                if (bids[i].head != NIL) {
                    nb = i;
                    break;
                }
            }
            bestBid = nb;
            return;
        }
        if (bestAsk != t || asks[t].head != NIL) {
            return;
        }
        int na = -1;
        for (int i = t; i < nLevels; i++) {
            if (asks[i].head != NIL) {
                na = i;
                break;
            }
        }
        bestAsk = na;
    }

    /** Remove a resting order; its remaining quantity leaves the level. */
    public void cancel(long orderId) {
        Order o = ids.remove(orderId);
        long rem = o.qty - o.filled;
        byte side = o.side;
        int t = tickOf(o.price);
        int slot = o.slot;
        unlink(slot, side, t, rem);
        freeSlot(slot);
        repairBest(side, t);
    }

    /**
     * Fill an order to completion, then remove it. Same structural work as cancel; the difference
     * is that the departing quantity is booked as filled rather than withdrawn.
     */
    public void fill(long orderId) {
        Order o = ids.remove(orderId);
        long rem = o.qty - o.filled;
        o.filled = o.qty;
        byte side = o.side;
        int t = tickOf(o.price);
        int slot = o.slot;
        unlink(slot, side, t, rem);
        freeSlot(slot);
        repairBest(side, t);
    }
```

`lane(byte)` and `tickOf(long)` already exist as private helpers — use them rather than re-deriving.

Finally, make `rebuildIds` skip freed slots. Restore writes every slot back into the pool including freed ones, but `orderId == 0` must never become a map key:

```java
    public void rebuildIds() {
        ids.clear();
        for (int slot = 0; slot < hwm; slot++) {
            if (pool[slot].orderId != 0) {
                ids.put(pool[slot].orderId, pool[slot]);
            }
        }
    }
```

- [ ] **Step 4: Run the build**

```sh
cd java && ./gradlew build
```

Expected: BUILD SUCCESSFUL, including the pre-existing `Book` tests.

- [ ] **Step 5: Commit**

```sh
git add java/smr-collections-common
git commit -m "feat(smrcoll,java): Book cancel/fill with intrusive LIFO free list

Slots recycle through freeHead; freed slots are marked orderId=0 and chain
via their own next field. Emptying the best level triggers a ladder rescan,
deliberately on the timed path. Agrona's Long2ObjectHashMap already does
compaction-on-remove, so Java needs no equivalent of Go's idMap.del work."
```

---

### Task 3: Snapshot v2 — real `freeHead`, restore validation

**Files:**
- Modify: `java/smr-collections-common/src/main/java/net/knego/hiperf/smrcollections/Snapshotter.java`
- Test: `java/smr-collections-common/src/test/java/net/knego/hiperf/smrcollections/SnapshotTest.java`

**Interfaces:**
- Consumes: `Book.freeHead`, `Book.cancel` (Task 2)

Java's codec is already v2 (regenerated in plan 1). `encode` currently writes a placeholder `enc.freeHead(u32(Book.NIL))` with a comment saying Java has no free list "yet" — that is now false. Without this change a Java image of a churned book would name slot 0 as the free-list head, which is wrong rather than merely different.

**The orders loop does not change.** The image serialises every slot `0..hwm`, freed ones included: a freed slot's own `nextSlot` field carries the chain link, which is why capturing the single `freeHead` scalar suffices.

- [ ] **Step 1: Write the failing tests**

Append to `SnapshotTest.java`:

```java
    private static Book buildBookWithCancels(SmrConfig c, int n, int cancelEvery) {
        Book b = new Book(c);
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        for (int i = 0; i < n; i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins);
            b.insert(ins.orderId, ins.price, ins.qty, ins.side);
            if (i % cancelEvery == cancelEvery - 1) {
                b.cancel(ins.orderId);
            }
        }
        return b;
    }

    private static SmrConfig snapCfg() {
        return new SmrConfig(4096, 64, 1L, 0L, 2000, 0, 0, 4096, 200_000, 20_000, 100);
    }

    @Test
    void roundTripPreservesFreeListOrder() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        assertNotEquals(Book.NIL, b.freeHead, "test needs a non-empty free list");
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        byte[] img = java.util.Arrays.copyOf(s.backing(), n);
        Book r = Snapshotter.restore(img, n, c);
        assertEquals(walkFree(b), walkFree(r), "free list order survives exactly");
    }

    private static java.util.List<Integer> walkFree(Book b) {
        java.util.List<Integer> out = new java.util.ArrayList<>();
        for (int slot = b.freeHead; slot != Book.NIL; slot = b.pool[slot].next) {
            out.add(slot);
        }
        return out;
    }

    @Test
    void restoreAfterCancelsReencodesIdentically() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n1 = s.encode(b);
        byte[] first = java.util.Arrays.copyOf(s.backing(), n1);
        Book r = Snapshotter.restore(first, n1, c);
        Snapshotter s2 = new Snapshotter(4 * 1024 * 1024);
        int n2 = s2.encode(r);
        byte[] second = java.util.Arrays.copyOf(s2.backing(), n2);
        assertArrayEquals(first, second);
    }

    @Test
    void freedSlotsStayOutOfTheIdMap() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        Book r = Snapshotter.restore(java.util.Arrays.copyOf(s.backing(), n), n, c);
        for (int slot = 0; slot < b.hwm(); slot++) {
            long id = b.pool[slot].orderId;
            if (id != 0) {
                assertEquals(slot, r.getSlot(id), "live order " + id + " keeps its slot");
            } else {
                assertEquals(0, r.pool[slot].orderId, "slot " + slot + " stays marked free");
            }
        }
        assertNull(r.ids.get(0L), "orderId 0 must never be a key");
    }

    @Test
    void restoreRejectsCapacityMismatch() {
        SmrConfig c = snapCfg();
        Book b = buildBookWithCancels(c, c.steady(), 4);
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        byte[] img = java.util.Arrays.copyOf(s.backing(), n);
        SmrConfig smaller = new SmrConfig(2048, 64, 1L, 0L, 2000, 0, 0, 2048, 200_000, 20_000, 100);
        assertThrows(IllegalArgumentException.class, () -> Snapshotter.restore(img, n, smaller));
    }
```

Add any missing static imports (`assertNotEquals`, `assertNull`, `assertArrayEquals`, `assertThrows`).

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd java && ./gradlew :smr-collections-common:test --tests '*SnapshotTest*'
```

Expected: `roundTripPreservesFreeListOrder` fails (restored `freeHead` is `NIL`, source is not) and `restoreRejectsCapacityMismatch` fails (no such check yet).

- [ ] **Step 3: Implement**

In `encode`, replace the placeholder line and its four-line comment with:

```java
        enc.freeHead(u32(b.freeHead));
```

In `restore`, after the header is wrapped and before the body is decoded, add the version gate:

```java
        if (header.version() != BookSnapshotEncoder.SCHEMA_VERSION) {
            throw new IllegalArgumentException("unsupported snapshot schema version "
                    + header.version() + " (expected " + BookSnapshotEncoder.SCHEMA_VERSION + ")");
        }
```

After the scalars are read onto `b`, add the capacity check and the free-list head:

```java
        if ((int) dec.capacity() != cfg.cap()) {
            throw new IllegalArgumentException(
                    "snapshot capacity " + dec.capacity() + " != SMRC_CAP " + cfg.cap());
        }
        b.freeHead = (int) dec.freeHead();
```

The capacity check must come before the orders loop, which indexes `b.pool[slot]`. Fixed-block SBE accessors are offset-based, so reading `capacity` and `freeHead` out of declaration order is safe.

The orders loop itself is unchanged — every slot is written back verbatim, which is what restores the chain, and `rebuildIds` (Task 2) already skips the freed ones.

- [ ] **Step 4: Run the build**

```sh
cd java && ./gradlew build
```

Expected: BUILD SUCCESSFUL, including the pre-existing `GoldenTest` — an insert-only book has an empty free list, so `b.freeHead` is `NIL` and the bytes are unchanged.

- [ ] **Step 5: Commit**

```sh
git add java/smr-collections-common
git commit -m "feat(smrcoll,java): encode the real freeHead; validate on restore

Replaces the v2 placeholder with the book's actual free-list head, and adds
the schema-version and capacity checks the Rust and Go restores already had."
```

---

### Task 4: `CowBook` — free list, cancel, `CowRoot.freeHead`, v2 CoW snapshot

**Files:**
- Modify: `.../CowBook.java`, `.../CowRoot.java`, `.../CowSnapshotter.java`
- Test: `.../CowBookTest.java`, `.../CowSnapshotTest.java`

**Interfaces:**
- Produces: `CowBook.freeHead`, `CowBook.cancel`, `CowBook.fill`, `CowRoot.freeHead`

`CowBook` is the chunked copy-on-write twin: pool and ladder live in chunks behind a chunk table, and `capture()` clones the chunk-ref arrays and bumps a generation so the writer copies a chunk before its first write.

**Two things to get right:**

**The placeholder.** `CowSnapshotter` currently writes `enc.freeHead(u32(Book.NIL))`, correct only while `CowBook` had no free list. Change it to the root's real value. Leaving it silently emits a wrong free-list head that only a churn workload exposes — this is the single most likely thing to be missed in this task.

**Accessor discipline.** The ladder rescan must read through the **read-only** level accessor, never the copy-on-write mutable one. A rescan is a read; routing it through the mutable accessor would copy untouched chunks and corrupt the very measurement this store exists to produce. Read `CowBook.java` to find the exact accessor names — the Rust and Go equivalents are `level()`/`LevelAt` (read) versus `level_mut()`/`levelMut` (copy-on-write).

- [ ] **Step 1: Write the failing tests**

Append to `CowBookTest.java`:

```java
    @Test
    void cowCancelMatchesBookCancel() {
        SmrConfig c = new SmrConfig(4096, 64, 1L, 0L, 500, 0, 0, 512, 200_000, 20_000, 100);
        Book b = new Book(c);
        CowBook cb = new CowBook(c);
        Workload.SplitMix r1 = new Workload.SplitMix(Workload.SEED);
        Workload.SplitMix r2 = new Workload.SplitMix(Workload.SEED);
        Workload.Insert i1 = new Workload.Insert();
        Workload.Insert i2 = new Workload.Insert();
        for (int i = 0; i < 500; i++) {
            Workload.nextInsert(r1, i, c.levels(), c.tick(), c.priceMin(), i1);
            Workload.nextInsert(r2, i, c.levels(), c.tick(), c.priceMin(), i2);
            b.insert(i1.orderId, i1.price, i1.qty, i1.side);
            cb.insert(i2.orderId, i2.price, i2.qty, i2.side);
        }
        for (long id = 1; id <= 500; id += 3) {
            b.cancel(id);
            cb.cancel(id);
        }
        assertEquals(b.freeHead, cb.freeHead, "free heads agree");
        assertEquals(b.hwm(), cb.hwm, "hwm agrees");
        assertEquals(b.bestBid(), cb.bestBid, "best bid agrees");
        assertEquals(b.bestAsk(), cb.bestAsk, "best ask agrees");
        for (int t = 0; t < c.levels(); t++) {
            assertEquals(b.levelQty((byte) 0, t), cb.levelQty((byte) 0, t), "bid level " + t);
            assertEquals(b.levelQty((byte) 1, t), cb.levelQty((byte) 1, t), "ask level " + t);
        }
    }

    @Test
    void captureCarriesFreeHead() {
        SmrConfig c = new SmrConfig(4096, 64, 1L, 0L, 100, 0, 0, 512, 200_000, 20_000, 100);
        CowBook cb = new CowBook(c);
        cb.insert(1, 5, 10, (byte) 0);
        cb.insert(2, 5, 10, (byte) 0);
        cb.cancel(1);
        assertEquals(cb.freeHead, cb.capture().freeHead);
    }
```

Append to `CowSnapshotTest.java`:

```java
    @Test
    void cowCancelImageMatchesFlatImage() {
        SmrConfig c = new SmrConfig(4096, 64, 1L, 0L, 2000, 0, 0, 512, 200_000, 20_000, 100);
        Book b = new Book(c);
        CowBook cb = new CowBook(c);
        Workload.SplitMix r1 = new Workload.SplitMix(Workload.SEED);
        Workload.SplitMix r2 = new Workload.SplitMix(Workload.SEED);
        Workload.Insert i1 = new Workload.Insert();
        Workload.Insert i2 = new Workload.Insert();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(r1, i, c.levels(), c.tick(), c.priceMin(), i1);
            Workload.nextInsert(r2, i, c.levels(), c.tick(), c.priceMin(), i2);
            b.insert(i1.orderId, i1.price, i1.qty, i1.side);
            cb.insert(i2.orderId, i2.price, i2.qty, i2.side);
        }
        for (long id = 1; id <= c.steady(); id += 3) {
            b.cancel(id);
            cb.cancel(id);
        }
        Snapshotter s1 = new Snapshotter(4 * 1024 * 1024);
        int n1 = s1.encode(b);
        byte[] flat = java.util.Arrays.copyOf(s1.backing(), n1);
        CowSnapshotter s2 = new CowSnapshotter(4 * 1024 * 1024);
        int n2 = s2.encodeRoot(cb.capture());
        byte[] cow = java.util.Arrays.copyOf(s2.backing(), n2);
        assertArrayEquals(flat, cow, "CoW image == flat image");
    }
```

Adjust `CowSnapshotter`'s constructor/method names to whatever the class actually declares.

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd java && ./gradlew :smr-collections-common:test --tests '*Cow*'
```

Expected: compilation failure — `cb.cancel` and `CowRoot.freeHead` do not exist.

- [ ] **Step 3: Implement**

Add `freeHead` to `CowBook` (initialised `NIL`) and to `CowRoot` as a `public final int`, threading it through `CowRoot`'s constructor and `capture()`'s call to it.

Then port `Book`'s `allocSlot` / `freeSlot` / `unlink` / `repairBest` / `cancel` / `fill` (Task 2) through `CowBook`'s chunk accessors, keeping the operation order identical. The mutable accessor is required wherever a field is written; the read-only accessor is required in `repairBest` and wherever a value is only read. Also make `CowBook.rebuildIds` skip `orderId == 0` slots, exactly as `Book.rebuildIds` does.

- [ ] **Step 4: Mirror the codec changes**

In `CowSnapshotter`, replace the placeholder `enc.freeHead(u32(Book.NIL))` with the root's real value. If the class has a restore path, give it the same schema-version gate, capacity check and `freeHead` read-back that Task 3 added to `Snapshotter` — read `Snapshotter.java` and mirror it, since the two must stay byte-identical for the same logical state.

- [ ] **Step 5: Run the build**

```sh
cd java && ./gradlew build
```

Expected: BUILD SUCCESSFUL, including the pre-existing `CowSnapshotTest` golden check.

- [ ] **Step 6: Commit**

```sh
git add java/smr-collections-common
git commit -m "feat(smrcoll,java): CowBook cancel/fill + freeHead in CowRoot

CowRoot carries freeHead so a restored replica reproduces allocation order.
The ladder rescan reads through the read-only level accessor, so it does not
trigger copy-on-write of untouched chunks."
```

---

### Task 5: The churn driver and the cross-language churn golden

**Files:**
- Create: `.../Churn.java`
- Create: `java/smr-collections-common/src/test/java/net/knego/hiperf/smrcollections/ChurnTest.java`
- Modify: `.../Book.java`, `.../CowBook.java` (add `implements Churn.Store`)

**Interfaces:**
- Produces:
  - `interface Churn.Store { void insert(long orderId, long price, long qty, byte side); void cancel(long orderId); void fill(long orderId); }`
  - `final class Churn.Op { public byte kind; public long orderId, price, qty; public byte side; }` with `Churn.OP_INSERT`/`OP_CANCEL`/`OP_FILL`
  - `Churn(SmrConfig cfg)`, `void nextOp(Op out)`, `void prebuild(Store store, int steady)`, `static void apply(Store store, Op op)`
  - `final class Churn.Samples { public long[] insertNs, cancelNs, fillNs; public int insertN, cancelN, fillN; }`
  - `static Samples Churn.run(SmrConfig cfg, Store store, Churn c, long[] rssOut)`
  - `static void Churn.emit(String experiment, Samples s, long rssGrowth)`

This is where the payoff lands. The merged Rust implementation exported `rust/smr-collections/testdata/golden_churn_snapshot.bin` from a 12,000-op run, and Go already reproduces it byte for byte. `crossLanguageChurnGoldenBytes` does the same from Java — the single check that proves Java's cancel path, free list, id-map removal and encoder all agree with the other two.

**Three things that must be exact:**

**Victim removal is a swap-remove** — move the last live ID into the victim's index, then shorten. That is what Rust's `Vec::swap_remove` and Go's manual equivalent do; any order-preserving removal silently diverges the streams a few ops later.

**Op generation sits outside the timed region** — generate, start clock, apply, stop.

**Follow the allocation-free idiom.** `nextOp(Op out)` fills a caller-owned object rather than returning a new one, matching `Workload.nextInsert(..., Insert out)`. `live` is a `long[]` with an explicit count, not an `ArrayList<Long>` (which would box every ID).

- [ ] **Step 1: Write the failing tests**

Create `ChurnTest.java`:

```java
package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Files;
import java.nio.file.Path;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class ChurnTest {

    private static SmrConfig churnCfg() {
        return new SmrConfig(4096, 64, 1L, 0L, 2000, 0, 0, 4096, 200_000, 20_000, 100);
    }

    @Test
    void opStreamIsDeterministic() {
        SmrConfig c = churnCfg();
        Churn a = new Churn(c);
        Churn b = new Churn(c);
        Churn.Op oa = new Churn.Op();
        Churn.Op ob = new Churn.Op();
        for (int k = 0; k < 10_000; k++) {
            a.nextOp(oa);
            b.nextOp(ob);
            assertEquals(oa.kind, ob.kind, "op " + k + " kind diverged");
            assertEquals(oa.orderId, ob.orderId, "op " + k + " id diverged");
        }
    }

    @Test
    void streamAlternatesAndHonoursOtr() {
        SmrConfig c = churnCfg();
        Churn ch = new Churn(c);
        Book store = new Book(c);
        ch.prebuild(store, c.steady());
        Churn.Op op = new Churn.Op();
        int ins = 0;
        int can = 0;
        int fil = 0;
        for (int i = 0; i < 100_000; i++) {
            ch.nextOp(op);
            if (op.kind == Churn.OP_INSERT) {
                ins++;
            } else if (op.kind == Churn.OP_CANCEL) {
                can++;
            } else {
                fil++;
            }
        }
        assertEquals(50_000, ins, "half the ops are inserts");
        assertEquals(50_000, can + fil, "the other half depart");
        assertTrue(fil >= 300 && fil <= 800, "fills out of band: " + fil);
    }

    @Test
    void liveSetStaysConstant() {
        SmrConfig c = churnCfg();
        Churn ch = new Churn(c);
        Book store = new Book(c);
        ch.prebuild(store, c.steady());
        Churn.Op op = new Churn.Op();
        for (int i = 0; i < 20_000; i++) {
            ch.nextOp(op);
            Churn.apply(store, op);
        }
        int live = 0;
        for (int slot = 0; slot < store.hwm(); slot++) {
            if (store.pool[slot].orderId != 0) {
                live++;
            }
        }
        assertEquals(c.steady(), live, "alternating stream holds the live set flat");
    }

    @Test
    void snapshotRestoreReplayIsBitIdentical() {
        SmrConfig c = churnCfg();
        Churn ch = new Churn(c);
        Book hot = new Book(c);
        ch.prebuild(hot, c.steady());
        Churn.Op op = new Churn.Op();
        for (int i = 0; i < 5000; i++) {
            ch.nextOp(op);
            Churn.apply(hot, op);
        }
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(hot);
        byte[] img = java.util.Arrays.copyOf(s.backing(), n);
        Book cold = Snapshotter.restore(img, n, c);
        // Replay the SAME ops into both.
        Churn.Op[] ops = new Churn.Op[5000];
        for (int i = 0; i < ops.length; i++) {
            ops[i] = new Churn.Op();
            ch.nextOp(ops[i]);
        }
        for (Churn.Op o : ops) {
            Churn.apply(hot, o);
            Churn.apply(cold, o);
        }
        Snapshotter sh = new Snapshotter(4 * 1024 * 1024);
        Snapshotter sc = new Snapshotter(4 * 1024 * 1024);
        int nh = sh.encode(hot);
        int nc = sc.encode(cold);
        assertArrayEquals(
                java.util.Arrays.copyOf(sh.backing(), nh),
                java.util.Arrays.copyOf(sc.backing(), nc),
                "restored replica diverged from the never-restarted one");
    }

    /** The cross-language check: Java must reproduce the image Rust exported, byte for byte. */
    @Test
    void crossLanguageChurnGoldenBytes() throws Exception {
        // Same path idiom as the existing GoldenTest (GoldenTest.java:15).
        byte[] golden = Files.readAllBytes(
                Path.of("..", "..", "rust", "smr-collections", "testdata", "golden_churn_snapshot.bin"));
        SmrConfig c = churnCfg();
        Book b = new Book(c);
        Churn ch = new Churn(c);
        ch.prebuild(b, c.steady());
        Churn.Op op = new Churn.Op();
        for (int i = 0; i < 10_000; i++) {
            ch.nextOp(op);
            Churn.apply(b, op);
        }
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int n = s.encode(b);
        assertArrayEquals(golden, java.util.Arrays.copyOf(s.backing(), n),
                "java churn bytes differ from rust golden");
    }
}
```

(The path above matches `GoldenTest.java:15`, which resolves relative to the subproject directory under Gradle. `Workload.SplitMix.next()` returning `long` is confirmed at `Workload.java:17`.)

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd java && ./gradlew :smr-collections-common:test --tests '*ChurnTest*'
```

Expected: compilation failure — `Churn` does not exist.

- [ ] **Step 3: Implement**

Create `Churn.java`:

```java
package net.knego.hiperf.smrcollections;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;

/**
 * The churn workload: a deterministic insert/cancel/fill stream at a configurable order-to-trade
 * ratio (default 1 %, the real-exchange figure).
 *
 * <p>Op generation is deliberately outside the timed region — the driver produces an op, the
 * caller times only the store's application of it, so the per-op numbers are store work alone.
 * Note this makes them NOT directly comparable with the older insert/update cells, which time
 * their own generation; see the design spec's "Must be recorded in the next run's journal entry".
 */
public final class Churn {

    public static final byte OP_INSERT = 0;
    public static final byte OP_CANCEL = 1;
    public static final byte OP_FILL = 2;

    /** The store surface a churn stream drives. Book and CowBook both implement it. */
    public interface Store {
        void insert(long orderId, long price, long qty, byte side);

        void cancel(long orderId);

        void fill(long orderId);
    }

    /** Reusable op holder — filled by nextOp, never allocated per call. */
    public static final class Op {
        public byte kind;
        public long orderId;
        public long price;
        public long qty;
        public byte side;
    }

    private final Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
    private final Workload.Insert scratch = new Workload.Insert();
    /** Order IDs currently resting, dense so a victim is one uniform draw. */
    private final long[] live;
    private int liveN;
    /**
     * Global op index: drives both the insert/depart alternation and the order ID, so IDs are
     * sparse (1, 3, 5, …) but never reused.
     */
    private int i;

    private final int otrBps;
    private final int levels;
    private final long tick;
    private final long priceMin;

    public Churn(SmrConfig cfg) {
        this.live = new long[cfg.cap()];
        this.otrBps = cfg.otrBps();
        this.levels = cfg.levels();
        this.tick = cfg.tick();
        this.priceMin = cfg.priceMin();
    }

    private void insertOp(Op out) {
        Workload.nextInsert(rng, i, levels, tick, priceMin, scratch);
        i++;
        live[liveN++] = scratch.orderId;
        out.kind = OP_INSERT;
        out.orderId = scratch.orderId;
        out.price = scratch.price;
        out.qty = scratch.qty;
        out.side = scratch.side;
    }

    /**
     * Fill {@code out} with the next op. Even index inserts, odd index departs; a departure is a
     * fill with probability otrBps/10000, otherwise a cancel.
     */
    public void nextOp(Op out) {
        if (i % 2 == 0 || liveN == 0) {
            insertOp(out);
            return;
        }
        i++;
        int v = (int) Long.remainderUnsigned(rng.next(), liveN);
        long id = live[v];
        boolean isFill = Long.remainderUnsigned(rng.next(), 10_000L) < otrBps;
        // swap-remove, matching Rust's Vec::swap_remove exactly — the op streams must be identical.
        live[v] = live[liveN - 1];
        liveN--;
        out.kind = isFill ? OP_FILL : OP_CANCEL;
        out.orderId = id;
    }

    /** Bring the store to its steady-state live set with inserts only. */
    public void prebuild(Store store, int steady) {
        Op op = new Op();
        for (int k = 0; k < steady; k++) {
            insertOp(op);
            apply(store, op);
        }
    }

    public static void apply(Store store, Op op) {
        if (op.kind == OP_INSERT) {
            store.insert(op.orderId, op.price, op.qty, op.side);
        } else if (op.kind == OP_CANCEL) {
            store.cancel(op.orderId);
        } else {
            store.fill(op.orderId);
        }
    }

    /** Per-op-type sample buffers, preallocated so the timed loop never allocates. */
    public static final class Samples {
        public final long[] insertNs;
        public final long[] cancelNs;
        public final long[] fillNs;
        public int insertN;
        public int cancelN;
        public int fillN;

        Samples(int half) {
            this.insertNs = new long[half];
            this.cancelNs = new long[half];
            this.fillNs = new long[half];
        }
    }

    /**
     * Warm up, then time cfg.iters() ops into per-op-type buffers. Only the store call is inside
     * the clock. {@code rssOut[0]} receives the RSS baseline, taken after the buffers are
     * allocated so their pages are not counted as store growth.
     */
    public static Samples run(SmrConfig cfg, Store store, Churn c, long[] rssOut) {
        Op op = new Op();
        for (int k = 0; k < cfg.warmup(); k++) {
            c.nextOp(op);
            apply(store, op);
        }
        Samples s = new Samples(cfg.iters() / 2 + 1);
        rssOut[0] = SmrCollections.rssBytes();
        for (int k = 0; k < cfg.iters(); k++) {
            c.nextOp(op);
            long t0 = System.nanoTime();
            apply(store, op);
            long ns = System.nanoTime() - t0;
            if (op.kind == OP_INSERT) {
                s.insertNs[s.insertN++] = ns;
            } else if (op.kind == OP_CANCEL) {
                s.cancelNs[s.cancelN++] = ns;
            } else {
                s.fillNs[s.fillN++] = ns;
            }
        }
        return s;
    }

    /**
     * Emit the per-op-type distributions plus RSS growth. A distribution with no samples is
     * skipped rather than emitted as zeros — at SMRC_OTR_BPS=0 there are no fills, and a
     * fabricated zero would read as a real measurement.
     */
    public static void emit(String experiment, Samples s, long rssGrowth) {
        if (s.insertN > 0) {
            SmrCollections.emitLatency(experiment, "insert", java.util.Arrays.copyOf(s.insertNs, s.insertN));
        }
        if (s.cancelN > 0) {
            SmrCollections.emitLatency(experiment, "cancel", java.util.Arrays.copyOf(s.cancelNs, s.cancelN));
        }
        if (s.fillN > 0) {
            SmrCollections.emitLatency(experiment, "fill", java.util.Arrays.copyOf(s.fillNs, s.fillN));
        }
        SmrCollections.emitInt(experiment, "rss_growth_bytes", rssGrowth, "bytes", 1);
    }
}
```

Then add `implements Churn.Store` to both `Book` and `CowBook`. Their existing `insert`/`cancel`/`fill` signatures already match, so no method bodies change.

Check `Workload.SplitMix`'s draw-method name and use the real one — the code above assumes `next()` returning `long`. The unsigned remainder helpers (`Long.remainderUnsigned`) matter: Rust and Go treat the draw as `u64`, and Java's `long` is signed, so a plain `%` would diverge on any draw with the high bit set.

- [ ] **Step 4: Run the build**

```sh
cd java && ./gradlew build
```

Expected: BUILD SUCCESSFUL. `crossLanguageChurnGoldenBytes` is the one that matters — it proves Java's whole cancel stack agrees with Rust's and Go's over 12,000 ops. **If it fails, do not regenerate the golden**: the divergence is in this Java code. A length match with differing bytes points at op semantics or field values; a length mismatch points at the free list or the set of occupied levels. The signed-vs-unsigned remainder above is the most likely culprit.

- [ ] **Step 5: Commit**

```sh
git add java/smr-collections-common
git commit -m "feat(smrcoll,java): churn workload driver at ~1% OTR

Alternating insert/depart stream over a dense live array with uniform victim
selection and swap-remove, matching Rust's Vec::swap_remove so the three op
streams are identical. Reusable Op holder keeps the driver allocation-free,
matching the Workload idiom. Verified against the cross-language churn golden."
```

---

### Task 6: The four Java cells

**Files:**
- Modify: `java/settings.gradle.kts`
- Create: `java/smr-collections-churn/{build.gradle.kts,src/main/java/net/knego/hiperf/smrcollections/churn/Main.java}`
- Create: `java/smr-collections-mvcc_churn/...`
- Create: `java/smr-collections-live_stw_churn/...`
- Create: `java/smr-collections-live_mvcc_churn/...`

- [ ] **Step 1: Register the four subprojects**

Add to `java/settings.gradle.kts`'s include list, next to the existing `smr-collections-*` entries:

```kotlin
    "smr-collections-churn",
    "smr-collections-mvcc_churn",
    "smr-collections-live_stw_churn",
    "smr-collections-live_mvcc_churn",
```

Each new `build.gradle.kts` copies `smr-collections-insert/build.gradle.kts` with only the main class changed, e.g.:

```kotlin
plugins {
    application
}

dependencies {
    implementation(project(":common"))
    implementation(project(":smr-collections-common"))
}

application {
    mainClass.set("net.knego.hiperf.smrcollections.churn.Main")
}
```

- [ ] **Step 2: Write `smr-collections-churn`'s `Main.java`**

```java
package net.knego.hiperf.smrcollections.churn;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Churn;

/**
 * smr-collections/churn (Java): insert/cancel/fill at a real-exchange order-to-trade ratio
 * against the flat stop-the-world book. Cancels recycle slots through the free list, so this is
 * the steady state a matching engine actually lives in.
 */
public final class Main {
    private static final String EXPERIMENT = "churn";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            Book book = new Book(cfg);
            Churn churn = new Churn(cfg);
            churn.prebuild(book, cfg.steady());
            long[] rss0 = new long[1];
            Churn.Samples s = Churn.run(cfg, book, churn, rss0);
            long growth = Math.max(0L, SmrCollections.rssBytes() - rss0[0]);
            Churn.emit(EXPERIMENT, s, growth);
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 3: Write `smr-collections-mvcc_churn`'s `Main.java`**

Identical apart from the store, the package and the experiment name:

```java
package net.knego.hiperf.smrcollections.mvccchurn;

import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Churn;
import net.knego.hiperf.smrcollections.CowBook;

/**
 * smr-collections/mvcc_churn (Java): the churn workload against the chunked copy-on-write book.
 * Cancels scatter writes across chunks rather than appending to the newest one, so this is where
 * CoW's first-touch copy cost is exercised hardest.
 */
public final class Main {
    private static final String EXPERIMENT = "mvcc_churn";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
            Churn churn = new Churn(cfg);
            churn.prebuild(book, cfg.steady());
            long[] rss0 = new long[1];
            Churn.Samples s = Churn.run(cfg, book, churn, rss0);
            long growth = Math.max(0L, SmrCollections.rssBytes() - rss0[0]);
            Churn.emit(EXPERIMENT, s, growth);
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 4: Write `smr-collections-live_stw_churn`'s `Main.java`**

Mirror `java/smr-collections-live_stw/.../Main.java` with the churn stream in place of the update-only one, plus the per-op-type split and `rss_peak_bytes`.

**`SmrCollections.rssBytes()` must be called only after the iteration's elapsed time is computed, never between the clock start and stop.** It reads and parses `/proc/self/statm` and allocates while doing so, costing microseconds against sub-microsecond ops; sampling it inside inflates `writer_max`, the headline metric this cell exists to report. This exact defect was found and fixed on the Rust side.

```java
package net.knego.hiperf.smrcollections.livestwchurn;

import java.util.Arrays;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.Book;
import net.knego.hiperf.smrcollections.Churn;
import net.knego.hiperf.smrcollections.Snapshotter;

/**
 * smr-collections/live_stw_churn (Java): writer-observed latency under the churn workload while
 * stop-the-world snapshots run inline at a fixed op cadence (the trigger op pays the whole
 * serialize; writer_max is the stall).
 */
public final class Main {
    private static final String EXPERIMENT = "live_stw_churn";

    public static void main(String[] args) {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            Book book = new Book(cfg);
            Churn churn = new Churn(cfg);
            churn.prebuild(book, cfg.steady());
            Churn.Op op = new Churn.Op();
            for (int k = 0; k < cfg.warmup(); k++) {
                churn.nextOp(op);
                Churn.apply(book, op);
            }
            Snapshotter s = new Snapshotter(64 + cfg.cap() * 64 + cfg.levels() * 2 * 32);
            // warm the encode path + buffer pages so the k=0 trigger measures steady-state
            // stall, not first-touch cost
            s.encode(book);

            long[] writerNs = new long[cfg.liveIters()];
            long[] snapNs = new long[cfg.liveIters() / cfg.snapEvery() + 1];
            int snapCount = 0;
            long snapLen = 0;
            int half = cfg.liveIters() / 2 + 1;
            long[] ins = new long[half];
            long[] can = new long[half];
            long[] fil = new long[half];
            int insN = 0;
            int canN = 0;
            int filN = 0;
            long rssPeak = SmrCollections.rssBytes();
            for (int k = 0; k < cfg.liveIters(); k++) {
                churn.nextOp(op);
                boolean fired = k % cfg.snapEvery() == 0;
                long t0 = System.nanoTime();
                if (fired) {
                    snapLen = s.encode(book);
                    snapNs[snapCount++] = System.nanoTime() - t0;
                }
                Churn.apply(book, op);
                long ns = System.nanoTime() - t0;
                // Sample RSS only AFTER the clock closes: rssBytes() reads and parses
                // /proc/self/statm — microseconds against sub-microsecond ops — so calling it
                // inside the timed region would inflate writer_max, the one metric this cell
                // exists to report precisely.
                if (fired) {
                    rssPeak = Math.max(rssPeak, SmrCollections.rssBytes());
                }
                writerNs[k] = ns;
                if (op.kind == Churn.OP_INSERT) {
                    ins[insN++] = ns;
                } else if (op.kind == Churn.OP_CANCEL) {
                    can[canN++] = ns;
                } else {
                    fil[filN++] = ns;
                }
            }
            SmrCollections.emitLive(EXPERIMENT, writerNs, Arrays.copyOf(snapNs, snapCount), 0, snapLen);
            if (insN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "insert", Arrays.copyOf(ins, insN));
            }
            if (canN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "cancel", Arrays.copyOf(can, canN));
            }
            if (filN > 0) {
                SmrCollections.emitLatency(EXPERIMENT, "fill", Arrays.copyOf(fil, filN));
            }
            SmrCollections.emitInt(EXPERIMENT, "rss_peak_bytes", rssPeak, "bytes", 1);
        } catch (IllegalArgumentException | IllegalStateException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
```

- [ ] **Step 5: Write `smr-collections-live_mvcc_churn`'s `Main.java`**

Read `java/smr-collections-live_mvcc/.../Main.java` first and follow exactly what it does for the capture/serializer handoff and the `skipped` counter — this cell must differ from it only in the workload. Then apply Step 4's three additions, **with the two differences the async snapshot forces**, both mirroring `rust/smr-collections/live_mvcc_churn/src/main.rs`:

1. Gate the mid-loop RSS sample on a `captured` boolean set inside the *non-skipped* branch — not on `fired`, which is also true on iterations where the serializer was busy and the capture was skipped.
2. Take one more RSS sample **after** the serializer is drained and before any trimming of `snapNs`. Step 4's cell needs no equivalent because its snapshot is synchronous and fully inside the timed op; here the final capture's encode and its CoW chunk copies run concurrently and can still be growing memory after the loop's last fired iteration. Without it, `rss_peak_bytes` reads systematically low against the twins in the other two languages.

- [ ] **Step 6: Build and smoke-run all four**

```sh
cd java && ./gradlew build
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_WARMUP=1000 SMRC_ITERS=20000 ./gradlew :smr-collections-churn:run -q
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_WARMUP=1000 SMRC_ITERS=20000 ./gradlew :smr-collections-mvcc_churn:run -q
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_LIVE_ITERS=20000 SMRC_SNAP_EVERY=5000 ./gradlew :smr-collections-live_stw_churn:run -q
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_LIVE_ITERS=20000 SMRC_SNAP_EVERY=5000 ./gradlew :smr-collections-live_mvcc_churn:run -q
```

Expected: each prints result-contract JSON lines carrying `"focus_area":"smr-collections"`, `"language":"java"`, the right `experiment`, and metrics `insert_*`, `cancel_*`, `fill_*`, `rss_growth_bytes` (plus `writer_*`, `snapshot_*`, `rss_peak_bytes` for the live cells).

Note Gradle itself writes to stdout unless `-q` is passed — that is a property of the runner, not the cell, and `bench-infra` invokes it the same way. Confirm that with `-q` the only stdout content is result lines.

**These are local fitness checks, not results. Do not journal them.**

- [ ] **Step 7: Commit**

```sh
git add java/settings.gradle.kts java/smr-collections-churn java/smr-collections-mvcc_churn \
        java/smr-collections-live_stw_churn java/smr-collections-live_mvcc_churn
git commit -m "feat(smrcoll,java): java churn cells — churn, mvcc_churn, live_{stw,mvcc}_churn

Four cells emitting per-op-type distributions (insert/cancel/fill) plus
rss_growth_bytes; the live pair adds writer_max and rss_peak_bytes while a
snapshot is in flight."
```

---

### Task 7: Infra rows and docs — make the grid runnable

**Files:**
- Modify: `bench-infra/ansible/group_vars/all.yml`
- Modify: `CLAUDE.md`

This is the task that turns seven cells per language into something the fleet will actually run. Until it lands, **no churn cell runs anywhere**.

- [ ] **Step 1: Add the experiment rows**

In `bench-infra/ansible/group_vars/all.yml`, after the existing `smr-collections` rows, add seven entries. One row per **experiment**, not per artifact — the matrix fans out over languages, and the `languages:` filter narrows it where a language has no such cell:

```yaml
  # Churn: cancel-heavy workload at a real-exchange order-to-trade ratio
  # (~1% of orders trade). ultima_* are Rust-only; the rest are all three.
  - { focus_area: smr-collections,  experiment: churn,             kind: local }
  - { focus_area: smr-collections,  experiment: mvcc_churn,        kind: local }
  - { focus_area: smr-collections,  experiment: live_stw_churn,    kind: local }
  - { focus_area: smr-collections,  experiment: live_mvcc_churn,   kind: local }
  - { focus_area: smr-collections,  experiment: ultima_churn,       kind: local, languages: [rust] }
  - { focus_area: smr-collections,  experiment: ultima_batch_churn, kind: local, languages: [rust] }
  - { focus_area: smr-collections,  experiment: live_ultima_churn,  kind: local, languages: [rust] }
```

Then add the new knob to the same file's smr-collections params block, next to `smrc_cap`/`smrc_levels`/`smrc_tick`:

```yaml
smrc_otr_bps: 100
```

**Verify the params block is actually plumbed through to the benchmark environment.** Find where the existing `smrc_*` variables are turned into `SMRC_*` env vars — most likely in `bench-infra/ansible/roles/run/tasks/main.yml` or a template — and add `SMRC_OTR_BPS` alongside them. A row in `group_vars` that no task reads is silently inert, and the cells would all run at the default rather than the configured value. Say in your report exactly where you found the mapping and what you changed.

- [ ] **Step 2: Update `CLAUDE.md`**

Two edits, both in the existing text rather than new sections:

1. In the **Build & run** artifact-name list, extend the `smr-collections-{...}` enumerations: add `churn,mvcc_churn,live_stw_churn,live_mvcc_churn` to the all-languages group and `ultima_churn,ultima_batch_churn,live_ultima_churn` to the Rust-only group.
2. In the `smr-collections` status paragraph, add a sentence recording that the grid now also measures a cancel-heavy churn workload at a configurable order-to-trade ratio (`SMRC_OTR_BPS`, default 100 bps = 1 %), that cancel recycles slots through an intrusive free list captured in the snapshot as `freeHead` (schema v2), and that the churn images are golden-verified byte-identical across all three languages.

Keep the register of the surrounding prose — dense and factual, no marketing.

- [ ] **Step 3: Verify nothing else references the experiment list**

```sh
grep -rn "mvcc_snapshot\|live_mvcc" --include=*.yml --include=*.md . | grep -v /build/ | grep -v docs/superpowers
```

Anything that enumerates experiments and has not been updated is a gap — the ansible matrix and `CLAUDE.md` are the two known ones, but check the output and report anything else you find rather than assuming.

- [ ] **Step 4: Sanity-check the YAML**

```sh
cd bench-infra && python3 -c "import yaml,sys; d=yaml.safe_load(open('ansible/group_vars/all.yml')); \
print('experiments:', len(d['experiments'])); print('otr:', d.get('smrc_otr_bps'))"
```

Expected: the experiment count grew by exactly 7, and `otr: 100`.

**Do not run `make bench`, `make up`, or any `terraform` command.** Real runs cost money and are user-initiated.

- [ ] **Step 5: Commit**

```sh
git add bench-infra/ansible CLAUDE.md
git commit -m "feat(bench-infra): churn experiment rows + SMRC_OTR_BPS

Seven new rows (four all-language, three Rust-only) and the OTR knob, plus
the CLAUDE.md artifact list and status paragraph. This is what makes any
churn cell runnable on the fleet — until now none of them ran anywhere."
```

---

## Plan Self-Review

**Spec coverage.** Config → Task 1. `Book` → Task 2. Snapshot v2 restore validation → Tasks 3 and 4 (the schema itself landed in plan 1). `CowBook` → Task 4. Op stream and metrics → Task 5. Cells → Task 6. Infra rows, `SMRC_OTR_BPS` plumbing and `CLAUDE.md` → Task 7. Testing item 1 (bit-identical resumption) → Task 5; item 2's cross-language half → Task 5's golden test plus the pre-existing `GoldenTest`; items 3–5 → Tasks 2, 3, 4. `UltimaBook` and the canonical digest are Rust-only by design.

**What this plan inherits rather than re-derives.** The SBE schema, both golden files, and Java's regenerated v2 codec all landed in plan 1. This plan only replaces the `freeHead = NIL` placeholders with real values. If a golden test fails here, the bug is in this plan's Java code, never in the golden.

**Three Java-specific risks, flagged at the point of use rather than glossed:**

- **`SmrConfig` is a record**, so adding `otrBps` breaks every construction site at compile time. Task 1 Step 1 greps for them first; the compiler will find any that grep misses.
- **Signed vs unsigned remainder.** Rust and Go treat the RNG draw as `u64`; Java's `long` is signed. A plain `%` on a draw with the high bit set yields a different victim index and silently diverges the op stream a few ops later — visible only as a golden mismatch. Task 5 uses `Long.remainderUnsigned` and names this as the first thing to suspect if the golden fails.
- **Cells are Gradle subprojects**, so a new cell needs a `settings.gradle.kts` entry, a `build.gradle.kts` with the right `mainClass`, and a package matching the directory. Task 6 Step 1 does all three together; missing the settings entry produces a confusing "project not found" rather than a compile error.

**Two carried corrections from the merged plans**, both written into Task 6 so Java does not repeat them: `rssBytes()` outside the timed region in both live cells, and the async-only additions (`captured` gate plus post-drain sample) in `live_mvcc_churn`.

**Where the risk is concentrated.** Task 5's `crossLanguageChurnGoldenBytes` is both the highest-value check and the most likely to fail first, because it is the first point at which Java's entire cancel stack is compared against bytes the other two languages already agree on. Task 7 is low-risk but high-consequence: it is the only task whose omission would leave the whole three-plan effort unrunnable, and its one subtle step is confirming that `smrc_otr_bps` is actually plumbed into `SMRC_OTR_BPS` rather than merely declared.
