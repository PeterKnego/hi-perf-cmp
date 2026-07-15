package smrcoll

import (
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func cfg() bench.SmrConfig {
	return bench.SmrConfig{Cap: 1024, Levels: 16, Tick: 1, PriceMin: 0, Steady: 100, Warmup: 0, Iters: 0}
}

func TestInsertPlacesOrder(t *testing.T) {
	b := NewBook(cfg())
	b.Insert(1, 5, 10, 0)
	b.Insert(2, 5, 7, 0)
	b.Insert(3, 8, 3, 1)
	if b.LevelQty(0, 5) != 17 {
		t.Fatalf("bid level qty = %d, want 17", b.LevelQty(0, 5))
	}
	if b.LevelQty(1, 8) != 3 {
		t.Fatalf("ask level qty = %d, want 3", b.LevelQty(1, 8))
	}
	if b.BestBidTick() != 5 || b.BestAskTick() != 8 {
		t.Fatalf("best = %d/%d, want 5/8", b.BestBidTick(), b.BestAskTick())
	}
	if b.GetSlot(2) != 1 {
		t.Fatalf("slot of id 2 = %d, want 1", b.GetSlot(2))
	}
}

func TestUpdateCapsFill(t *testing.T) {
	b := NewBook(cfg())
	b.Insert(1, 5, 10, 0)
	b.Update(1, 4)
	if b.LevelQty(0, 5) != 6 {
		t.Fatalf("after fill 4 qty = %d, want 6", b.LevelQty(0, 5))
	}
	b.Update(1, 100)
	if b.LevelQty(0, 5) != 0 {
		t.Fatalf("after over-fill qty = %d, want 0", b.LevelQty(0, 5))
	}
}
