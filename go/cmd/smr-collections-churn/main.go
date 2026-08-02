// smr-collections-churn (Go): insert/cancel/fill at a real-exchange
// order-to-trade ratio against the flat stop-the-world book. Cancels recycle
// slots through the free list, so this is the steady state a matching engine
// actually lives in.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "churn"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	churn := smrcoll.NewChurn(cfg)
	churn.Prebuild(book, cfg.Steady)
	samples, rss0 := smrcoll.RunChurn(cfg, book, churn)
	rss1 := bench.RSSBytes()
	growth := rss1 - rss0
	if growth < 0 {
		growth = 0
	}
	smrcoll.EmitChurn(experiment, samples, growth)
}
