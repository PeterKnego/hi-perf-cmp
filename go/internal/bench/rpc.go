package bench

import (
	"fmt"
	"os"
	"sort"
	"time"
)

const rpcFocusArea = "rpc-roundtrip"

// RpcConfig configures the rpc-roundtrip benchmark, sourced from RPC_* env vars.
type RpcConfig struct {
	Mode       Mode
	Host       string
	UDPPort    int
	TCPPort    int
	GRPCPort   int
	Warmup     int
	Iterations int
}

// LoadRpcConfig reads and validates the RPC_* environment.
func LoadRpcConfig() (RpcConfig, error) {
	mode, err := loadMode("RPC_MODE", ModeLoopback)
	if err != nil {
		return RpcConfig{}, err
	}
	udp, err := positiveEnv("RPC_UDP_PORT", 9200)
	if err != nil {
		return RpcConfig{}, err
	}
	tcp, err := positiveEnv("RPC_TCP_PORT", 9201)
	if err != nil {
		return RpcConfig{}, err
	}
	grpcPort, err := positiveEnv("RPC_GRPC_PORT", 9202)
	if err != nil {
		return RpcConfig{}, err
	}
	warmup, err := positiveEnv("RPC_WARMUP", 10000)
	if err != nil {
		return RpcConfig{}, err
	}
	iters, err := positiveEnv("RPC_ITERATIONS", 100000)
	if err != nil {
		return RpcConfig{}, err
	}
	host := os.Getenv("RPC_HOST")
	if mode == ModeClient && host == "" {
		return RpcConfig{}, fmt.Errorf("RPC_HOST: required in client mode")
	}
	return RpcConfig{Mode: mode, Host: host, UDPPort: udp, TCPPort: tcp, GRPCPort: grpcPort, Warmup: warmup, Iterations: iters}, nil
}

// MeasureN runs warmup discarded round trips, then times iterations round trips
// into a pre-allocated buffer (allocation never enters the timed path).
func MeasureN(warmup, iterations int, rt RoundTrip) ([]int64, error) {
	for i := 0; i < warmup; i++ {
		if err := rt(); err != nil {
			return nil, err
		}
	}
	samples := make([]int64, iterations)
	for i := 0; i < iterations; i++ {
		start := time.Now()
		if err := rt(); err != nil {
			return nil, err
		}
		samples[i] = time.Since(start).Nanoseconds()
	}
	return samples, nil
}

// EmitRoundtrip sorts samples and emits rtt_p50/p99/mean (ns) under the
// rpc-roundtrip focus area. samples is sorted in place.
func EmitRoundtrip(experiment string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: "rtt_p50", Value: float64(Percentile(samples, 50)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: "rtt_p99", Value: float64(Percentile(samples, 99)), Unit: "ns", Samples: n})
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: "rtt_mean", Value: Mean(samples), Unit: "ns", Samples: n})
}

// EmitRoundtripInt emits one integer metric line under the rpc-roundtrip focus area.
func EmitRoundtripInt(experiment, metric string, value int64, unit string, samples int64) {
	Emit(Result{FocusArea: rpcFocusArea, Experiment: experiment, Metric: metric, Value: float64(value), Unit: unit, Samples: samples})
}
