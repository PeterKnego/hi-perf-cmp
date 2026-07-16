// serialization-protobuf (Go): encode/decode cost of the ~500-byte journal
// record via the canonical google.golang.org/protobuf runtime.
package main

import (
	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal"
	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalpb"
)

const experiment = "protobuf"

func main() {
	cfg, err := bench.LoadSerialConfig()
	if err != nil {
		bench.Fatalf("serialization-"+experiment, "%v", err)
	}
	bench.RunJournal(experiment, cfg,
		func(i uint64) *journalpb.JournalRecord {
			r := serjournal.BuildRecord(i, cfg.Entries, cfg.CmdBytes)
			return serjournal.ToProto(&r)
		},
		serjournal.EncodeProto,
		serjournal.DecodeProtoChecksum,
	)
}
