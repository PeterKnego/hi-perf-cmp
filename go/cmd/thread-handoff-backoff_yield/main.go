// thread-handoff-backoff_yield (Go): the same paced ping-pong as
// thread-handoff-backoff, but the responder's ladder serves parks shorter
// than 1 ms by yielding to a deadline instead of sleeping — the aeron-go
// 1ce3720 strategy — so the ladder's short rungs are honoured instead of
// collapsing to the runtime timer's granularity. The backoff -> backoff_yield
// delta is that fix's value on this grid's methodology. See the backoff
// design spec and go/internal/idle.
package main

import (
	"sync/atomic"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/idle"
)

const experiment = "backoff_yield"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	var req, resp atomic.Uint64 // 0 == empty; token is a non-zero 1

	done := make(chan struct{})
	go func() {
		b := idle.NewYieldingBackoff()
		for i := 0; i < total; i++ {
			for req.Load() == 0 {
				b.Idle(0)
			}
			b.Idle(1) // work: reset the ladder
			req.Store(0)
			resp.Store(1)
		}
		close(done)
	}()

	samples := bench.MeasureHandoffPaced(cfg, func() {
		req.Store(1)
		for resp.Load() == 0 {
		}
		resp.Store(0)
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
