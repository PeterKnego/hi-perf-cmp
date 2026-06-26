package bench

import (
	"os"
	"path/filepath"
	"testing"
)

func TestRunDurableAppendBatchAndPrealloc(t *testing.T) {
	dir := t.TempDir()
	cfg := FsConfig{Dir: dir, EntryBytes: 64, Warmup: 5, Iterations: 20, Batch: 4}

	samples, tput, err := RunDurableAppend(cfg, "test-batch", SyncData, 4, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(samples) != 5 {
		t.Fatalf("want 5 syncs (20/4), got %d", len(samples))
	}
	if tput <= 0 {
		t.Fatalf("want positive throughput, got %v", tput)
	}

	// Non-divisible: ceil(21/4) = 6.
	cfg2 := cfg
	cfg2.Iterations = 21
	s2, _, err := RunDurableAppend(cfg2, "test-batch2", SyncData, 4, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(s2) != 6 {
		t.Fatalf("want 6 syncs (ceil 21/4), got %d", len(s2))
	}

	// Prealloc, batch 1 → 20 syncs; file at least the preallocated size.
	s3, _, err := RunDurableAppend(cfg, "test-prealloc", SyncData, 1, true)
	if err != nil {
		t.Fatal(err)
	}
	if len(s3) != 20 {
		t.Fatalf("want 20 syncs, got %d", len(s3))
	}
	fi, err := os.Stat(filepath.Join(dir, "filesystem-write-test-prealloc.log"))
	if err != nil {
		t.Fatal(err)
	}
	if min := int64((cfg.Warmup + cfg.Iterations) * cfg.EntryBytes); fi.Size() < min {
		t.Fatalf("prealloc file too small: %d < %d", fi.Size(), min)
	}
}
