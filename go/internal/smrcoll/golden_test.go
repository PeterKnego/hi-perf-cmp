package smrcoll

import (
	"bytes"
	"os"
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func TestCrossLanguageGoldenBytes(t *testing.T) {
	golden, err := os.ReadFile("../../../rust/smr-collections/testdata/golden_snapshot.bin")
	if err != nil {
		t.Fatalf("read golden (run the Rust R4 export first): %v", err)
	}
	c := bench.SmrConfig{Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0, Steady: 2000, Warmup: 0, Iters: 0}
	s := NewSnapshotter()
	got := s.Encode(buildBook(c, c.Steady))
	if !bytes.Equal(got, golden) {
		t.Fatalf("go snapshot bytes differ from rust golden (len go=%d rust=%d)", len(got), len(golden))
	}
}
