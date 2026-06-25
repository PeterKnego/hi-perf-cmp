package bench

import (
	"fmt"
	"os"
	"sort"
	"time"
)

// RoundTrip performs exactly one strict ping-pong round trip (send a request,
// read the full echo, assert equality). It is supplied by each experiment and
// must allocate nothing on the timed path.
type RoundTrip func() error

// Measure runs cfg.Warmup discarded round trips, then cfg.Iterations timed
// round trips, recording per-round-trip elapsed nanoseconds into a
// pre-allocated buffer. The buffer is allocated once, before timing, so
// allocation never falls inside the measured path.
func Measure(cfg Config, rt RoundTrip) ([]int64, error) {
	// Warmup (discarded).
	for i := 0; i < cfg.Warmup; i++ {
		if err := rt(); err != nil {
			return nil, err
		}
	}

	// Measured.
	samples := make([]int64, cfg.Iterations)
	for i := 0; i < cfg.Iterations; i++ {
		start := time.Now()
		if err := rt(); err != nil {
			return nil, err
		}
		samples[i] = time.Since(start).Nanoseconds()
	}

	return samples, nil
}

// EmitRTT sorts samples and emits the rtt_p50, rtt_p99 and rtt_mean result
// lines (unit ns) for the given experiment. samples is sorted in place.
func EmitRTT(experiment string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))

	Emit(Result{
		FocusArea:  "network-rtt",
		Experiment: experiment,
		Metric:     "rtt_p50",
		Value:      float64(Percentile(samples, 50)),
		Unit:       "ns",
		Samples:    n,
	})
	Emit(Result{
		FocusArea:  "network-rtt",
		Experiment: experiment,
		Metric:     "rtt_p99",
		Value:      float64(Percentile(samples, 99)),
		Unit:       "ns",
		Samples:    n,
	})
	Emit(Result{
		FocusArea:  "network-rtt",
		Experiment: experiment,
		Metric:     "rtt_mean",
		Value:      Mean(samples),
		Unit:       "ns",
		Samples:    n,
	})
}

// Logf writes a diagnostic line to stderr (never stdout).
func Logf(prefix, format string, args ...any) {
	fmt.Fprintf(os.Stderr, prefix+": "+format+"\n", args...)
}

// Fatalf logs to stderr and exits with status 1.
func Fatalf(prefix, format string, args ...any) {
	Logf(prefix, format, args...)
	os.Exit(1)
}
