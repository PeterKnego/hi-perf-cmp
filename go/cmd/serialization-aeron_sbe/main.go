// serialization-aeron_sbe (Go): encode/decode cost of the ~500-byte journal
// record via the real-logic SBE tool's zero-copy Golang flyweight codec — the
// Go twin of the Rust aeron_sbe cell (same tool, same wire).
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
)

const experiment = "aeron_sbe"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	codec := serjournal.NewSBECodec()
	bench.RunJournal(experiment, cfg,
		func(i uint64) serjournal.Record {
			return serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
		},
		codec.Encode,
		codec.DecodeChecksum,
	)
}
