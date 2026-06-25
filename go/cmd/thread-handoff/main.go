// thread-handoff benchmark (Go) — STUB.
//
// Emits one result-contract JSON line on stdout. Real measurement logic to be
// added later. See docs/result-contract.md for the schema.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/result"

func main() {
	// Placeholder result. Replace Metric/Value/Unit/Samples once the real
	// thread-handoff (goroutine/channel) benchmark is implemented.
	result.Emit(result.Result{
		FocusArea: "thread-handoff",
		Metric:    "placeholder",
		Value:     0,
		Unit:      "ns",
		Samples:   0,
		Notes:     "stub",
	})
}
