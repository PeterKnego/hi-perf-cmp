package bench

import (
	"os"
	"testing"
)

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

func TestOtrBpsDefaultsTo100(t *testing.T) {
	os.Unsetenv("SMRC_OTR_BPS")
	c, err := LoadSmrConfig()
	if err != nil {
		t.Fatalf("defaults must parse: %v", err)
	}
	if c.OtrBps != 100 {
		t.Fatalf("OtrBps = %d, want 100 (1%%)", c.OtrBps)
	}
}

func TestOtrBpsZeroIsLegalAndOver10000Rejected(t *testing.T) {
	os.Setenv("SMRC_OTR_BPS", "0")
	c, err := LoadSmrConfig()
	os.Unsetenv("SMRC_OTR_BPS")
	if err != nil {
		t.Fatalf("0 bps (pure-cancel run) must be legal: %v", err)
	}
	if c.OtrBps != 0 {
		t.Fatalf("OtrBps = %d, want 0", c.OtrBps)
	}
	os.Setenv("SMRC_OTR_BPS", "10001")
	_, err = LoadSmrConfig()
	os.Unsetenv("SMRC_OTR_BPS")
	if err == nil {
		t.Fatal("OTR above 100% must be rejected")
	}
}

func TestChurnSizedRunParsesButFailsBumpCapacity(t *testing.T) {
	// warmup+iters > cap is legal for a slot-recycling churn cell and illegal
	// for a bump-allocating insert cell.
	os.Setenv("SMRC_CAP", "1024")
	os.Setenv("SMRC_STEADY", "512")
	os.Setenv("SMRC_CHUNK", "256")
	os.Setenv("SMRC_WARMUP", "1000")
	os.Setenv("SMRC_ITERS", "10000")
	c, err := LoadSmrConfig()
	bumpErr := error(nil)
	if err == nil {
		bumpErr = c.RequireBumpCapacity()
	}
	for _, k := range []string{"SMRC_CAP", "SMRC_STEADY", "SMRC_CHUNK", "SMRC_WARMUP", "SMRC_ITERS"} {
		os.Unsetenv(k)
	}
	if err != nil {
		t.Fatalf("churn-sized config must parse: %v", err)
	}
	if bumpErr == nil {
		t.Fatal("bump-allocating cells must reject warmup+iters > cap")
	}
}

func TestRSSBytesIsNonzero(t *testing.T) {
	if RSSBytes() <= 0 {
		t.Fatal("RSS must be readable from /proc/self/statm")
	}
}
