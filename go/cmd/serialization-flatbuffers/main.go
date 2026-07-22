// serialization-flatbuffers (Go): encode/decode cost of the ~500-byte journal
// record via Google FlatBuffers using the zero-copy read path (0 decode alloc).
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
)

const experiment = "flatbuffers"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	codec := serjournal.NewFBCodec()
	bench.RunJournal(experiment, cfg,
		func(i uint64) serjournal.Record {
			return serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
		},
		codec.Encode,
		codec.DecodeChecksum,
	)
}
