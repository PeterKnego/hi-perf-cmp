package bench

import (
	"sort"
	"time"
)

// THFocusArea is the focus area for every thread-handoff experiment.
const THFocusArea = "thread-handoff"

// HandoffConfig holds the thread-handoff parameters from the TH_* env vars.
type HandoffConfig struct {
	Warmup     int
	Iterations int
	RingCap    int
	// GapNs paces the backoff cells: the requester busy-waits this long
	// between round trips so the responder's idle ladder ramps. Ignored by
	// the unpaced cells.
	GapNs int64
}

// LoadHandoffConfig reads TH_WARMUP, TH_ITERATIONS and TH_RING_CAP, applying
// defaults. Invalid or non-positive values yield an error.
func LoadHandoffConfig() (HandoffConfig, error) {
	warmup, err := positiveEnv("TH_WARMUP", 10000)
	if err != nil {
		return HandoffConfig{}, err
	}
	iterations, err := positiveEnv("TH_ITERATIONS", 100000)
	if err != nil {
		return HandoffConfig{}, err
	}
	ringCap, err := positiveEnv("TH_RING_CAP", 1024)
	if err != nil {
		return HandoffConfig{}, err
	}
	gapNs, err := positiveEnv("TH_GAP_NS", 100000)
	if err != nil {
		return HandoffConfig{}, err
	}
	return HandoffConfig{Warmup: warmup, Iterations: iterations, RingCap: ringCap,
		GapNs: int64(gapNs)}, nil
}

// HandoffRoundTrip performs exactly one ping-pong handoff (send a token, wait
// for its echo). Infallible — it runs entirely in-process.
type HandoffRoundTrip func()

// MeasureHandoff runs cfg.Warmup discarded round trips, then cfg.Iterations
// timed round trips into a pre-allocated buffer (ns). Mirrors Measure but for
// the infallible in-process handoff.
func MeasureHandoff(cfg HandoffConfig, rt HandoffRoundTrip) []int64 {
	for i := 0; i < cfg.Warmup; i++ {
		rt()
	}
	samples := make([]int64, cfg.Iterations)
	for i := 0; i < cfg.Iterations; i++ {
		start := time.Now()
		rt()
		samples[i] = time.Since(start).Nanoseconds()
	}
	return samples
}

// MeasureHandoffPaced is MeasureHandoff with a busy-waited gap of cfg.GapNs
// before every round trip (warmup included), left OUTSIDE the timed window.
// The gap lets a responder's idle ladder ramp between requests; it is a
// busy-wait, not a sleep, so the requester's send timing does not inherit the
// timer overshoot the backoff cells exist to measure.
func MeasureHandoffPaced(cfg HandoffConfig, rt HandoffRoundTrip) []int64 {
	gap := time.Duration(cfg.GapNs)
	wait := func() {
		deadline := time.Now().Add(gap)
		for time.Now().Before(deadline) {
		}
	}
	for i := 0; i < cfg.Warmup; i++ {
		wait()
		rt()
	}
	samples := make([]int64, cfg.Iterations)
	for i := 0; i < cfg.Iterations; i++ {
		wait()
		start := time.Now()
		rt()
		samples[i] = time.Since(start).Nanoseconds()
	}
	return samples
}

// EmitHandoff sorts samples and emits the handoff_rtt_p50/p99/mean lines (ns).
func EmitHandoff(experiment string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_rtt_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_rtt_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_rtt_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}

// EmitHandoffThroughput emits the single handoff_throughput line (ops/sec).
func EmitHandoffThroughput(experiment string, opsPerSec float64, samples int64) {
	Emit(Result{FocusArea: THFocusArea, Experiment: experiment, Metric: "handoff_throughput", Value: opsPerSec, Unit: "ops_per_sec", Samples: samples})
}
