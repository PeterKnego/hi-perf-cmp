// thread-handoff-backoff (Go): paced ping-pong where the responder waits
// under the naive Aeron-style backoff ladder (spin -> yield -> time.Sleep
// park doubling 1µs -> 1ms). Go's timer overshoots sub-millisecond sleeps by
// orders of magnitude, collapsing the ladder to its top rung — deliberately
// kept: that overshoot is what this cell measures. See the backoff design
// spec and go/internal/idle.
//
// The requester busy-waits TH_GAP_NS between round trips (untimed) so the
// responder's ladder ramps, then times the round trip while spinning — the
// requester is the measurement side, the responder the system-under-test.
package main

import (
	"sync/atomic"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/idle"
)

const experiment = "backoff"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	var req, resp atomic.Uint64 // 0 == empty; token is a non-zero 1

	done := make(chan struct{})
	go func() {
		b := idle.NewBackoff()
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
