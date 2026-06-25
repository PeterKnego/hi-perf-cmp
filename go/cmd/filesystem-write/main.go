// filesystem-write benchmark (Go) — STUB.
//
// Emits one result-contract JSON line on stdout. Real measurement logic to be
// added later. See docs/result-contract.md for the schema.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

func main() {
	// Placeholder result. Replace Experiment/Metric/Value/Unit/Samples once the
	// real filesystem-write benchmark is implemented.
	bench.Emit(bench.Result{
		FocusArea:  "filesystem-write",
		Experiment: "placeholder",
		Metric:     "placeholder",
		Value:      0,
		Unit:       "ns",
		Samples:    0,
		Notes:      "stub",
	})
}
