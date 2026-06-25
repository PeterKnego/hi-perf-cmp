// network-rtt benchmark (Go).
//
// Measures synchronous request/response round-trip latency for both TCP and
// UDP transports, using a strict ping-pong loop (one request outstanding at a
// time). It supports three modes selected by RTT_MODE:
//
//   - loopback (default): an in-process echo server on an ephemeral 127.0.0.1
//     port plus a client. Emits the six result-contract JSON lines.
//   - server: bind TCP and UDP echo responders on 0.0.0.0 at the configured
//     ports and serve until killed. Emits nothing to stdout (logs to stderr).
//   - client: connect to RTT_HOST on both ports, measure, and emit the six
//     result-contract JSON lines.
//
// All logs and errors go to stderr; only result lines go to stdout. See
// docs/result-contract.md and the network-rtt design spec for details.
package main

import (
	"fmt"
	"net"
	"os"
	"sort"
	"strconv"

	"github.com/peterknego/hi-perf-cmp/go/internal/result"
)

func main() {
	cfg, err := loadConfig()
	if err != nil {
		fatalf("%v", err)
	}

	switch cfg.Mode {
	case ModeLoopback:
		runLoopback(cfg)
	case ModeServer:
		runServer(cfg)
	case ModeClient:
		runClient(cfg)
	default:
		fatalf("unknown mode %q", cfg.Mode)
	}
}

// runLoopback starts in-process TCP and UDP echo servers on ephemeral
// 127.0.0.1 ports, measures against each, and emits the six result lines.
func runLoopback(cfg Config) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		fatalf("tcp: listen: %v", err)
	}
	defer ln.Close()
	go func() { _ = tcpServe(ln, cfg.PayloadBytes) }()

	udpAddr, err := net.ResolveUDPAddr("udp", "127.0.0.1:0")
	if err != nil {
		fatalf("udp: resolve: %v", err)
	}
	udpConn, err := net.ListenUDP("udp", udpAddr)
	if err != nil {
		fatalf("udp: listen: %v", err)
	}
	defer udpConn.Close()
	go func() { _ = udpServe(udpConn, cfg.PayloadBytes) }()

	tcpSamples, err := tcpClient(ln.Addr().String(), cfg)
	if err != nil {
		fatalf("%v", err)
	}
	udpSamples, err := udpClient(udpConn.LocalAddr().String(), cfg)
	if err != nil {
		fatalf("%v", err)
	}

	emit("tcp", tcpSamples)
	emit("udp", udpSamples)
}

// runServer binds TCP and UDP echo responders on 0.0.0.0 at the configured
// ports and serves until the process is killed. It emits nothing to stdout.
func runServer(cfg Config) {
	tcpAddr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.TCPPort))
	ln, err := net.Listen("tcp", tcpAddr)
	if err != nil {
		fatalf("tcp: listen: %v", err)
	}
	defer ln.Close()

	udpAddr, err := net.ResolveUDPAddr("udp", net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.UDPPort)))
	if err != nil {
		fatalf("udp: resolve: %v", err)
	}
	udpConn, err := net.ListenUDP("udp", udpAddr)
	if err != nil {
		fatalf("udp: listen: %v", err)
	}
	defer udpConn.Close()

	logf("serving: tcp %s, udp %s", tcpAddr, udpAddr)

	errc := make(chan error, 2)
	go func() { errc <- tcpServe(ln, cfg.PayloadBytes) }()
	go func() { errc <- udpServe(udpConn, cfg.PayloadBytes) }()

	// Serve until killed; surface the first responder failure if one returns.
	if err := <-errc; err != nil {
		fatalf("server: %v", err)
	}
}

// runClient connects to RTT_HOST on both ports, measures, and emits the six
// result lines.
func runClient(cfg Config) {
	tcpAddr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.TCPPort))
	udpAddr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.UDPPort))

	tcpSamples, err := tcpClient(tcpAddr, cfg)
	if err != nil {
		fatalf("%v", err)
	}
	udpSamples, err := udpClient(udpAddr, cfg)
	if err != nil {
		fatalf("%v", err)
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

// logf writes a diagnostic line to stderr (never stdout).
func logf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "network-rtt: "+format+"\n", args...)
}

// fatalf logs to stderr and exits with status 1.
func fatalf(format string, args ...any) {
	logf(format, args...)
	os.Exit(1)
}
