# smr-collections Churn — Go Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring Go to parity with the Rust cancel/churn work — a cancel op with slot recycling on both Go stores, schema-v2 restore validation, a deterministic churn driver, and the four Go churn cells.

**Architecture:** Mirrors the Rust implementation op-for-op, because the two must produce byte-identical snapshot images. The one genuinely Go-specific piece is deletion from the hand-rolled open-addressed `idMap`, which needs backward-shift compaction — Rust gets removal free from `HashMap`, Java from Agrona. Everything else is a transliteration whose correctness is pinned by two cross-language golden files.

**Tech Stack:** Go 1.22, single module, real-logic `sbe-tool` 1.38.1 (committed generated codec).

**Spec:** [`docs/superpowers/specs/2026-07-30-smr-collections-cancel-churn-design.md`](../specs/2026-07-30-smr-collections-cancel-churn-design.md)

## Scope

**Plan 2 of 3.** Plan 1 (Rust core) is merged: schema v2 landed, both goldens exist, and Go's codec was already regenerated for v2 with both encoders writing `freeHead = NIL` as a placeholder. Plan 3 covers Java plus the ansible matrix rows and `CLAUDE.md` — infra goes last because adding matrix rows before all languages exist breaks a fleet run.

On completion `go build ./... && go vet ./... && go test ./...` is green, Go verifies against **both** golden files, and `churn` / `mvcc_churn` / `live_stw_churn` / `live_mvcc_churn` run in Go.

Not in this plan: any ultima cell (Rust-only — Go has no MVCC-engine adapter), the canonical digest (only needed to compare against ultima), and anything under `rust/`, `java/` or `bench-infra/`.

## Global Constraints

- Go **1.22**, single module. `go build ./... && go vet ./... && go test ./...` must all pass before any commit.
- **stdout is result-contract JSON lines only.** Logs, progress and diagnostics go to stderr — a stray `fmt.Println` breaks the downstream journal tooling silently.
- Result lines come **only** from `internal/bench` helpers (`EmitSmrLatency`, `EmitSmrInt`, `EmitSmrLive`), never hand-rolled JSON. Every line carries `focus_area: "smr-collections"` and the cell's `experiment`, which must exactly match the command-directory suffix.
- **Determinism is the top requirement.** Go and Rust must produce byte-identical images from the same op stream, on any host, and across snapshot/restore. Go must never iterate a `map` where the order reaches output.
- Order IDs start at **1**; `OrderID == 0` is the freed-slot marker. `NIL == 0xFFFFFFFF`.
- Fixed capacity **never grows** — no rehash, no realloc. Exhaustion fails loudly.
- `SMRC_OTR_BPS` is the order-to-trade ratio in basis points, default **100** (= 1 %), valid range **0..=10000**.
- The two golden files under `rust/smr-collections/testdata/` are **read-only** here. A mismatch is a real finding — report it, never regenerate.
- Churn cells recycle slots and must NOT call `RequireBumpCapacity()`.
- Op **generation** sits outside the timed region: generate the op, start the clock, apply, stop. (This differs from the older `insert`/`update` cells, which time their own generation — a known, documented asymmetry.)
- Do NOT run any AWS benchmark, `terraform`, or anything under `bench-infra/`. Do not touch `rust/` or `java/`.

## File Structure

**Modified:**
- `go/internal/bench/smrcoll.go` — `OtrBps` config field, `RequireBumpCapacity()`, relocated capacity check
- `go/internal/bench/config.go` — `nonNegativeEnv`, `RSSBytes()`
- `go/internal/smrcoll/book.go` — `idMap.del` with backward-shift compaction; free list, `Cancel`, `Fill`, `repairBest`
- `go/internal/smrcoll/snapshot.go` — write real `FreeHead`; version/capacity validation on restore
- `go/internal/smrcoll/cowbook.go` — same store changes, `CowRoot.FreeHead`
- `go/internal/smrcoll/cowsnapshot.go` — same codec changes
- `go/cmd/smr-collections-{insert,mvcc_insert}/main.go` — call `RequireBumpCapacity()`

**Created:**
- `go/internal/smrcoll/churn.go` — `ChurnStore`, `ChurnOp`, `Churn`, `RunChurn`, `EmitChurn`
- `go/internal/smrcoll/churn_test.go` — driver tests + the cross-language churn golden check
- `go/cmd/smr-collections-{churn,mvcc_churn,live_stw_churn,live_mvcc_churn}/main.go`

---

### Task 1: Config — `SMRC_OTR_BPS`, capacity-check refactor, RSS helper

**Files:**
- Modify: `go/internal/bench/smrcoll.go`
- Modify: `go/internal/bench/config.go`
- Modify: `go/cmd/smr-collections-insert/main.go`, `go/cmd/smr-collections-mvcc_insert/main.go`
- Test: `go/internal/bench/smrcoll_test.go`

**Interfaces:**
- Produces: `bench.SmrConfig.OtrBps int`; `func (c SmrConfig) RequireBumpCapacity() error`; `func RSSBytes() int64`

- [ ] **Step 1: Write the failing tests**

Append to `go/internal/bench/smrcoll_test.go`:

```go
func TestOtrBpsDefaultsTo100(t *testing.T) {
	os.Unsetenv("SMRC_OTR_BPS")
	c, err := LoadSmrConfig()
	if err != nil {
		t.Fatalf("defaults must parse: %v", err)
	}
	if c.OtrBps != 100 {
		t.Fatalf("OtrBps = %d, want 100 (1%%)", c.OtrBps)
	}
}

func TestOtrBpsZeroIsLegalAndOver10000Rejected(t *testing.T) {
	os.Setenv("SMRC_OTR_BPS", "0")
	c, err := LoadSmrConfig()
	os.Unsetenv("SMRC_OTR_BPS")
	if err != nil {
		t.Fatalf("0 bps (pure-cancel run) must be legal: %v", err)
	}
	if c.OtrBps != 0 {
		t.Fatalf("OtrBps = %d, want 0", c.OtrBps)
	}
	os.Setenv("SMRC_OTR_BPS", "10001")
	_, err = LoadSmrConfig()
	os.Unsetenv("SMRC_OTR_BPS")
	if err == nil {
		t.Fatal("OTR above 100% must be rejected")
	}
}

func TestChurnSizedRunParsesButFailsBumpCapacity(t *testing.T) {
	// warmup+iters > cap is legal for a slot-recycling churn cell and illegal
	// for a bump-allocating insert cell.
	os.Setenv("SMRC_CAP", "1024")
	os.Setenv("SMRC_STEADY", "512")
	os.Setenv("SMRC_CHUNK", "256")
	os.Setenv("SMRC_WARMUP", "1000")
	os.Setenv("SMRC_ITERS", "10000")
	c, err := LoadSmrConfig()
	bumpErr := error(nil)
	if err == nil {
		bumpErr = c.RequireBumpCapacity()
	}
	for _, k := range []string{"SMRC_CAP", "SMRC_STEADY", "SMRC_CHUNK", "SMRC_WARMUP", "SMRC_ITERS"} {
		os.Unsetenv(k)
	}
	if err != nil {
		t.Fatalf("churn-sized config must parse: %v", err)
	}
	if bumpErr == nil {
		t.Fatal("bump-allocating cells must reject warmup+iters > cap")
	}
}

func TestRSSBytesIsNonzero(t *testing.T) {
	if RSSBytes() <= 0 {
		t.Fatal("RSS must be readable from /proc/self/statm")
	}
}
```

Add `"os"` to that file's imports if absent.

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd go && go test ./internal/bench/ -run 'OtrBps|ChurnSized|RSSBytes' -v
```

Expected: compile failure — `c.OtrBps undefined`, `c.RequireBumpCapacity undefined`, `undefined: RSSBytes`.

- [ ] **Step 3: Implement**

In `go/internal/bench/smrcoll.go`, add to the `SmrConfig` struct:

```go
	// OtrBps is the order-to-trade ratio in basis points: the share of
	// departures that are fills rather than cancels. 100 = 1 %, the
	// real-exchange figure.
	OtrBps int
```

In `LoadSmrConfig`, after the `priceMin` block, add:

```go
	otrBps, err := nonNegativeEnv("SMRC_OTR_BPS", 100)
	if err != nil {
		return SmrConfig{}, err
	}
	if otrBps > 10000 {
		return SmrConfig{}, fmt.Errorf("SMRC_OTR_BPS must be in 0..=10000, got %d", otrBps)
	}
```

Add `OtrBps: otrBps,` to the `cfg := SmrConfig{...}` literal.

**Delete** this block from `LoadSmrConfig` (it is a bump-allocator constraint, not a universal one):

```go
	if warmup+iters > cap_ {
		return SmrConfig{}, fmt.Errorf("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP")
	}
```

Add the method to the same file:

```go
// RequireBumpCapacity reports whether the pool has room for every op a
// bump-allocating cell will run. Churn cells recycle slots and must not call
// it.
func (c SmrConfig) RequireBumpCapacity() error {
	if c.Warmup+c.Iters > c.Cap {
		return fmt.Errorf("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP")
	}
	return nil
}
```

In `go/internal/bench/config.go`, alongside `positiveEnv`:

```go
// nonNegativeEnv is positiveEnv but admits zero, for knobs where zero is a
// meaningful setting rather than a mistake.
func nonNegativeEnv(name string, def int) (int, error) {
	s := os.Getenv(name)
	if s == "" {
		return def, nil
	}
	v, err := strconv.Atoi(s)
	if err != nil {
		return 0, fmt.Errorf("%s: %q is not a valid integer", name, s)
	}
	if v < 0 {
		return 0, fmt.Errorf("%s: must not be negative, got %d", name, v)
	}
	return v, nil
}

// RSSBytes returns resident set size in bytes from Linux /proc/self/statm
// field 2 (resident pages), or 0 where unreadable. The bench hosts are
// x86-64 Linux with 4 KiB pages, which is the only case that must be right.
func RSSBytes() int64 {
	data, err := os.ReadFile("/proc/self/statm")
	if err != nil {
		return 0
	}
	fields := strings.Fields(string(data))
	if len(fields) < 2 {
		return 0
	}
	pages, err := strconv.ParseInt(fields[1], 10, 64)
	if err != nil {
		return 0
	}
	return pages * 4096
}
```

Add `"strings"` to that file's imports.

- [ ] **Step 4: Guard the bump-allocating cells**

In each of `go/cmd/smr-collections-insert/main.go` and `go/cmd/smr-collections-mvcc_insert/main.go`, immediately after the `LoadSmrConfig` error check, insert:

```go
	if err := cfg.RequireBumpCapacity(); err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
```

Leave `update`, `snapshot`, `mvcc_update`, `mvcc_snapshot`, `live_stw` and `live_mvcc` alone — they pre-build `Steady` orders then only mutate, so the universal `steady <= cap` check already covers them.

- [ ] **Step 5: Run the suite**

```sh
cd go && go build ./... && go vet ./... && go test ./...
```

Expected: PASS. Any pre-existing test that relied on `LoadSmrConfig` rejecting `warmup+iters > cap` will now fail — if one does, move its expectation to `RequireBumpCapacity()` rather than restoring the old check.

- [ ] **Step 6: Commit**

```sh
git add go/internal/bench go/cmd/smr-collections-insert go/cmd/smr-collections-mvcc_insert
git commit -m "feat(smrcoll,go): SMRC_OTR_BPS + RequireBumpCapacity() + RSSBytes()

Moves warmup+iters<=cap out of LoadSmrConfig into an explicit check the
bump-allocating cells call, so slot-recycling churn cells can run longer
than SMRC_CAP. Mirrors the Rust side."
```

---

### Task 2: `idMap.del` — backward-shift compaction

**Files:**
- Modify: `go/internal/smrcoll/book.go`
- Test: `go/internal/smrcoll/book_test.go`

**Interfaces:**
- Produces: `func (m *idMap) del(k int64)`

This is the one piece with no Rust counterpart. `idMap` is hand-rolled open addressing with linear probing (`book.go:23-57`). Deleting by simply zeroing a slot would break every probe chain that ran through it — subsequent lookups would stop early at the hole and report absent. Tombstones are the usual fix, but they rot: probe chains only grow, so lookup cost degrades over uptime, which is precisely what a churn benchmark must not do. Backward-shift compaction (what Agrona does) keeps chains tight forever.

- [ ] **Step 1: Write the failing tests**

Append to `go/internal/smrcoll/book_test.go`:

```go
func TestIDMapDeleteKeepsSurvivorsFindable(t *testing.T) {
	m := newIDMap(1024)
	for k := int64(1); k <= 500; k++ {
		m.put(k, uint32(k))
	}
	for k := int64(1); k <= 500; k += 2 {
		m.del(k)
	}
	for k := int64(1); k <= 500; k++ {
		got := m.get(k)
		if k%2 == 1 {
			if got != NIL {
				t.Fatalf("deleted key %d still resolves to %d", k, got)
			}
		} else if got != uint32(k) {
			t.Fatalf("surviving key %d: got %d, want %d", k, got, k)
		}
	}
}

func TestIDMapSurvivesLongChurn(t *testing.T) {
	// A steady live set with continuous turnover — the churn workload's shape.
	// Without backward-shift compaction this degrades or returns wrong answers.
	m := newIDMap(256)
	live := make(map[int64]uint32)
	for k := int64(1); k <= 100; k++ {
		m.put(k, uint32(k))
		live[k] = uint32(k)
	}
	for k := int64(101); k <= 5000; k++ {
		old := k - 100
		m.del(old)
		delete(live, old)
		m.put(k, uint32(k))
		live[k] = uint32(k)
	}
	for k, v := range live {
		if got := m.get(k); got != v {
			t.Fatalf("key %d: got %d, want %d", k, got, v)
		}
	}
	if got := m.get(999999); got != NIL {
		t.Fatalf("absent key resolved to %d — probe chain did not terminate", got)
	}
}

func TestIDMapDeleteAbsentKeyIsANoop(t *testing.T) {
	m := newIDMap(64)
	m.put(7, 7)
	m.del(1234)
	if got := m.get(7); got != 7 {
		t.Fatalf("deleting an absent key disturbed the table: got %d", got)
	}
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd go && go test ./internal/smrcoll/ -run IDMap -v
```

Expected: compile failure — `m.del undefined`.

- [ ] **Step 3: Implement**

Add to `go/internal/smrcoll/book.go`, next to `put`/`get`:

```go
// del removes k and backward-shift compacts the run behind it, so probe
// chains never accumulate tombstones and lookup cost stays flat over uptime.
// The resulting table is a pure function of the operations applied — which
// matters because the id-map is rebuilt from the pool on restore and both
// replicas must agree.
func (m *idMap) del(k int64) {
	i := (uint64(k) * 0x9E3779B97F4A7C15) & m.mask
	for m.keys[i] != k {
		if m.keys[i] == 0 {
			return // absent
		}
		i = (i + 1) & m.mask
	}
	m.keys[i] = 0
	j := i
	for {
		j = (j + 1) & m.mask
		if m.keys[j] == 0 {
			return
		}
		h := (uint64(m.keys[j]) * 0x9E3779B97F4A7C15) & m.mask
		// Move keys[j] into the hole at i iff its ideal slot h does NOT lie
		// cyclically within (i, j] — otherwise the move would place it before
		// its own probe start and make it unreachable.
		if ((j - h) & m.mask) >= ((j - i) & m.mask) {
			m.keys[i] = m.keys[j]
			m.vals[i] = m.vals[j]
			m.keys[j] = 0
			i = j
		}
	}
}
```

- [ ] **Step 4: Run tests to verify they pass**

```sh
cd go && go test ./internal/smrcoll/ -run IDMap -v
```

Expected: PASS, all three.

- [ ] **Step 5: Commit**

```sh
git add go/internal/smrcoll/book.go go/internal/smrcoll/book_test.go
git commit -m "feat(smrcoll,go): idMap.del with backward-shift compaction

Go's id-map is hand-rolled open addressing, so removal needs explicit
compaction; Rust gets it from HashMap and Java from Agrona. Tombstones
would rot probe chains over a churn run, which is exactly the degradation
this benchmark exists to detect."
```

---

### Task 3: `Book` — free list, `Cancel`, `Fill`, best-price rescan

**Files:**
- Modify: `go/internal/smrcoll/book.go`
- Test: `go/internal/smrcoll/book_test.go`

**Interfaces:**
- Consumes: `idMap.del` (Task 2)
- Produces: `Book.FreeHead uint32`; `func (b *Book) Cancel(orderID int64)`; `func (b *Book) Fill(orderID int64)`

Semantics must match Rust's `Book` exactly — the two produce byte-identical images, so any divergence in operation order, in what the withdrawn quantity is computed from, or in the link fixups is a real defect.

- [ ] **Step 1: Write the failing tests**

Append to `go/internal/smrcoll/book_test.go` (the file's existing tests show the `bench.SmrConfig` literal style to follow):

```go
func churnTestCfg() bench.SmrConfig {
	return bench.SmrConfig{Cap: 1024, Levels: 16, Tick: 1, PriceMin: 0, Steady: 100, Chunk: 256, OtrBps: 100}
}

func TestCancelUnlinksMiddleOfLevelFIFO(t *testing.T) {
	b := NewBook(churnTestCfg())
	b.Insert(1, 5, 10, 0)
	b.Insert(2, 5, 7, 0)
	b.Insert(3, 5, 3, 0)
	b.Cancel(2)
	if got := b.LevelQty(0, 5); got != 13 {
		t.Fatalf("level qty = %d, want 13", got)
	}
	lvl := b.Bids[5]
	if lvl.Count != 2 || lvl.Head != 0 || lvl.Tail != 2 {
		t.Fatalf("level = %+v, want count 2 head 0 tail 2", lvl)
	}
	if b.Pool[0].Next != 2 || b.Pool[2].Prev != 0 {
		t.Fatalf("links not re-stitched: pool[0].Next=%d pool[2].Prev=%d", b.Pool[0].Next, b.Pool[2].Prev)
	}
}

func TestCancelHeadAndTailFixLevelEnds(t *testing.T) {
	b := NewBook(churnTestCfg())
	b.Insert(1, 5, 10, 0)
	b.Insert(2, 5, 7, 0)
	b.Cancel(1) // head
	if b.Bids[5].Head != 1 || b.Pool[1].Prev != NIL {
		t.Fatalf("head did not advance: head=%d prev=%d", b.Bids[5].Head, b.Pool[1].Prev)
	}
	b.Cancel(2) // tail; level now empty
	lvl := b.Bids[5]
	if lvl.Head != NIL || lvl.Tail != NIL || lvl.Count != 0 || lvl.QtyTotal != 0 {
		t.Fatalf("emptied level = %+v", lvl)
	}
}

func TestCancelEmptyingBestLevelRescans(t *testing.T) {
	b := NewBook(churnTestCfg())
	b.Insert(1, 3, 10, 0)
	b.Insert(2, 9, 10, 0) // best bid = 9
	b.Insert(3, 4, 10, 1)
	b.Insert(4, 2, 10, 1) // best ask = 2
	b.Cancel(2)
	if b.BestBid != 3 {
		t.Fatalf("best bid = %d, want 3 (next occupied below)", b.BestBid)
	}
	b.Cancel(4)
	if b.BestAsk != 4 {
		t.Fatalf("best ask = %d, want 4 (next occupied above)", b.BestAsk)
	}
	b.Cancel(1)
	if b.BestBid != -1 {
		t.Fatalf("best bid = %d, want -1 (side empty)", b.BestBid)
	}
}

func TestCancelledSlotsAreReusedLIFO(t *testing.T) {
	b := NewBook(churnTestCfg())
	b.Insert(1, 5, 10, 0) // slot 0
	b.Insert(2, 5, 10, 0) // slot 1
	b.Insert(3, 5, 10, 0) // slot 2
	b.Cancel(1)           // free: 0
	b.Cancel(3)           // free: 2 -> 0
	if b.FreeHead != 2 {
		t.Fatalf("FreeHead = %d, want 2", b.FreeHead)
	}
	b.Insert(4, 5, 10, 0)
	if got := b.GetSlot(4); got != 2 {
		t.Fatalf("LIFO reuse: slot = %d, want 2", got)
	}
	b.Insert(5, 5, 10, 0)
	if got := b.GetSlot(5); got != 0 {
		t.Fatalf("LIFO reuse: slot = %d, want 0", got)
	}
	b.Insert(6, 5, 10, 0)
	if got := b.GetSlot(6); got != 3 || b.Hwm != 4 {
		t.Fatalf("free list empty must bump hwm: slot=%d hwm=%d", got, b.Hwm)
	}
}

func TestFreedSlotIsMarkedWithZeroOrderID(t *testing.T) {
	b := NewBook(churnTestCfg())
	b.Insert(1, 5, 10, 0)
	b.Cancel(1)
	if b.Pool[0].OrderID != 0 {
		t.Fatalf("freed slot OrderID = %d, want 0 (the snapshot walk's marker)", b.Pool[0].OrderID)
	}
}

func TestFillCompletesThenFreesTheSlot(t *testing.T) {
	b := NewBook(churnTestCfg())
	b.Insert(1, 5, 10, 0)
	b.Update(1, 4) // partial: remaining 6
	if got := b.LevelQty(0, 5); got != 6 {
		t.Fatalf("after partial fill level qty = %d, want 6", got)
	}
	b.Fill(1)
	if got := b.LevelQty(0, 5); got != 0 {
		t.Fatalf("after full fill level qty = %d, want 0", got)
	}
	if b.Bids[5].Count != 0 || b.FreeHead != 0 {
		t.Fatalf("fill must recycle like a cancel: count=%d freeHead=%d", b.Bids[5].Count, b.FreeHead)
	}
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd go && go test ./internal/smrcoll/ -run 'Cancel|Fill|Freed' -v
```

Expected: compile failure — `b.Cancel undefined`, `b.FreeHead undefined`.

- [ ] **Step 3: Implement**

Add `FreeHead uint32` to the `Book` struct and set `FreeHead: NIL` in `NewBook`'s literal.

Replace the first two lines of `Insert`'s body (`slot := b.Hwm` / `b.Hwm++`) with:

```go
	slot := b.allocSlot()
```

Add these methods:

```go
func (b *Book) allocSlot() uint32 {
	if b.FreeHead != NIL {
		slot := b.FreeHead
		b.FreeHead = b.Pool[slot].Next
		return slot
	}
	if int(b.Hwm) == len(b.Pool) {
		panic(fmt.Sprintf("order pool exhausted: SMRC_CAP=%d reached", len(b.Pool)))
	}
	slot := b.Hwm
	b.Hwm++
	return slot
}

func (b *Book) freeSlot(slot uint32) {
	head := b.FreeHead
	o := &b.Pool[slot]
	o.OrderID = 0 // freed marker: the snapshot walk skips these
	o.Next = head
	o.Prev = NIL
	b.FreeHead = slot
}

// unlink removes slot from its level's intrusive FIFO and debits rem from the
// level's remaining quantity.
func (b *Book) unlink(slot uint32, side uint8, t uint32, rem int64) {
	prev, next := b.Pool[slot].Prev, b.Pool[slot].Next
	if prev != NIL {
		b.Pool[prev].Next = next
	}
	if next != NIL {
		b.Pool[next].Prev = prev
	}
	lvl := &b.lane(side)[t]
	if lvl.Head == slot {
		lvl.Head = next
	}
	if lvl.Tail == slot {
		lvl.Tail = prev
	}
	lvl.QtyTotal -= rem
	lvl.Count--
}

// repairBest restores the cached best for side after a removal emptied level
// t. O(levels) worst case and deliberately on the timed path — real books
// maintain this, and hiding it would hide the worst-case cancel.
func (b *Book) repairBest(side uint8, t uint32) {
	if side == 0 {
		if b.BestBid != int32(t) || b.Bids[t].Head != NIL {
			return
		}
		nb := int32(-1)
		for i := int(t); i >= 0; i-- {
			if b.Bids[i].Head != NIL {
				nb = int32(i)
				break
			}
		}
		b.BestBid = nb
		return
	}
	if b.BestAsk != int32(t) || b.Asks[t].Head != NIL {
		return
	}
	na := int32(-1)
	for i := int(t); i < int(b.NLevels); i++ {
		if b.Asks[i].Head != NIL {
			na = int32(i)
			break
		}
	}
	b.BestAsk = na
}

// Cancel removes a resting order; its remaining quantity leaves the level.
func (b *Book) Cancel(orderID int64) {
	slot := b.ids.get(orderID)
	o := b.Pool[slot]
	rem := o.Qty - o.Filled
	t := b.tickOf(o.Price)
	b.ids.del(orderID)
	b.unlink(slot, o.Side, t, rem)
	b.freeSlot(slot)
	b.repairBest(o.Side, t)
}

// Fill completes an order then removes it. Same structural work as Cancel;
// the difference is that the departing quantity is booked as filled rather
// than withdrawn.
func (b *Book) Fill(orderID int64) {
	slot := b.ids.get(orderID)
	o := &b.Pool[slot]
	rem := o.Qty - o.Filled
	o.Filled = o.Qty
	side, price := o.Side, o.Price
	t := b.tickOf(price)
	b.ids.del(orderID)
	b.unlink(slot, side, t, rem)
	b.freeSlot(slot)
	b.repairBest(side, t)
}
```

Add `"fmt"` to the file's imports.

Finally, make `rebuildIDs` skip freed slots — restore writes every slot back into the pool, including freed ones, but they must not enter the id-map:

```go
func (b *Book) rebuildIDs() {
	b.ids = newIDMap(len(b.Pool))
	for slot := uint32(0); slot < b.Hwm; slot++ {
		if b.Pool[slot].OrderID != 0 {
			b.ids.put(b.Pool[slot].OrderID, slot)
		}
	}
}
```

- [ ] **Step 4: Run the suite**

```sh
cd go && go test ./internal/smrcoll/
```

Expected: PASS, including the pre-existing `Book` tests.

- [ ] **Step 5: Commit**

```sh
git add go/internal/smrcoll/book.go go/internal/smrcoll/book_test.go
git commit -m "feat(smrcoll,go): Book Cancel/Fill with intrusive LIFO free list

Slots recycle through FreeHead; freed slots are marked OrderID=0 and chain
via their own Next field. Emptying the best level triggers a ladder rescan,
deliberately on the timed path. Mirrors the Rust Book op-for-op."
```

---

### Task 4: Snapshot v2 — real `FreeHead`, restore validation

**Files:**
- Modify: `go/internal/smrcoll/snapshot.go`
- Test: `go/internal/smrcoll/snapshot_test.go`

**Interfaces:**
- Consumes: `Book.FreeHead`, `Book.Cancel` (Task 3)

Go's codec is already v2 (regenerated in plan 1). `Encode` currently writes a placeholder `msg.FreeHead = NIL` with a comment saying Go has no free list "yet" — that is now false and must become the real value. Without this change a Go image of a churned book would name slot 0 as the free-list head.

- [ ] **Step 1: Write the failing tests**

Append to `go/internal/smrcoll/snapshot_test.go`:

```go
func buildBookWithCancels(c bench.SmrConfig, n, cancelEvery int) *Book {
	b := NewBook(c)
	rng := NewSplitMix(SmrSeed)
	for i := 0; i < n; i++ {
		ins := NextInsert(rng, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
		if i%cancelEvery == cancelEvery-1 {
			b.Cancel(ins.OrderID)
		}
	}
	return b
}

func TestRoundTripPreservesFreeListOrder(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Chunk: 4096, OtrBps: 100}
	b := buildBookWithCancels(c, c.Steady, 4)
	if b.FreeHead == NIL {
		t.Fatal("test needs a non-empty free list")
	}
	s := NewSnapshotter()
	img := append([]byte(nil), s.Encode(b)...)
	r, err := Restore(img, c)
	if err != nil {
		t.Fatalf("restore: %v", err)
	}
	walk := func(bk *Book) []uint32 {
		var out []uint32
		for slot := bk.FreeHead; slot != NIL; slot = bk.Pool[slot].Next {
			out = append(out, slot)
		}
		return out
	}
	got, want := walk(r), walk(b)
	if len(got) != len(want) {
		t.Fatalf("free list length %d, want %d", len(got), len(want))
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("free list[%d] = %d, want %d", i, got[i], want[i])
		}
	}
}

func TestRestoreAfterCancelsReencodesIdentically(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Chunk: 4096, OtrBps: 100}
	b := buildBookWithCancels(c, c.Steady, 4)
	s := NewSnapshotter()
	first := append([]byte(nil), s.Encode(b)...)
	r, err := Restore(first, c)
	if err != nil {
		t.Fatalf("restore: %v", err)
	}
	second := NewSnapshotter().Encode(r)
	if !bytes.Equal(first, second) {
		t.Fatalf("re-encode differs: %d vs %d bytes", len(first), len(second))
	}
}

func TestFreedSlotsStayOutOfTheIDMap(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Chunk: 4096, OtrBps: 100}
	b := buildBookWithCancels(c, c.Steady, 4)
	s := NewSnapshotter()
	r, err := Restore(append([]byte(nil), s.Encode(b)...), c)
	if err != nil {
		t.Fatalf("restore: %v", err)
	}
	for slot := uint32(0); slot < b.Hwm; slot++ {
		id := b.Pool[slot].OrderID
		if id != 0 {
			if got := r.GetSlot(id); got != slot {
				t.Fatalf("live order %d: slot %d, want %d", id, got, slot)
			}
		} else if r.Pool[slot].OrderID != 0 {
			t.Fatalf("slot %d lost its freed marker", slot)
		}
	}
	if got := r.GetSlot(0); got != NIL {
		t.Fatalf("OrderID 0 must never be a key, got slot %d", got)
	}
}

func TestRestoreRejectsCapacityMismatch(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Chunk: 4096, OtrBps: 100}
	b := buildBookWithCancels(c, c.Steady, 4)
	img := NewSnapshotter().Encode(b)
	smaller := c
	smaller.Cap = 2048
	if _, err := Restore(img, smaller); err == nil {
		t.Fatal("restoring into a smaller-capacity build must fail loudly")
	}
}
```

Add `"bytes"` to that file's imports if absent.

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd go && go test ./internal/smrcoll/ -run 'FreeList|Reencodes|FreedSlots|CapacityMismatch' -v
```

Expected: `TestRoundTripPreservesFreeListOrder` fails (restored `FreeHead` is `NIL`, source is not) and `TestRestoreRejectsCapacityMismatch` fails (no such check yet).

- [ ] **Step 3: Implement**

In `Encode`, replace the placeholder assignment and its three-line comment with:

```go
	msg.FreeHead = b.FreeHead
```

In `Restore`, after `hdr.Decode` succeeds and before `msg.Decode`, add the version gate:

```go
	if hdr.Version != msg.SbeSchemaVersion() {
		return nil, fmt.Errorf("unsupported snapshot schema version %d (expected %d)", hdr.Version, msg.SbeSchemaVersion())
	}
```

After the scalars are copied onto `b`, add the capacity check and the free-list head:

```go
	if int(msg.Capacity) != cfg.Cap {
		return nil, fmt.Errorf("snapshot capacity %d != SMRC_CAP %d", msg.Capacity, cfg.Cap)
	}
	b.FreeHead = msg.FreeHead
```

The orders loop is unchanged — every slot is written back to the pool verbatim, which is what restores the free chain; `rebuildIDs` (Task 3) already skips the freed ones.

- [ ] **Step 4: Run the suite**

```sh
cd go && go test ./internal/smrcoll/
```

Expected: PASS, including `TestCrossLanguageGoldenBytes` — an insert-only book has an empty free list, so `b.FreeHead` is `NIL` and the bytes are unchanged.

- [ ] **Step 5: Commit**

```sh
git add go/internal/smrcoll/snapshot.go go/internal/smrcoll/snapshot_test.go
git commit -m "feat(smrcoll,go): encode the real FreeHead; validate on restore

Replaces the v2 placeholder with the book's actual free-list head, and adds
the schema-version and capacity checks the Rust restore already had."
```

---

### Task 5: `CowBook` — free list, cancel, `CowRoot.FreeHead`, v2 CoW snapshot

**Files:**
- Modify: `go/internal/smrcoll/cowbook.go`
- Modify: `go/internal/smrcoll/cowsnapshot.go`
- Test: `go/internal/smrcoll/cowbook_test.go`, `go/internal/smrcoll/cowsnapshot_test.go`

**Interfaces:**
- Produces: `CowBook.FreeHead uint32`, `CowBook.Cancel`, `CowBook.Fill`, `CowRoot.FreeHead uint32`

`CowBook` is the chunked copy-on-write twin: pool and ladder live in chunks behind a chunk table, `Capture()` clones the chunk-ref tables and bumps a generation so the writer copies a chunk before its first write. Its ops must stay in lockstep with `Book` — they are compared byte-for-byte.

**Critical accessor discipline:** `repairBest` must read through `LevelAt`, never `levelMut`. `levelMut` triggers a copy-on-write of a chunk; a rescan is a read, and routing it through the mutable accessor would copy untouched chunks and corrupt the measurement this store exists to produce.

- [ ] **Step 1: Write the failing tests**

Append to `go/internal/smrcoll/cowbook_test.go`:

```go
func TestCowCancelMatchesBookCancel(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 500, Chunk: 512, OtrBps: 100}
	b := NewBook(c)
	cb := NewCowBook(c)
	r1, r2 := NewSplitMix(SmrSeed), NewSplitMix(SmrSeed)
	for i := 0; i < 500; i++ {
		a := NextInsert(r1, i, c.Levels, c.Tick, c.PriceMin)
		x := NextInsert(r2, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(a.OrderID, a.Price, a.Qty, a.Side)
		cb.Insert(x.OrderID, x.Price, x.Qty, x.Side)
	}
	for id := int64(1); id <= 500; id += 3 {
		b.Cancel(id)
		cb.Cancel(id)
	}
	if cb.FreeHead != b.FreeHead || cb.Hwm != b.Hwm {
		t.Fatalf("freeHead %d/%d hwm %d/%d", cb.FreeHead, b.FreeHead, cb.Hwm, b.Hwm)
	}
	if cb.BestBid != b.BestBid || cb.BestAsk != b.BestAsk {
		t.Fatalf("best bid %d/%d ask %d/%d", cb.BestBid, b.BestBid, cb.BestAsk, b.BestAsk)
	}
	for tk := uint32(0); tk < c.Levels; tk++ {
		if cb.LevelQty(0, tk) != b.LevelQty(0, tk) || cb.LevelQty(1, tk) != b.LevelQty(1, tk) {
			t.Fatalf("level %d diverged", tk)
		}
	}
}

func TestCaptureCarriesFreeHead(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 100, Chunk: 512, OtrBps: 100}
	cb := NewCowBook(c)
	cb.Insert(1, 5, 10, 0)
	cb.Insert(2, 5, 10, 0)
	cb.Cancel(1)
	if root := cb.Capture(); root.FreeHead != cb.FreeHead {
		t.Fatalf("root FreeHead = %d, want %d", root.FreeHead, cb.FreeHead)
	}
}
```

Append to `go/internal/smrcoll/cowsnapshot_test.go`:

```go
func TestCowCancelImageMatchesFlatImage(t *testing.T) {
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Chunk: 512, OtrBps: 100}
	b := NewBook(c)
	cb := NewCowBook(c)
	r1, r2 := NewSplitMix(SmrSeed), NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		a := NextInsert(r1, i, c.Levels, c.Tick, c.PriceMin)
		x := NextInsert(r2, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(a.OrderID, a.Price, a.Qty, a.Side)
		cb.Insert(x.OrderID, x.Price, x.Qty, x.Side)
	}
	for id := int64(1); id <= int64(c.Steady); id += 3 {
		b.Cancel(id)
		cb.Cancel(id)
	}
	flat := append([]byte(nil), NewSnapshotter().Encode(b)...)
	cow := NewSnapshotter().EncodeRoot(cb.Capture())
	if !bytes.Equal(flat, cow) {
		t.Fatalf("CoW image differs from flat: %d vs %d bytes", len(cow), len(flat))
	}
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd go && go test ./internal/smrcoll/ -run 'CowCancel|CaptureCarries' -v
```

Expected: compile failure — `cb.Cancel undefined`, `CowRoot.FreeHead undefined`.

- [ ] **Step 3: Implement the store changes**

Add `FreeHead uint32` to both `CowRoot` and `CowBook`; initialise `FreeHead: NIL` in `NewCowBook`; copy it in `Capture` (`FreeHead: b.FreeHead,`).

Replace `Insert`'s `slot := b.Hwm` / `b.Hwm++` with `slot := b.allocSlot()`, and add:

```go
func (b *CowBook) allocSlot() uint32 {
	if b.FreeHead != NIL {
		slot := b.FreeHead
		b.FreeHead = b.OrderAt(slot).Next
		return slot
	}
	if int(b.Hwm) == b.capacity {
		panic(fmt.Sprintf("order pool exhausted: SMRC_CAP=%d reached", b.capacity))
	}
	slot := b.Hwm
	b.Hwm++
	return slot
}

func (b *CowBook) freeSlot(slot uint32) {
	head := b.FreeHead
	o := b.orderMut(slot)
	o.OrderID = 0
	o.Next = head
	o.Prev = NIL
	b.FreeHead = slot
}

func (b *CowBook) unlink(slot uint32, side uint8, t uint32, rem int64) {
	prev, next := b.OrderAt(slot).Prev, b.OrderAt(slot).Next
	if prev != NIL {
		b.orderMut(prev).Next = next
	}
	if next != NIL {
		b.orderMut(next).Prev = prev
	}
	lvl := b.levelMut(side, t)
	if lvl.Head == slot {
		lvl.Head = next
	}
	if lvl.Tail == slot {
		lvl.Tail = prev
	}
	lvl.QtyTotal -= rem
	lvl.Count--
}

// repairBest reads through LevelAt, never levelMut — a rescan must not
// trigger copy-on-write of untouched chunks.
func (b *CowBook) repairBest(side uint8, t uint32) {
	if side == 0 {
		if b.BestBid != int32(t) || b.LevelAt(0, t).Head != NIL {
			return
		}
		nb := int32(-1)
		for i := int(t); i >= 0; i-- {
			if b.LevelAt(0, uint32(i)).Head != NIL {
				nb = int32(i)
				break
			}
		}
		b.BestBid = nb
		return
	}
	if b.BestAsk != int32(t) || b.LevelAt(1, t).Head != NIL {
		return
	}
	na := int32(-1)
	for i := int(t); i < int(b.NLevels); i++ {
		if b.LevelAt(1, uint32(i)).Head != NIL {
			na = int32(i)
			break
		}
	}
	b.BestAsk = na
}

// Cancel — same op semantics as Book.Cancel (keep in lockstep).
func (b *CowBook) Cancel(orderID int64) {
	slot := b.ids.get(orderID)
	o := b.OrderAt(slot)
	rem := o.Qty - o.Filled
	side, price := o.Side, o.Price
	t := b.tickOf(price)
	b.ids.del(orderID)
	b.unlink(slot, side, t, rem)
	b.freeSlot(slot)
	b.repairBest(side, t)
}

// Fill — same op semantics as Book.Fill (keep in lockstep).
func (b *CowBook) Fill(orderID int64) {
	slot := b.ids.get(orderID)
	o := b.orderMut(slot)
	rem := o.Qty - o.Filled
	o.Filled = o.Qty
	side, price := o.Side, o.Price
	t := b.tickOf(price)
	b.ids.del(orderID)
	b.unlink(slot, side, t, rem)
	b.freeSlot(slot)
	b.repairBest(side, t)
}
```

Add `"fmt"` to the imports. Note the field is the unexported `b.capacity` (an `int`) — `Capacity` with a capital C is a `uint32` field on `CowRoot`, not on `CowBook`.

Also make `CowBook.rebuildIDs` skip freed slots, exactly as `Book.rebuildIDs` does:

```go
		if b.OrderAt(slot).OrderID != 0 {
			b.ids.put(b.OrderAt(slot).OrderID, slot)
		}
```

- [ ] **Step 4: Mirror the codec changes**

In `cowsnapshot.go`, replace the placeholder `msg.FreeHead = NIL` with `msg.FreeHead = r.FreeHead`. In `RestoreCow`, add the same schema-version gate, capacity check and `b.FreeHead = msg.FreeHead` assignment that Task 4 added to `Restore` — read `snapshot.go` and mirror it, since the two must stay byte-identical for the same logical state.

- [ ] **Step 5: Run the suite**

```sh
cd go && go test ./internal/smrcoll/
```

Expected: PASS, including the pre-existing `TestCowBookMatchesGoldenBytes`.

- [ ] **Step 6: Commit**

```sh
git add go/internal/smrcoll/cowbook.go go/internal/smrcoll/cowsnapshot.go \
        go/internal/smrcoll/cowbook_test.go go/internal/smrcoll/cowsnapshot_test.go
git commit -m "feat(smrcoll,go): CowBook Cancel/Fill + FreeHead in CowRoot

CowRoot carries FreeHead so a restored replica reproduces allocation order.
The ladder rescan reads through LevelAt, never levelMut, so it does not
trigger copy-on-write of untouched chunks."
```

---

### Task 6: The churn driver and the cross-language churn golden

**Files:**
- Create: `go/internal/smrcoll/churn.go`, `go/internal/smrcoll/churn_test.go`

**Interfaces:**
- Produces:
  - `type ChurnStore interface { Insert(orderID, price, qty int64, side uint8); Cancel(orderID int64); Fill(orderID int64) }`
  - `type ChurnOpKind uint8` with `ChurnInsert`, `ChurnCancel`, `ChurnFill`
  - `type ChurnOp struct { Kind ChurnOpKind; OrderID, Price, Qty int64; Side uint8 }`
  - `func NewChurn(cfg bench.SmrConfig) *Churn`, `(*Churn).NextOp() ChurnOp`, `(*Churn).Prebuild(store ChurnStore, steady int)`, `func ApplyChurn(store ChurnStore, op ChurnOp)`
  - `type ChurnSamples struct { InsertNs, CancelNs, FillNs []int64 }`
  - `func RunChurn(cfg bench.SmrConfig, store ChurnStore, c *Churn) (ChurnSamples, int64)`
  - `func EmitChurn(experiment string, s ChurnSamples, rssGrowth int64)`

`Book` and `CowBook` satisfy `ChurnStore` structurally once Tasks 3 and 5 land — no explicit declaration needed.

The victim removal **must** be a swap-remove (move the last element into the victim's index, then shorten), because that is what Rust's `Vec::swap_remove` does and the two op streams must be identical.

- [ ] **Step 1: Write the failing tests**

Create `go/internal/smrcoll/churn_test.go`:

```go
package smrcoll

import (
	"bytes"
	"os"
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func churnCfg() bench.SmrConfig {
	return bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Chunk: 4096, OtrBps: 100}
}

func TestChurnOpStreamIsDeterministic(t *testing.T) {
	c := churnCfg()
	a, b := NewChurn(c), NewChurn(c)
	for k := 0; k < 10000; k++ {
		if a.NextOp() != b.NextOp() {
			t.Fatalf("op %d diverged", k)
		}
	}
}

func TestChurnAlternatesAndHonoursOTR(t *testing.T) {
	c := churnCfg()
	ch := NewChurn(c)
	store := NewBook(c)
	ch.Prebuild(store, c.Steady)
	ins, can, fil := 0, 0, 0
	for i := 0; i < 100000; i++ {
		switch ch.NextOp().Kind {
		case ChurnInsert:
			ins++
		case ChurnCancel:
			can++
		case ChurnFill:
			fil++
		}
	}
	if ins != 50000 || can+fil != 50000 {
		t.Fatalf("mix: %d inserts, %d departures", ins, can+fil)
	}
	if fil < 300 || fil > 800 {
		t.Fatalf("fills = %d, want ~500 (100 bps of 50k departures)", fil)
	}
}

func TestChurnHoldsLiveSetConstant(t *testing.T) {
	c := churnCfg()
	ch := NewChurn(c)
	store := NewBook(c)
	ch.Prebuild(store, c.Steady)
	for i := 0; i < 20000; i++ {
		ApplyChurn(store, ch.NextOp())
	}
	live := 0
	for slot := uint32(0); slot < store.Hwm; slot++ {
		if store.Pool[slot].OrderID != 0 {
			live++
		}
	}
	if live != c.Steady {
		t.Fatalf("live set = %d, want %d", live, c.Steady)
	}
}

func TestChurnSnapshotRestoreReplayIsBitIdentical(t *testing.T) {
	c := churnCfg()
	ch := NewChurn(c)
	hot := NewBook(c)
	ch.Prebuild(hot, c.Steady)
	for i := 0; i < 5000; i++ {
		ApplyChurn(hot, ch.NextOp())
	}
	img := append([]byte(nil), NewSnapshotter().Encode(hot)...)
	cold, err := Restore(img, c)
	if err != nil {
		t.Fatalf("restore: %v", err)
	}
	ops := make([]ChurnOp, 5000)
	for i := range ops {
		ops[i] = ch.NextOp()
	}
	for _, op := range ops {
		ApplyChurn(hot, op)
		ApplyChurn(cold, op)
	}
	a := append([]byte(nil), NewSnapshotter().Encode(hot)...)
	b := NewSnapshotter().Encode(cold)
	if !bytes.Equal(a, b) {
		t.Fatal("restored replica diverged from the never-restarted one")
	}
}

// The cross-language check for the churn path: Go must reproduce the image
// Rust exported, byte for byte, from the identical op stream.
func TestCrossLanguageChurnGoldenBytes(t *testing.T) {
	golden, err := os.ReadFile("../../../rust/smr-collections/testdata/golden_churn_snapshot.bin")
	if err != nil {
		t.Fatalf("read churn golden: %v", err)
	}
	c := churnCfg()
	b := NewBook(c)
	ch := NewChurn(c)
	ch.Prebuild(b, c.Steady)
	for i := 0; i < 10000; i++ {
		ApplyChurn(b, ch.NextOp())
	}
	got := NewSnapshotter().Encode(b)
	if !bytes.Equal(got, golden) {
		t.Fatalf("go churn bytes differ from rust golden (len go=%d rust=%d)", len(got), len(golden))
	}
}
```

- [ ] **Step 2: Run tests to verify they fail**

```sh
cd go && go test ./internal/smrcoll/ -run Churn -v
```

Expected: compile failure — `NewChurn`, `ChurnOp` and friends do not exist.

- [ ] **Step 3: Implement**

Create `go/internal/smrcoll/churn.go`:

```go
// The churn workload: a deterministic insert/cancel/fill stream at a
// configurable order-to-trade ratio (default 1 %, the real-exchange figure).
//
// Op generation sits outside the timed region — the driver produces an op,
// the caller times only the store's application of it, so the per-op numbers
// are store work alone. Note this makes them NOT directly comparable with the
// older insert/update cells, which time their own generation; see the design
// spec's "Must be recorded in the next run's journal entry".
package smrcoll

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

// ChurnStore is the store surface a churn stream drives. Book and CowBook
// both satisfy it structurally.
type ChurnStore interface {
	Insert(orderID, price, qty int64, side uint8)
	Cancel(orderID int64)
	Fill(orderID int64)
}

type ChurnOpKind uint8

const (
	ChurnInsert ChurnOpKind = iota
	ChurnCancel
	ChurnFill
)

type ChurnOp struct {
	Kind                ChurnOpKind
	OrderID, Price, Qty int64
	Side                uint8
}

type Churn struct {
	rng *SplitMix
	// live holds the order IDs currently resting, dense so a victim is one
	// uniform draw.
	live []int64
	// i is the global op index: it drives both the insert/depart alternation
	// and the order ID, so IDs are sparse (1, 3, 5, …) but never reused.
	i        int
	otrBps   uint64
	levels   uint32
	tick     int64
	priceMin int64
}

func NewChurn(cfg bench.SmrConfig) *Churn {
	return &Churn{
		rng:      NewSplitMix(SmrSeed),
		live:     make([]int64, 0, cfg.Cap),
		otrBps:   uint64(cfg.OtrBps),
		levels:   cfg.Levels,
		tick:     cfg.Tick,
		priceMin: cfg.PriceMin,
	}
}

func (c *Churn) insertOp() ChurnOp {
	ins := NextInsert(c.rng, c.i, c.levels, c.tick, c.priceMin)
	c.i++
	c.live = append(c.live, ins.OrderID)
	return ChurnOp{Kind: ChurnInsert, OrderID: ins.OrderID, Price: ins.Price, Qty: ins.Qty, Side: ins.Side}
}

// NextOp returns the next op. Even index inserts, odd index departs; a
// departure is a fill with probability otrBps/10000, otherwise a cancel.
func (c *Churn) NextOp() ChurnOp {
	if c.i%2 == 0 || len(c.live) == 0 {
		return c.insertOp()
	}
	c.i++
	v := int(c.rng.Next() % uint64(len(c.live)))
	id := c.live[v]
	isFill := c.rng.Next()%10000 < c.otrBps
	// swap-remove, matching Rust's Vec::swap_remove exactly — the two op
	// streams must be identical.
	c.live[v] = c.live[len(c.live)-1]
	c.live = c.live[:len(c.live)-1]
	if isFill {
		return ChurnOp{Kind: ChurnFill, OrderID: id}
	}
	return ChurnOp{Kind: ChurnCancel, OrderID: id}
}

// Prebuild brings the store to its steady-state live set with inserts only.
func (c *Churn) Prebuild(store ChurnStore, steady int) {
	for i := 0; i < steady; i++ {
		ApplyChurn(store, c.insertOp())
	}
}

func ApplyChurn(store ChurnStore, op ChurnOp) {
	switch op.Kind {
	case ChurnInsert:
		store.Insert(op.OrderID, op.Price, op.Qty, op.Side)
	case ChurnCancel:
		store.Cancel(op.OrderID)
	case ChurnFill:
		store.Fill(op.OrderID)
	}
}

type ChurnSamples struct {
	InsertNs, CancelNs, FillNs []int64
}

// RunChurn warms up, then times cfg.Iters ops into per-op-type sample slices.
// Only the store call is inside the clock. Returns the samples and the RSS
// baseline taken at the clock boundary — after warmup and after the sample
// slices are allocated, so neither is counted as store growth.
func RunChurn(cfg bench.SmrConfig, store ChurnStore, c *Churn) (ChurnSamples, int64) {
	for i := 0; i < cfg.Warmup; i++ {
		ApplyChurn(store, c.NextOp())
	}
	half := cfg.Iters/2 + 1
	s := ChurnSamples{
		InsertNs: make([]int64, half),
		CancelNs: make([]int64, half),
		FillNs:   make([]int64, half),
	}
	// make() zeroes, so the pages are already resident; reslice to empty and
	// keep the capacity so the timed loop never allocates.
	s.InsertNs, s.CancelNs, s.FillNs = s.InsertNs[:0], s.CancelNs[:0], s.FillNs[:0]
	rss0 := bench.RSSBytes()
	for i := 0; i < cfg.Iters; i++ {
		op := c.NextOp()
		t0 := time.Now()
		ApplyChurn(store, op)
		ns := time.Since(t0).Nanoseconds()
		switch op.Kind {
		case ChurnInsert:
			s.InsertNs = append(s.InsertNs, ns)
		case ChurnCancel:
			s.CancelNs = append(s.CancelNs, ns)
		case ChurnFill:
			s.FillNs = append(s.FillNs, ns)
		}
	}
	return s, rss0
}

// EmitChurn emits the per-op-type distributions plus RSS growth. A
// distribution with no samples is skipped rather than emitted as zeros — at
// SMRC_OTR_BPS=0 there are no fills, and a fabricated zero would read as a
// real measurement.
func EmitChurn(experiment string, s ChurnSamples, rssGrowth int64) {
	if len(s.InsertNs) > 0 {
		bench.EmitSmrLatency(experiment, "insert", s.InsertNs)
	}
	if len(s.CancelNs) > 0 {
		bench.EmitSmrLatency(experiment, "cancel", s.CancelNs)
	}
	if len(s.FillNs) > 0 {
		bench.EmitSmrLatency(experiment, "fill", s.FillNs)
	}
	bench.EmitSmrInt(experiment, "rss_growth_bytes", rssGrowth, "bytes", 1)
}
```

(`(*SplitMix).Next() uint64` and `NewSplitMix(seed uint64)` are confirmed to exist at `go/internal/smrcoll/rng.go:9,11`.)

- [ ] **Step 4: Run the suite**

```sh
cd go && go test ./internal/smrcoll/
```

Expected: PASS. `TestCrossLanguageChurnGoldenBytes` is the one that matters most — it proves Go's cancel path, free list, id-map deletion and encoder all agree with Rust's, byte for byte, over 12,000 ops. If it fails, **do not regenerate the golden**: the divergence is in this Go code, and the failure message's length difference is the first clue (a length match with differing bytes points at op semantics; a length mismatch points at the free list or the level set).

- [ ] **Step 5: Commit**

```sh
git add go/internal/smrcoll/churn.go go/internal/smrcoll/churn_test.go
git commit -m "feat(smrcoll,go): churn workload driver at ~1% OTR

Alternating insert/depart stream over a dense live slice with uniform victim
selection and swap-remove, matching Rust's Vec::swap_remove so the two op
streams are identical. Verified against the cross-language churn golden."
```

---

### Task 7: The four Go cells

**Files:**
- Create: `go/cmd/smr-collections-churn/main.go`
- Create: `go/cmd/smr-collections-mvcc_churn/main.go`
- Create: `go/cmd/smr-collections-live_stw_churn/main.go`
- Create: `go/cmd/smr-collections-live_mvcc_churn/main.go`

**Interfaces:**
- Consumes: everything from Tasks 1–6

- [ ] **Step 1: Write `churn/main.go`**

```go
// smr-collections-churn (Go): insert/cancel/fill at a real-exchange
// order-to-trade ratio against the flat stop-the-world book. Cancels recycle
// slots through the free list, so this is the steady state a matching engine
// actually lives in.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "churn"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	churn := smrcoll.NewChurn(cfg)
	churn.Prebuild(book, cfg.Steady)
	samples, rss0 := smrcoll.RunChurn(cfg, book, churn)
	rss1 := bench.RSSBytes()
	growth := rss1 - rss0
	if growth < 0 {
		growth = 0
	}
	smrcoll.EmitChurn(experiment, samples, growth)
}
```

- [ ] **Step 2: Write `mvcc_churn/main.go`**

Identical apart from the store and the experiment name:

```go
// smr-collections-mvcc_churn (Go): the churn workload against the chunked
// copy-on-write book. Cancels scatter writes across chunks rather than
// appending to the newest one, so this is where CoW's first-touch copy cost
// is exercised hardest.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "mvcc_churn"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewCowBook(cfg)
	churn := smrcoll.NewChurn(cfg)
	churn.Prebuild(book, cfg.Steady)
	samples, rss0 := smrcoll.RunChurn(cfg, book, churn)
	rss1 := bench.RSSBytes()
	growth := rss1 - rss0
	if growth < 0 {
		growth = 0
	}
	smrcoll.EmitChurn(experiment, samples, growth)
}
```

- [ ] **Step 3: Write `live_stw_churn/main.go`**

This mirrors `go/cmd/smr-collections-live_stw/main.go` with the churn stream in place of the update-only one, plus the per-op-type split and `rss_peak_bytes`.

**`bench.RSSBytes()` must be called only after the iteration's elapsed time is computed, never inside the timed region.** It reads `/proc/self/statm`, costing microseconds against sub-microsecond ops; sampling it inside inflates `writer_max`, the headline metric this cell exists to report. This exact defect was found and fixed on the Rust side.

```go
// smr-collections-live_stw_churn (Go): writer-observed latency under the
// churn workload while stop-the-world snapshots run inline at a fixed op
// cadence (the trigger op pays the whole serialize; writer_max is the stall).
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_stw_churn"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	churn := smrcoll.NewChurn(cfg)
	churn.Prebuild(book, cfg.Steady)
	for i := 0; i < cfg.Warmup; i++ {
		smrcoll.ApplyChurn(book, churn.NextOp())
	}
	s := smrcoll.NewSnapshotter()
	// warm the encode path + buffer pages so the k=0 trigger measures
	// steady-state stall, not first-touch cost
	s.Encode(book)

	writerNs := make([]int64, cfg.LiveIters)
	snapNs := make([]int64, 0, cfg.LiveIters/cfg.SnapEvery+1)
	var snapLen int
	var ins, can, fil []int64
	rssPeak := bench.RSSBytes()
	for k := 0; k < cfg.LiveIters; k++ {
		op := churn.NextOp()
		fired := k%cfg.SnapEvery == 0
		t0 := time.Now()
		if fired {
			img := s.Encode(book)
			snapLen = len(img)
			snapNs = append(snapNs, time.Since(t0).Nanoseconds())
		}
		smrcoll.ApplyChurn(book, op)
		ns := time.Since(t0).Nanoseconds()
		// Sample RSS only AFTER the clock closes: RSSBytes reads
		// /proc/self/statm — microseconds against sub-microsecond ops — so
		// calling it inside the timed region would inflate writer_max, the one
		// metric this cell exists to report precisely.
		if fired {
			if r := bench.RSSBytes(); r > rssPeak {
				rssPeak = r
			}
		}
		writerNs[k] = ns
		switch op.Kind {
		case smrcoll.ChurnInsert:
			ins = append(ins, ns)
		case smrcoll.ChurnCancel:
			can = append(can, ns)
		case smrcoll.ChurnFill:
			fil = append(fil, ns)
		}
	}
	bench.EmitSmrLive(experiment, writerNs, snapNs, 0, int64(snapLen))
	if len(ins) > 0 {
		bench.EmitSmrLatency(experiment, "insert", ins)
	}
	if len(can) > 0 {
		bench.EmitSmrLatency(experiment, "cancel", can)
	}
	if len(fil) > 0 {
		bench.EmitSmrLatency(experiment, "fill", fil)
	}
	bench.EmitSmrInt(experiment, "rss_peak_bytes", rssPeak, "bytes", 1)
}
```

- [ ] **Step 4: Write `live_mvcc_churn/main.go`**

Read `go/cmd/smr-collections-live_mvcc/main.go` first and follow exactly what it does for the capture/serializer handoff and the `skipped` counter — this cell must differ from it only in the workload. Then apply the same three additions Step 3 made: the churn stream, the per-op-type split, and `rss_peak_bytes` sampled outside the clock. Use `smrcoll.NewCowBook`, `book.Capture()` and `s.EncodeRoot(root)` in place of the flat store's inline `s.Encode(book)`, and set `experiment = "live_mvcc_churn"`.

- [ ] **Step 5: Build, vet and smoke-run all four**

```sh
cd go && go build ./... && go vet ./... && go test ./...
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_WARMUP=1000 SMRC_ITERS=20000 go run ./cmd/smr-collections-churn
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_WARMUP=1000 SMRC_ITERS=20000 go run ./cmd/smr-collections-mvcc_churn
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_LIVE_ITERS=20000 SMRC_SNAP_EVERY=5000 go run ./cmd/smr-collections-live_stw_churn
SMRC_CAP=65536 SMRC_STEADY=8000 SMRC_LIVE_ITERS=20000 SMRC_SNAP_EVERY=5000 go run ./cmd/smr-collections-live_mvcc_churn
```

Expected: each prints result-contract JSON lines on stdout carrying `"focus_area":"smr-collections"`, `"language":"go"`, the right `experiment`, and metrics `insert_*`, `cancel_*`, `fill_*`, `rss_growth_bytes` (plus `writer_*`, `snapshot_*`, `rss_peak_bytes` for the live cells). **Nothing but result lines on stdout.**

**These are local fitness checks, not results. Do not journal them.**

- [ ] **Step 6: Commit**

```sh
git add go/cmd/smr-collections-churn go/cmd/smr-collections-mvcc_churn \
        go/cmd/smr-collections-live_stw_churn go/cmd/smr-collections-live_mvcc_churn
git commit -m "feat(smrcoll,go): go churn cells — churn, mvcc_churn, live_{stw,mvcc}_churn

Four cells emitting per-op-type distributions (insert/cancel/fill) plus
rss_growth_bytes; the live pair adds writer_max and rss_peak_bytes while a
snapshot is in flight."
```

---

## Plan Self-Review

**Spec coverage.** Op stream → Task 6. `Book` → Task 3. `CowBook` → Task 5. Snapshot v2 restore validation → Tasks 4 and 5 (the schema itself landed in plan 1). Metrics → Tasks 6 and 7. Config → Task 1. Error handling → Tasks 1, 3, 4 (capacity panic, version/capacity rejection). Testing item 1 (bit-identical resumption) → Task 6; item 2's cross-language half → Task 6's golden test plus the pre-existing plain golden test; items 3–5 → Tasks 2, 3, 5. `UltimaBook` and the canonical digest are Rust-only by design. Infra rows and `CLAUDE.md` are plan 3.

**Three things this plan deliberately inherits rather than re-derives.** The SBE schema, both golden files, and Go's regenerated v2 codec all landed in plan 1 — this plan only replaces the `freeHead = NIL` placeholders with real values. If a golden test fails here, the bug is in this plan's Go code, never in the golden.

**Both API assumptions were checked against the source before publishing, and one was wrong.** `(*SplitMix).Next() uint64` exists as assumed (`rng.go:9,11`). `CowBook`'s capacity field is the unexported `b.capacity` (`int`), not `b.Capacity` — the capitalised name belongs to `CowRoot` and is a `uint32`. Task 5's code is corrected; this is exactly the class of mistake that blocked a task on the Rust plan (`insert_with_id` cited from the wrong impl block), so it was worth the two greps.

**Where the risk is concentrated.** Task 2 (`idMap.del`) is the only algorithm here without a Rust counterpart to transliterate from, and a subtle bug in backward-shift compaction produces *wrong lookups* rather than crashes — which would surface as a golden mismatch three tasks later. Its long-churn test exists to catch that early. Task 6's cross-language golden is the backstop: it exercises the whole Go stack against bytes Rust produced.
