// serialization-sbe_struct (Go): encode/decode cost of the ~500-byte journal
// record via the real-logic SBE tool's default (struct/owned) Golang codec —
// same wire as aeron_sbe, but decode materializes an owned struct (nonzero alloc).
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalsbestruct"
)

const experiment = "sbe_struct"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	codec := serjournal.NewSBEStructCodec()
	bench.RunJournal(experiment, cfg,
		func(i uint64) journalsbestruct.JournalRecord {
			r := serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
			return serjournal.ToSBEStruct(&r)
		},
		codec.Encode,
		codec.DecodeChecksum,
	)
}
