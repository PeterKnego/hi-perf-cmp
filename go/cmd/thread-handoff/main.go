// thread-handoff benchmark (Go) — STUB.
//
// Emits one result-contract JSON line on stdout. Real measurement logic to be
// added later. See docs/result-contract.md for the schema.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

func main() {
	// Placeholder result. Replace Experiment/Metric/Value/Unit/Samples once the
	// real thread-handoff (goroutine/channel) benchmark is implemented.
	bench.Emit(bench.Result{
		FocusArea:  "thread-handoff",
		Experiment: "placeholder",
		Metric:     "placeholder",
		Value:      0,
		Unit:       "ns",
		Samples:    0,
		Notes:      "stub",
	})
}
