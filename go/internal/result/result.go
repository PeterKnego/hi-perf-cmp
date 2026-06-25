// Package result defines the shared benchmark result contract and a helper to
// emit it as a single JSON line on stdout. See docs/result-contract.md.
package result

import (
	"encoding/json"
	"fmt"
	"os"
)

// Result is one benchmark measurement in the shared cross-language format.
type Result struct {
	Language  string  `json:"language"`
	FocusArea string  `json:"focus_area"`
	Metric    string  `json:"metric"`
	Value     float64 `json:"value"`
	Unit      string  `json:"unit"`
	Samples   int64   `json:"samples"`
	Notes     string  `json:"notes,omitempty"`
}

// Emit writes r as a single JSON line to stdout.
func Emit(r Result) {
	r.Language = "go"
	b, err := json.Marshal(r)
	if err != nil {
		fmt.Fprintf(os.Stderr, "result: marshal failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Println(string(b))
}
