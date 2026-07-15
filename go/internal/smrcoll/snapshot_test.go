package smrcoll

import (
	"bytes"
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func buildBook(c bench.SmrConfig, n int) *Book {
	b := NewBook(c)
	rng := NewSplitMix(SmrSeed)
	for i := 0; i < n; i++ {
		ins := NextInsert(rng, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	return b
}

func snapCfg() bench.SmrConfig {
	return bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Warmup: 0, Iters: 0}
}

func TestSnapshotRoundTrip(t *testing.T) {
	c := snapCfg()
	b := buildBook(c, c.Steady)
	s := NewSnapshotter()
	img := append([]byte(nil), s.Encode(b)...) // copy: buffer is reused
	r, err := Restore(img, c)
	if err != nil {
		t.Fatalf("restore: %v", err)
	}
	if r.BestBidTick() != b.BestBidTick() || r.BestAskTick() != b.BestAskTick() || r.HwmVal() != b.HwmVal() {
		t.Fatalf("header mismatch after restore")
	}
	for id := int64(1); id <= int64(c.Steady); id++ {
		if r.GetSlot(id) != b.GetSlot(id) {
			t.Fatalf("slot mismatch for id %d", id)
		}
	}
	for tk := uint32(0); tk < c.Levels; tk++ {
		if r.LevelQty(0, tk) != b.LevelQty(0, tk) || r.LevelQty(1, tk) != b.LevelQty(1, tk) {
			t.Fatalf("level qty mismatch at tick %d", tk)
		}
	}
}

func TestSnapshotDeterministic(t *testing.T) {
	c := snapCfg()
	s := NewSnapshotter()
	a := append([]byte(nil), s.Encode(buildBook(c, c.Steady))...)
	b := append([]byte(nil), s.Encode(buildBook(c, c.Steady))...)
	if !bytes.Equal(a, b) {
		t.Fatalf("same ops => bytes must be identical")
	}
}
