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
