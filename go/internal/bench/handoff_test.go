package bench

import (
	"testing"
	"time"
)

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

func TestMeasureHandoffPacedKeepsTheGapBetweenRoundTrips(t *testing.T) {
	gap := int64(time.Millisecond)
	cfg := HandoffConfig{Warmup: 1, Iterations: 4, RingCap: 16, GapNs: gap}
	var starts []time.Time
	samples := MeasureHandoffPaced(cfg, func() { starts = append(starts, time.Now()) })
	if len(samples) != 4 {
		t.Fatalf("want 4 samples, got %d", len(samples))
	}
	if len(starts) != 5 {
		t.Fatalf("want 5 calls (warmup+iterations), got %d", len(starts))
	}
	for i := 1; i < len(starts); i++ {
		if d := starts[i].Sub(starts[i-1]).Nanoseconds(); d < gap {
			t.Fatalf("round trips %d..%d only %dns apart, want >= %dns", i-1, i, d, gap)
		}
	}
	// The gap busy-wait must sit OUTSIDE the timed window: each sample is the
	// (near-instant) round trip, never the millisecond gap.
	for i, s := range samples {
		if s >= gap {
			t.Fatalf("sample %d = %dns includes the gap", i, s)
		}
	}
}
