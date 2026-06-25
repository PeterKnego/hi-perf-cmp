package main

import (
	"fmt"
	"os"
	"strconv"
)

// Config holds the benchmark parameters, sourced from env vars with defaults.
type Config struct {
	PayloadBytes int
	Warmup       int
	Iterations   int
}

// loadConfig reads RTT_PAYLOAD_BYTES, RTT_WARMUP and RTT_ITERATIONS from the
// environment, applying defaults. Each value must be a positive integer;
// invalid or non-positive values yield an error.
func loadConfig() (Config, error) {
	payload, err := positiveEnv("RTT_PAYLOAD_BYTES", 64)
	if err != nil {
		return Config{}, err
	}
	warmup, err := positiveEnv("RTT_WARMUP", 10000)
	if err != nil {
		return Config{}, err
	}
	iterations, err := positiveEnv("RTT_ITERATIONS", 100000)
	if err != nil {
		return Config{}, err
	}
	return Config{
		PayloadBytes: payload,
		Warmup:       warmup,
		Iterations:   iterations,
	}, nil
}

// positiveEnv parses env var name as a positive integer, returning def when
// unset/empty. Returns an error for non-integer or non-positive values.
func positiveEnv(name string, def int) (int, error) {
	s := os.Getenv(name)
	if s == "" {
		return def, nil
	}
	v, err := strconv.Atoi(s)
	if err != nil {
		return 0, fmt.Errorf("%s: %q is not a valid integer", name, s)
	}
	if v <= 0 {
		return 0, fmt.Errorf("%s: must be a positive integer, got %d", name, v)
	}
	return v, nil
}
