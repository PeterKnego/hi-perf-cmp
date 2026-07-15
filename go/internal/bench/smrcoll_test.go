package bench

import "testing"

func TestMeasureSmrSampleCount(t *testing.T) {
	calls := 0
	s := MeasureSmr(3, 5, func() { calls++ })
	if len(s) != 5 {
		t.Fatalf("want 5 samples, got %d", len(s))
	}
	if calls != 8 {
		t.Fatalf("want 8 calls, got %d", calls)
	}
}
