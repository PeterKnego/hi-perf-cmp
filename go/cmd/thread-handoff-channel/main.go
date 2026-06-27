// thread-handoff-channel (Go): unbuffered chan rendezvous in each direction —
// the idiomatic blocking-queue handoff. Emits three handoff_rtt_* lines.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

const experiment = "channel"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	req := make(chan uint64)  // unbuffered == rendezvous
	resp := make(chan uint64)

	done := make(chan struct{})
	go func() {
		for i := 0; i < total; i++ {
			v := <-req
			resp <- v
		}
		close(done)
	}()

	samples := bench.MeasureHandoff(cfg, func() {
		req <- 1
		<-resp
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
