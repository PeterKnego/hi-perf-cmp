// smr-collections-live_stw_churn (Go): writer-observed latency under the
// churn workload while stop-the-world snapshots run inline at a fixed op
// cadence (the trigger op pays the whole serialize; writer_max is the stall).
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll"
)

const experiment = "live_stw_churn"

func main() {
	cfg, err := bench.LoadSmrConfig()
	if err != nil {
		bench.Fatalf("smr-collections-"+experiment, "%v", err)
	}
	book := smrcoll.NewBook(cfg)
	churn := smrcoll.NewChurn(cfg)
	churn.Prebuild(book, cfg.Steady)
	for i := 0; i < cfg.Warmup; i++ {
		smrcoll.ApplyChurn(book, churn.NextOp())
	}
	s := smrcoll.NewSnapshotter()
	// warm the encode path + buffer pages so the k=0 trigger measures
	// steady-state stall, not first-touch cost
	s.Encode(book)

	writerNs := make([]int64, cfg.LiveIters)
	snapNs := make([]int64, 0, cfg.LiveIters/cfg.SnapEvery+1)
	var snapLen int
	var ins, can, fil []int64
	rssPeak := bench.RSSBytes()
	for k := 0; k < cfg.LiveIters; k++ {
		op := churn.NextOp()
		fired := k%cfg.SnapEvery == 0
		t0 := time.Now()
		if fired {
			img := s.Encode(book)
			snapLen = len(img)
			snapNs = append(snapNs, time.Since(t0).Nanoseconds())
		}
		smrcoll.ApplyChurn(book, op)
		ns := time.Since(t0).Nanoseconds()
		// Sample RSS only AFTER the clock closes: RSSBytes reads
		// /proc/self/statm — microseconds against sub-microsecond ops — so
		// calling it inside the timed region would inflate writer_max, the one
		// metric this cell exists to report precisely.
		if fired {
			if r := bench.RSSBytes(); r > rssPeak {
				rssPeak = r
			}
		}
		writerNs[k] = ns
		switch op.Kind {
		case smrcoll.ChurnInsert:
			ins = append(ins, ns)
		case smrcoll.ChurnCancel:
			can = append(can, ns)
		case smrcoll.ChurnFill:
			fil = append(fil, ns)
		}
	}
	bench.EmitSmrLive(experiment, writerNs, snapNs, 0, int64(snapLen))
	if len(ins) > 0 {
		bench.EmitSmrLatency(experiment, "insert", ins)
	}
	if len(can) > 0 {
		bench.EmitSmrLatency(experiment, "cancel", can)
	}
	if len(fil) > 0 {
		bench.EmitSmrLatency(experiment, "fill", fil)
	}
	bench.EmitSmrInt(experiment, "rss_peak_bytes", rssPeak, "bytes", 1)
}
