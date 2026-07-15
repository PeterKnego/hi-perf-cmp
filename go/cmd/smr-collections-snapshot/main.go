// smr-collections-snapshot (Go): time serialize + restore of a steady book.
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "snapshot"

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
	s := smrcoll.NewSnapshotter()
	snap := make([]int64, cfg.Iters)
	rest := make([]int64, cfg.Iters)
	var snapLen int
	for i := 0; i < cfg.Warmup; i++ {
		img := s.Encode(book)
		if _, err := smrcoll.Restore(img, cfg); err != nil {
			bench.Fatalf("smr-collections-"+experiment, "%v", err)
		}
	}
	for k := 0; k < cfg.Iters; k++ {
		t0 := time.Now()
		img := s.Encode(book)
		snap[k] = time.Since(t0).Nanoseconds()
		snapLen = len(img)
		t1 := time.Now()
		if _, err := smrcoll.Restore(img, cfg); err != nil {
			bench.Fatalf("smr-collections-"+experiment, "%v", err)
		}
		rest[k] = time.Since(t1).Nanoseconds()
	}
	bench.EmitSmrLatency(experiment, "snapshot", snap)
	bench.EmitSmrLatency(experiment, "restore", rest)
	bench.EmitSmrInt(experiment, "snapshot_bytes", int64(snapLen), "bytes", 1)
	mean := bench.Mean(snap)
	bench.EmitSmrFloat(experiment, "snapshot_throughput", float64(snapLen)/(mean/1e9), "bytes_per_sec", int64(cfg.Iters))
}
