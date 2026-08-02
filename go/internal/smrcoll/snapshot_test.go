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
