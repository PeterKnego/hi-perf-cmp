package bench

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
	"time"
)

// FSWFocusArea is the focus area for every filesystem-write experiment.
const FSWFocusArea = "filesystem-write"

// SyncKind selects the durability barrier issued per commit.
type SyncKind int

const (
	// SyncFull is a full fsync (data + all metadata).
	SyncFull SyncKind = iota
	// SyncData is fdatasync (data + size, skips timestamps).
	SyncData
)

// FsConfig holds the filesystem-write parameters from the FSW_* env vars.
type FsConfig struct {
	Dir        string
	EntryBytes int
	Warmup     int
	Iterations int
	Batch      int
}

// LoadFsConfig reads FSW_DIR (required), FSW_ENTRY_BYTES, FSW_WARMUP,
// FSW_ITERATIONS and FSW_BATCH, applying defaults. A missing FSW_DIR (guards
// against tmpfs) or any invalid value yields an error.
func LoadFsConfig() (FsConfig, error) {
	dir := strings.TrimSpace(os.Getenv("FSW_DIR"))
	if dir == "" {
		return FsConfig{}, fmt.Errorf("FSW_DIR: required (set FSW_DIR=<dir on a real disk, not tmpfs>)")
	}
	entryBytes, err := positiveEnv("FSW_ENTRY_BYTES", 256)
	if err != nil {
		return FsConfig{}, err
	}
	warmup, err := positiveEnv("FSW_WARMUP", 5000)
	if err != nil {
		return FsConfig{}, err
	}
	iterations, err := positiveEnv("FSW_ITERATIONS", 50000)
	if err != nil {
		return FsConfig{}, err
	}
	batch, err := positiveEnv("FSW_BATCH", 32)
	if err != nil {
		return FsConfig{}, err
	}
	return FsConfig{Dir: dir, EntryBytes: entryBytes, Warmup: warmup, Iterations: iterations, Batch: batch}, nil
}

// RunDurableAppend runs one filesystem-write experiment and returns the per-sync
// latencies in nanoseconds plus the end-to-end throughput in entries/sec.
// batchSize entries are written per sync; prealloc pre-writes the file so a
// size-extending sync is avoided.
func RunDurableAppend(cfg FsConfig, experiment string, sync SyncKind, batchSize int, prealloc bool) ([]int64, float64, error) {
	path := filepath.Join(cfg.Dir, "filesystem-write-"+experiment+".log")
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
	if err != nil {
		return nil, 0, fmt.Errorf("open %s: %w", path, err)
	}
	defer f.Close()

	// Make the file's existence durable (file + parent dir), outside timing.
	if err := f.Sync(); err != nil {
		return nil, 0, fmt.Errorf("initial sync: %w", err)
	}
	if err := syncDir(cfg.Dir); err != nil {
		return nil, 0, err
	}

	entry := make([]byte, cfg.EntryBytes)
	for i := range entry {
		entry[i] = 0xAB
	}

	if prealloc {
		if err := preallocate(f, (cfg.Warmup+cfg.Iterations)*cfg.EntryBytes); err != nil {
			return nil, 0, err
		}
	}

	doSync := func() error {
		if sync == SyncFull {
			return f.Sync()
		}
		return syscall.Fdatasync(int(f.Fd()))
	}

	// Warmup (discarded).
	if err := runEntries(f, entry, cfg.Warmup, batchSize, doSync, nil); err != nil {
		return nil, 0, err
	}

	// Measured.
	samples := make([]int64, 0, (cfg.Iterations+batchSize-1)/batchSize)
	tStart := time.Now()
	if err := runEntries(f, entry, cfg.Iterations, batchSize, doSync, &samples); err != nil {
		return nil, 0, err
	}
	throughput := float64(cfg.Iterations) / time.Since(tStart).Seconds()
	return samples, throughput, nil
}

// runEntries writes `entries` entries in chunks of batchSize, syncing once per
// chunk (trailing short chunk allowed). When samples != nil, each sync's elapsed
// ns is appended.
func runEntries(f *os.File, entry []byte, entries, batchSize int, doSync func() error, samples *[]int64) error {
	remaining := entries
	for remaining > 0 {
		count := batchSize
		if remaining < count {
			count = remaining
		}
		for i := 0; i < count; i++ {
			if _, err := f.Write(entry); err != nil {
				return fmt.Errorf("write: %w", err)
			}
		}
		start := time.Now()
		if err := doSync(); err != nil {
			return fmt.Errorf("sync: %w", err)
		}
		if samples != nil {
			*samples = append(*samples, time.Since(start).Nanoseconds())
		}
		remaining -= count
	}
	return nil
}

// preallocate real-zero-writes `total` bytes, fsyncs once, and seeks back to 0
// so the timed loop overwrites already-written blocks (no i_size extension).
func preallocate(f *os.File, total int) error {
	zeros := make([]byte, 1024*1024)
	remaining := total
	for remaining > 0 {
		n := len(zeros)
		if remaining < n {
			n = remaining
		}
		if _, err := f.Write(zeros[:n]); err != nil {
			return fmt.Errorf("preallocate write: %w", err)
		}
		remaining -= n
	}
	if err := f.Sync(); err != nil {
		return fmt.Errorf("preallocate sync: %w", err)
	}
	if _, err := f.Seek(0, io.SeekStart); err != nil {
		return fmt.Errorf("preallocate seek: %w", err)
	}
	return nil
}

// syncDir fsyncs the directory so a newly created file's entry is durable.
func syncDir(dir string) error {
	d, err := os.Open(dir)
	if err != nil {
		return fmt.Errorf("open dir %s: %w", dir, err)
	}
	defer d.Close()
	if err := d.Sync(); err != nil {
		return fmt.Errorf("sync dir %s: %w", dir, err)
	}
	return nil
}

// EmitFS sorts the per-sync samples and emits the four filesystem-write result
// lines. samples is sorted in place.
func EmitFS(experiment string, samples []int64, throughput float64, iterations int) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	nSync := int64(len(samples))
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "durable_append_throughput", Value: throughput, Unit: "ops_per_sec", Samples: int64(iterations)})
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "sync_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: nSync})
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "sync_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: nSync})
	Emit(Result{FocusArea: FSWFocusArea, Experiment: experiment, Metric: "sync_mean", Value: Mean(samples), Unit: "ns", Samples: nSync})
}
