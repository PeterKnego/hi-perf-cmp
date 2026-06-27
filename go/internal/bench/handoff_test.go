package bench

import "testing"

func TestMeasureHandoffSampleCountAndCalls(t *testing.T) {
	cfg := HandoffConfig{Warmup: 3, Iterations: 5, RingCap: 16}
	calls := 0
	samples := MeasureHandoff(cfg, func() { calls++ })
	if len(samples) != 5 {
		t.Fatalf("want 5 samples, got %d", len(samples))
	}
	if calls != 8 {
		t.Fatalf("want 8 calls (warmup+iterations), got %d", calls)
	}
}
