// smr-collections-mvcc_churn (Go): the churn workload against the chunked
// copy-on-write book. Cancels scatter writes across chunks rather than
// appending to the newest one, so this is where CoW's first-touch copy cost
// is exercised hardest.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "mvcc_churn"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewCowBook(cfg)
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
