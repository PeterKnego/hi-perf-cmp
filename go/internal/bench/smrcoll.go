package bench

import (
	"fmt"
	"os"
	"sort"
	"strconv"
	"time"
)

const SMRFocusArea = "smr-collections"

// SmrConfig configures the LOB benchmark, sourced from SMRC_* env vars.
type SmrConfig struct {
	Cap      int
	Levels   uint32
	Tick     int64
	PriceMin int64
	Steady   int
	Warmup   int
	Iters    int
}

// LoadSmrConfig reads and validates the SMRC_* environment (plan Appendix A.1).
func LoadSmrConfig() (SmrConfig, error) {
	cap_, err := positiveEnv("SMRC_CAP", 262144)
	if err != nil {
		return SmrConfig{}, err
	}
	levels, err := positiveEnv("SMRC_LEVELS", 1024)
	if err != nil {
		return SmrConfig{}, err
	}
	tick, err := positiveEnv("SMRC_TICK", 1)
	if err != nil {
		return SmrConfig{}, err
	}
	steady, err := positiveEnv("SMRC_STEADY", 60000)
	if err != nil {
		return SmrConfig{}, err
	}
	warmup, err := positiveEnv("SMRC_WARMUP", 10000)
	if err != nil {
		return SmrConfig{}, err
	}
	iters, err := positiveEnv("SMRC_ITERS", 100000)
	if err != nil {
		return SmrConfig{}, err
	}
	priceMin := signedEnv("SMRC_PRICE_MIN", 0)

	cfg := SmrConfig{
		Cap: cap_, Levels: uint32(levels), Tick: int64(tick), PriceMin: priceMin,
		Steady: steady, Warmup: warmup, Iters: iters,
	}
	if levels > 65535 {
		return SmrConfig{}, fmt.Errorf("SMRC_LEVELS must be <= 65535")
	}
	if steady > cap_ || steady > 65535 {
		return SmrConfig{}, fmt.Errorf("SMRC_STEADY must be <= SMRC_CAP and <= 65535")
	}
	if warmup+iters > cap_ {
		return SmrConfig{}, fmt.Errorf("SMRC_WARMUP + SMRC_ITERS must be <= SMRC_CAP")
	}
	return cfg, nil
}

// signedEnv parses an int64 env var allowing zero/negative; returns def if unset.
func signedEnv(name string, def int64) int64 {
	s := os.Getenv(name)
	if s == "" {
		return def
	}
	v, err := strconv.ParseInt(s, 10, 64)
	if err != nil {
		return def
	}
	return v
}

// MeasureSmr runs warmup discarded ops, then times iters ops (ns) into a
// pre-allocated slice.
func MeasureSmr(warmup, iters int, op func()) []int64 {
	for i := 0; i < warmup; i++ {
		op()
	}
	samples := make([]int64, iters)
	for i := 0; i < iters; i++ {
		start := time.Now()
		op()
		samples[i] = time.Since(start).Nanoseconds()
	}
	return samples
}

// EmitSmrLatency sorts samples and emits {prefix}_p50/p99/mean (ns).
func EmitSmrLatency(experiment, prefix string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: SMRFocusArea, Experiment: experiment, Metric: prefix + "_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: SMRFocusArea, Experiment: experiment, Metric: prefix + "_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: SMRFocusArea, Experiment: experiment, Metric: prefix + "_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}

// EmitSmrInt emits one integer metric line.
func EmitSmrInt(experiment, metric string, value int64, unit string, samples int64) {
	Emit(Result{FocusArea: SMRFocusArea, Experiment: experiment, Metric: metric, Value: float64(value), Unit: unit, Samples: samples})
}

// EmitSmrFloat emits one fractional metric line.
func EmitSmrFloat(experiment, metric string, value float64, unit string, samples int64) {
	Emit(Result{FocusArea: SMRFocusArea, Experiment: experiment, Metric: metric, Value: value, Unit: unit, Samples: samples})
}
