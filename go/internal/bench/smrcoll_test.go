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

func TestSmrConfigNewFieldDefaults(t *testing.T) {
	c, err := LoadSmrConfig()
	if err != nil {
		t.Fatal(err)
	}
	if c.Chunk != 4096 || c.LiveIters != 200000 || c.SnapEvery != 20000 {
		t.Fatalf("defaults wrong: %+v", c)
	}
}

func TestSmrConfigSnapEveryBound(t *testing.T) {
	t.Setenv("SMRC_LIVE_ITERS", "1000")
	t.Setenv("SMRC_SNAP_EVERY", "2000")
	if _, err := LoadSmrConfig(); err == nil {
		t.Fatal("want error: SNAP_EVERY > LIVE_ITERS")
	}
}

func TestSmrConfigChunkBound(t *testing.T) {
	t.Setenv("SMRC_CHUNK", "999999999")
	if _, err := LoadSmrConfig(); err == nil {
		t.Fatal("want error: CHUNK > CAP")
	}
}
