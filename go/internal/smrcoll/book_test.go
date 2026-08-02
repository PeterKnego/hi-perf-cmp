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

func TestIDMapDeleteCompactsWrappedProbeRun(t *testing.T) {
	const n = 8
	m := newIDMap(4) // n = 8 slots
	// Keys congruent mod n share an ideal slot. Slot 6 puts the run across
	// the table's wrap point, which is the case most likely to be wrong.
	var base int64
	for k := int64(1); k < 10000; k++ {
		if (uint64(k)*0x9E3779B97F4A7C15)&(n-1) == 6 {
			base = k
			break
		}
	}
	if base == 0 {
		t.Fatal("no key hashes to slot 6")
	}
	keys := []int64{base, base + n, base + 2*n, base + 3*n, base + 4*n}
	for _, k := range keys {
		if (uint64(k)*0x9E3779B97F4A7C15)&(n-1) != 6 {
			t.Fatalf("key %d does not share the ideal slot — test is not exercising a chain", k)
		}
	}
	for i, k := range keys {
		m.put(k, uint32(i))
	}
	m.del(keys[0]) // head of the run
	m.del(keys[2]) // middle of the run
	for i, k := range keys {
		got := m.get(k)
		if i == 0 || i == 2 {
			if got != NIL {
				t.Fatalf("deleted key %d still resolves to %d", k, got)
			}
		} else if got != uint32(i) {
			t.Fatalf("survivor %d: got %d, want %d", k, got, i)
		}
	}
}

func TestIDMapMatchesReferenceUnderCollisions(t *testing.T) {
	const slots = 128
	m := newIDMap(slots / 2) // n = 128
	ref := make(map[int64]uint32)
	var live []int64
	rng := NewSplitMix(SmrSeed)
	for step := 0; step < 20000; step++ {
		// Four residue classes mod 128 -> four ideal slots -> long probe runs.
		k := int64(1) + int64(step%4) + int64(step/4)*slots
		m.put(k, uint32(step))
		ref[k] = uint32(step)
		live = append(live, k)
		if len(live) > 40 {
			v := int(rng.Next() % uint64(len(live)))
			victim := live[v]
			live[v] = live[len(live)-1]
			live = live[:len(live)-1]
			m.del(victim)
			delete(ref, victim)
		}
		if step%97 == 0 {
			for kk, vv := range ref {
				if got := m.get(kk); got != vv {
					t.Fatalf("step %d: key %d got %d, want %d", step, kk, got, vv)
				}
			}
		}
	}
	for kk, vv := range ref {
		if got := m.get(kk); got != vv {
			t.Fatalf("final: key %d got %d, want %d", kk, got, vv)
		}
	}
}
