// filesystem-write-batch benchmark (Go): batch-append entries with preallocation, fdatasync per batch.
// Emits four result-contract JSON lines. See the filesystem-write design spec.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

const experiment = "batch"

func main() {
	cfg, err := bench.LoadFsConfig()
	if err != nil {
		bench.Fatalf("filesystem-write-"+experiment, "%v", err)
	}
	samples, throughput, err := bench.RunDurableAppend(cfg, experiment, bench.SyncData, cfg.Batch, true)
	if err != nil {
		bench.Fatalf("filesystem-write-"+experiment, "%v", err)
	}
	bench.EmitFS(experiment, samples, throughput, cfg.Iterations)
}
