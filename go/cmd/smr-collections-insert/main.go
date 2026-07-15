// smr-collections-insert (Go): time inserting resting orders into the book.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "insert"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	i := 0
	samples := bench.MeasureSmr(cfg.Warmup, cfg.Iters, func() {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
		i++
	})
	bench.EmitSmrLatency(experiment, "insert", samples)
}
