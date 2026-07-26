// smr-collections-mvcc_update (Go): time amend/partial-fill on existing orders in CoW book.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "mvcc_update"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewCowBook(cfg)
	rng := smrcoll.NewSplitMix(smrcoll.SmrSeed)
	for i := 0; i < cfg.Steady; i++ {
		ins := smrcoll.NextInsert(rng, i, cfg.Levels, cfg.Tick, cfg.PriceMin)
		book.Insert(ins.OrderID, ins.Price, ins.Qty, ins.Side)
	}
	n := cfg.Steady
	samples := bench.MeasureSmr(cfg.Warmup, cfg.Iters, func() {
		up := smrcoll.NextUpdate(rng, n)
		book.Update(up.OrderID, up.FillQty)
	})
	bench.EmitSmrLatency(experiment, "update", samples)
}
