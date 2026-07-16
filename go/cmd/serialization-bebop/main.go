// serialization-bebop (Go): encode/decode cost of the ~500-byte journal
// record via the 200sc/bebop safe API.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalbop"
)

const experiment = "bebop"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	bench.RunJournal(experiment, cfg,
		func(i uint64) journalbop.JournalRecord {
			r := serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
			return serjournal.ToBebop(&r)
		},
		serjournal.EncodeBebop,
		serjournal.DecodeBebopChecksum,
	)
}
