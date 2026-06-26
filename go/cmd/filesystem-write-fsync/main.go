// filesystem-write-fsync benchmark (Go): append one entry, full fsync per entry.
// Emits four result-contract JSON lines. See the filesystem-write design spec.
package main

import "github.com/peterknego/hi-perf-cmp/go/internal/bench"

const experiment = "fsync"

func main() {
	cfg, err := bench.LoadFsConfig()
	if err != nil {
		bench.Fatalf("filesystem-write-"+experiment, "%v", err)
	}
	samples, throughput, err := bench.RunDurableAppend(cfg, experiment, bench.SyncFull, 1, false)
	if err != nil {
		bench.Fatalf("filesystem-write-"+experiment, "%v", err)
	}
	bench.EmitFS(experiment, samples, throughput, cfg.Iterations)
}
