package smrcoll

import (
	"bytes"
	"os"
	"testing"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

func cowGoldenCfg() bench.SmrConfig {
	return bench.SmrConfig{
		Cap: 4096, Levels: 64, Tick: 1, PriceMin: 0,
		Steady: 2000, Warmup: 0, Iters: 0,
		Chunk: 512, LiveIters: 200000, SnapEvery: 20000,
	}
}

func buildCow(c bench.SmrConfig, n int) *CowBook {
	b := NewCowBook(c)
	rng := NewSplitMix(SmrSeed)
	for i := 0; i < n; i++ {
		ins := NextInsert(rng, i, c.Levels, c.Tick, c.PriceMin)
		b.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	return b
}

func TestCowBookMatchesGoldenBytes(t *testing.T) {
	c := cowGoldenCfg()
	cb := buildCow(c, c.Steady)
	root := cb.Capture()
	got := NewSnapshotter().EncodeRoot(root)
	want, err := os.ReadFile("../../../rust/smr-collections/testdata/golden_snapshot.bin")
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Fatalf("CowBook bytes differ from golden: got %d bytes, want %d", len(got), len(want))
	}
}

func TestCowEncodeEqualsStwEncodeAfterMixedOps(t *testing.T) {
	c := cowGoldenCfg()
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
	for i := 0; i < 500; i++ {
		a := NextUpdate(r1, c.Steady)
		x := NextUpdate(r2, c.Steady)
		b.Update(a.OrderID, a.FillQty)
		cb.Update(x.OrderID, x.FillQty)
	}
	stw := NewSnapshotter().Encode(b)
	cow := NewSnapshotter().EncodeRoot(cb.Capture())
	if !bytes.Equal(stw, cow) {
		t.Fatal("CoW bytes differ from STW bytes for identical state")
	}
}

func TestRestoreCowRoundTripAndCorruption(t *testing.T) {
	c := cowGoldenCfg()
	cb := buildCow(c, c.Steady)
	img := append([]byte(nil), NewSnapshotter().EncodeRoot(cb.Capture())...)
	r, err := RestoreCow(img, c)
	if err != nil {
		t.Fatal(err)
	}
	again := NewSnapshotter().EncodeRoot(r.Capture())
	if !bytes.Equal(img, again) {
		t.Fatal("restore does not round-trip")
	}
	bad := append([]byte(nil), img...)
	bad[0] ^= 0xFF
	if _, err := RestoreCow(bad, c); err == nil {
		t.Fatal("corrupt image accepted")
	}
}

// The concurrency correctness test (run under -race): capture at update k
// while the writer keeps going; the concurrently-encoded bytes must equal a
// single-threaded STW encode of a Book replayed to exactly k updates.
func TestConcurrentCaptureEqualsStwReplay(t *testing.T) {
	c := cowGoldenCfg()
	const totalUpdates, captureAt = 2000, 700

	ref := NewBook(c)
	rr := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		ins := NextInsert(rr, i, c.Levels, c.Tick, c.PriceMin)
		ref.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	for i := 0; i < captureAt; i++ {
		up := NextUpdate(rr, c.Steady)
		ref.Update(up.OrderID, up.FillQty)
	}
	want := append([]byte(nil), NewSnapshotter().Encode(ref)...)

	cb := buildCow(c, c.Steady)
	rng := NewSplitMix(SmrSeed)
	for i := 0; i < c.Steady; i++ {
		_ = NextInsert(rng, i, c.Levels, c.Tick, c.PriceMin) // skip consumed draws
	}
	rootCh := make(chan *CowRoot, 1)
	gotCh := make(chan []byte, 1)
	go func() {
		root := <-rootCh
		gotCh <- append([]byte(nil), NewSnapshotter().EncodeRoot(root)...)
	}()
	for k := 0; k < totalUpdates; k++ {
		if k == captureAt {
			rootCh <- cb.Capture()
		}
		up := NextUpdate(rng, c.Steady)
		cb.Update(up.OrderID, up.FillQty)
	}
	got := <-gotCh
	if !bytes.Equal(want, got) {
		t.Fatal("concurrent capture differs from STW replay at the same position")
	}
}
