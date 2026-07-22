package bench

import (
	"runtime"
	"sort"
	"time"
)

const serFocusArea = "serialization"

// serSink absorbs the decode checksums so the fold cannot be elided.
var serSink uint64

// SerialConfig configures the serialization journal benchmark, sourced from
// SER_* env vars (same names and defaults as the Rust cells). CmdBytes sizes
// the cmdText string on each entry (default 12); the record is field-heavy,
// dominated by the typed scalar fields rather than cmdText.
type SerialConfig struct {
	Warmup   int
	Iters    int
	Entries  int
	CmdBytes int
}

// LoadSerialConfig reads and validates the SER_* environment.
func LoadSerialConfig() (SerialConfig, error) {
	warmup, err := positiveEnv("SER_WARMUP", 1000)
	if err != nil {
		return SerialConfig{}, err
	}
	iters, err := positiveEnv("SER_ITERS", 100000)
	if err != nil {
		return SerialConfig{}, err
	}
	entries, err := positiveEnv("SER_ENTRIES", 4)
	if err != nil {
		return SerialConfig{}, err
	}
	cmdBytes, err := positiveEnv("SER_CMD_BYTES", 12)
	if err != nil {
		return SerialConfig{}, err
	}
	return SerialConfig{Warmup: warmup, Iters: iters, Entries: entries, CmdBytes: cmdBytes}, nil
}

// RunJournal drives the journal write/replay loop and emits the eight
// result-contract metrics. build(index) produces one codec-native record
// deterministically (pre-built, untimed — conversion from the logical model is
// not part of the measurement); encode(record, scratch) serializes into the
// reused scratch buffer and returns the encoded length; decode(bytes) decodes
// and fully materializes every field into a checksum, so owned-decode codecs
// and any future lazy codec pay for the same reads.
//
// Generic over the record type R so this package stays focus-neutral and never
// imports a focus area's model package.
func RunJournal[R any](experiment string, cfg SerialConfig, build func(uint64) R, encode func(R, []byte) int, decode func([]byte) uint64) {
	n := cfg.Iters

	// Pre-build all records (untimed); building is deterministic from index.
	records := make([]R, cfg.Warmup+n)
	for i := range records {
		records[i] = build(uint64(i))
	}

	scratch := make([]byte, 64*1024)
	encodeNs := make([]int64, 0, n)
	recordLen := 0

	// Warmup encode.
	for _, r := range records[:cfg.Warmup] {
		recordLen = encode(r, scratch)
	}
	// Timed encode.
	for _, r := range records[cfg.Warmup:] {
		t0 := time.Now()
		l := encode(r, scratch)
		dt := time.Since(t0).Nanoseconds()
		serSink ^= uint64(scratch[0])
		recordLen = l
		encodeNs = append(encodeNs, dt)
	}

	// Build the contiguous in-memory journal from the timed records.
	type frame struct{ off, len int }
	journal := make([]byte, 0, recordLen*n+64)
	frames := make([]frame, 0, n)
	for _, r := range records[cfg.Warmup:] {
		start := len(journal)
		l := encode(r, scratch)
		journal = append(journal, scratch[:l]...)
		frames = append(frames, frame{start, l})
	}

	decodeNs := make([]int64, 0, n)
	var sink uint64

	// Warmup decode.
	warm := cfg.Warmup
	if warm > len(frames) {
		warm = len(frames)
	}
	for _, f := range frames[:warm] {
		sink ^= decode(journal[f.off : f.off+f.len])
	}

	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)
	for _, f := range frames {
		t0 := time.Now()
		sum := decode(journal[f.off : f.off+f.len])
		dt := time.Since(t0).Nanoseconds()
		sink ^= sum
		decodeNs = append(decodeNs, dt)
	}
	runtime.ReadMemStats(&after)
	serSink ^= sink

	decodeAllocPer := (after.TotalAlloc - before.TotalAlloc) / uint64(n)

	emitSerLatency(experiment, "encode", encodeNs)
	emitSerLatency(experiment, "decode", decodeNs)
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: "encoded_bytes",
		Value: float64(recordLen), Unit: "bytes", Samples: 1})
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: "decode_alloc_bytes",
		Value: float64(decodeAllocPer), Unit: "bytes", Samples: int64(n)})
}

// emitSerLatency sorts samples and emits {op}_p50/p99/mean (ns), mirroring
// EmitSmrLatency but for the serialization focus area.
func emitSerLatency(experiment, op string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: op + "_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: op + "_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: serFocusArea, Experiment: experiment, Metric: op + "_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}
