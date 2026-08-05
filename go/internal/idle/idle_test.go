package idle

import "testing"

// recordingParker captures park requests instead of waiting.
type recordingParker struct {
	parks []int64
}

func (r *recordingParker) park(ns int64) {
	r.parks = append(r.parks, ns)
}

func newRecorded() (*Backoff, *recordingParker) {
	rec := &recordingParker{}
	b := NewBackoff()
	b.parker = rec.park
	return b, rec
}

func TestSpinsAndYieldsBeforeFirstPark(t *testing.T) {
	b, rec := newRecorded()
	for i := 0; i < MaxSpins+MaxYields; i++ {
		b.Idle(0)
	}
	if len(rec.parks) != 0 {
		t.Fatalf("parked during spin/yield rungs: %v", rec.parks)
	}
	b.Idle(0)
	if len(rec.parks) != 1 || rec.parks[0] != MinParkNs {
		t.Fatalf("first park must be MinParkNs, got %v", rec.parks)
	}
}

func TestParkPeriodDoublesAndCapsAtMax(t *testing.T) {
	b, rec := newRecorded()
	for i := 0; i < MaxSpins+MaxYields; i++ {
		b.Idle(0)
	}
	for i := 0; i < 16; i++ {
		b.Idle(0)
	}
	want := []int64{1000, 2000, 4000, 8000, 16000, 32000, 64000, 128000,
		256000, 512000, 1000000, 1000000, 1000000, 1000000, 1000000, 1000000}
	if len(rec.parks) != len(want) {
		t.Fatalf("park count %d != %d", len(rec.parks), len(want))
	}
	for i, w := range want {
		if rec.parks[i] != w {
			t.Fatalf("park[%d] = %d, want %d (all: %v)", i, rec.parks[i], w, rec.parks)
		}
	}
}

func TestWorkResetsTheLadder(t *testing.T) {
	b, rec := newRecorded()
	for i := 0; i < MaxSpins+MaxYields+3; i++ {
		b.Idle(0)
	}
	if len(rec.parks) != 3 {
		t.Fatalf("expected 3 parks before reset, got %v", rec.parks)
	}
	b.Idle(1) // work: full reset
	for i := 0; i < MaxSpins+MaxYields; i++ {
		b.Idle(0)
	}
	if len(rec.parks) != 3 {
		t.Fatalf("post-reset spin/yield rungs must not park, got %v", rec.parks)
	}
	b.Idle(0)
	if rec.parks[3] != MinParkNs {
		t.Fatalf("post-reset park must restart at MinParkNs, got %d", rec.parks[3])
	}
}

func TestYieldingVariantUsesSameLadder(t *testing.T) {
	rec := &recordingParker{}
	b := NewYieldingBackoff()
	b.parker = rec.park
	for i := 0; i < MaxSpins+MaxYields+2; i++ {
		b.Idle(0)
	}
	if len(rec.parks) != 2 || rec.parks[0] != MinParkNs || rec.parks[1] != 2*MinParkNs {
		t.Fatalf("yielding variant must run the identical ladder, got %v", rec.parks)
	}
}
