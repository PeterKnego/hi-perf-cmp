// thread-handoff-ring (Go): bounded SPSC ring, busy-wait, pipelined depth N.
// Emits one handoff_throughput line.
package main

import (
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "ring"

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := uint64(cfg.Warmup + cfg.Iterations)

	ring := newSPSC(cfg.RingCap)

	done := make(chan struct{})
	go func() {
		for i := uint64(0); i < total; i++ {
			ring.pop()
		}
		close(done)
	}()

	// Warmup pushes, then a drain barrier so timing excludes warmup.
	for i := 0; i < cfg.Warmup; i++ {
		ring.push(1)
	}
	for ring.consumed() < uint64(cfg.Warmup) {
	}

	start := time.Now()
	for i := 0; i < cfg.Iterations; i++ {
		ring.push(1)
	}
	for ring.consumed() < total {
	}
	elapsed := time.Since(start)

	<-done
	throughput := float64(cfg.Iterations) / elapsed.Seconds()
	bench.EmitHandoffThroughput(experiment, throughput, int64(cfg.Iterations))
}
