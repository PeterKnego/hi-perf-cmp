package smrcoll

import (
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func cowCfg() bench.SmrConfig {
	return bench.SmrConfig{
		Cap: 1024, Levels: 300, Tick: 1, PriceMin: 0,
		Steady: 500, Warmup: 0, Iters: 0,
		Chunk: 64, LiveIters: 200000, SnapEvery: 20000,
	}
}

func TestCowBookMatchesBookQueries(t *testing.T) {
	c := cowCfg()
	b := NewBook(c)
	cb := NewCowBook(c)
	r1 := NewSplitMix(SmrSeed)
	r2 := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		a := NextInsert(r1, i, c.Levels, c.Tick, c.PriceMin)
		x := NextInsert(r2, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(a.OrderID, a.Price, a.Qty, a.Side)
		cb.Insert(x.OrderID, x.Price, x.Qty, x.Side)
	}
	for i := 0; i < 1000; i++ {
		a := NextUpdate(r1, c.Steady)
		x := NextUpdate(r2, c.Steady)
		b.Update(a.OrderID, a.FillQty)
		cb.Update(x.OrderID, x.FillQty)
	}
	if cb.Hwm != b.Hwm || cb.BestBid != b.BestBid || cb.BestAsk != b.BestAsk {
		t.Fatalf("scalars diverge")
	}
	for id := int64(1); id <= int64(c.Steady); id++ {
		if cb.GetSlot(id) != b.GetSlot(id) {
			t.Fatalf("slot diverges for id %d", id)
		}
	}
	for tick := uint32(0); tick < c.Levels; tick++ {
		if cb.LevelQty(0, tick) != b.LevelQty(0, tick) || cb.LevelQty(1, tick) != b.LevelQty(1, tick) {
			t.Fatalf("level qty diverges at tick %d", tick)
		}
	}
	for slot := uint32(0); slot < cb.Hwm; slot++ {
		if *cb.OrderAt(slot) != b.Pool[slot] {
			t.Fatalf("order diverges at slot %d", slot)
		}
	}
}

func TestCaptureIsolatesRootFromLaterWrites(t *testing.T) {
	c := cowCfg()
	cb := NewCowBook(c)
	for i := 0; i < c.Steady; i++ {
		cb.Insert(int64(i)+1, int64(i%int(c.Levels)), 10, uint8(i%2))
	}
	root := cb.Capture()
	before := root.OrderAt(5).Filled
	cb.Update(6, 7) // order 6 lives in slot 5
	if root.OrderAt(5).Filled != before {
		t.Fatal("root saw a post-capture write")
	}
	if cb.OrderAt(5).Filled != before+7 {
		t.Fatal("writer did not advance")
	}
}

func TestSuccessiveCaptures(t *testing.T) {
	c := cowCfg()
	cb := NewCowBook(c)
	cb.Insert(1, 5, 10, 0)
	r1 := cb.Capture()
	cb.Update(1, 4)
	r2 := cb.Capture()
	if r1.OrderAt(0).Filled != 0 || r2.OrderAt(0).Filled != 4 {
		t.Fatalf("capture generations wrong: %d %d", r1.OrderAt(0).Filled, r2.OrderAt(0).Filled)
	}
}

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
	// Walk the full free chain, not just the head: a bug that swapped a
	// second-from-head link while leaving the head correct would otherwise
	// pass this test.
	var gotChain, wantChain []uint32
	for slot := cb.FreeHead; slot != NIL; slot = cb.OrderAt(slot).Next {
		gotChain = append(gotChain, slot)
	}
	for slot := b.FreeHead; slot != NIL; slot = b.Pool[slot].Next {
		wantChain = append(wantChain, slot)
	}
	if len(gotChain) != len(wantChain) {
		t.Fatalf("free chain length %d, want %d", len(gotChain), len(wantChain))
	}
	for i := range wantChain {
		if gotChain[i] != wantChain[i] {
			t.Fatalf("free chain[%d] = %d, want %d", i, gotChain[i], wantChain[i])
		}
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
