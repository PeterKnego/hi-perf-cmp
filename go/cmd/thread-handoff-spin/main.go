// thread-handoff-spin (Go): single-slot atomic handoff, busy-wait. Emits three
// handoff_rtt_* lines. See the thread-handoff design spec.
package main

import (
	"sync/atomic"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "spin"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	var req, resp atomic.Uint64 // 0 == empty; token is a non-zero 1

	done := make(chan struct{})
	go func() {
		for i := 0; i < total; i++ {
			for req.Load() == 0 {
			}
			req.Store(0)
			resp.Store(1)
		}
		close(done)
	}()

	samples := bench.MeasureHandoff(cfg, func() {
		req.Store(1)
		for resp.Load() == 0 {
		}
		resp.Store(0)
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
