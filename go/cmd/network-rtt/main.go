// network-rtt benchmark (Go).
//
// Measures synchronous request/response round-trip latency over loopback, for
// both TCP and UDP transports, using an in-process echo server and a single
// client connection (strict ping-pong, one request outstanding at a time).
//
// Emits six result-contract JSON lines on stdout (results only; all logs and
// errors go to stderr). See docs/result-contract.md and the network-rtt design
// spec for details.
package main

import (
	"fmt"
	"os"
	"sort"

	"github.com/peterknego/hi-perf-cmp/go/internal/result"
)

func main() {
	cfg, err := loadConfig()
	if err != nil {
		fmt.Fprintf(os.Stderr, "network-rtt: %v\n", err)
		os.Exit(1)
	}

	tcpSamples, err := measureTCP(cfg)
	if err != nil {
		fmt.Fprintf(os.Stderr, "network-rtt: %v\n", err)
		os.Exit(1)
	}

	udpSamples, err := measureUDP(cfg)
	if err != nil {
		fmt.Fprintf(os.Stderr, "network-rtt: %v\n", err)
		os.Exit(1)
	}

	emit("tcp", tcpSamples)
	emit("udp", udpSamples)
}

// emit sorts the samples and emits p50, p99 and mean result lines for the
// given transport prefix (e.g. "tcp" -> tcp_rtt_p50, tcp_rtt_p99, tcp_rtt_mean).
func emit(transport string, samples []int64) {
	sort.Slice(samples, func(i, j int) bool { return samples[i] < samples[j] })
	n := int64(len(samples))

	result.Emit(result.Result{
		FocusArea: "network-rtt",
		Metric:    transport + "_rtt_p50",
		Value:     float64(percentile(samples, 50)),
		Unit:      "ns",
		Samples:   n,
	})
	result.Emit(result.Result{
		FocusArea: "network-rtt",
		Metric:    transport + "_rtt_p99",
		Value:     float64(percentile(samples, 99)),
		Unit:      "ns",
		Samples:   n,
	})
	result.Emit(result.Result{
		FocusArea: "network-rtt",
		Metric:    transport + "_rtt_mean",
		Value:     mean(samples),
		Unit:      "ns",
		Samples:   n,
	})
}
