// smr-collections-live_stw (Go): writer-observed latency while stop-the-world
// snapshots run inline at a fixed op cadence (the trigger op pays the whole
// serialize; writer_max is the stall).
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_stw"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	for i := 0; i < cfg.Steady; i++ {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	n := cfg.Steady
	for i := 0; i < cfg.Warmup; i++ {
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
	}
	s := smrcoll.NewSnapshotter()
	writerNs := make([]int64, cfg.LiveIters)
	snapNs := make([]int64, 0, cfg.LiveIters/cfg.SnapEvery+1)
	var snapLen int
	for k := 0; k < cfg.LiveIters; k++ {
		t0 := time.Now()
		if k%cfg.SnapEvery == 0 {
			img := s.Encode(book)
			snapLen = len(img)
			snapNs = append(snapNs, time.Since(t0).Nanoseconds())
		}
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
		writerNs[k] = time.Since(t0).Nanoseconds()
	}
	bench.EmitSmrLive(experiment, writerNs, snapNs, 0, int64(snapLen))
}
