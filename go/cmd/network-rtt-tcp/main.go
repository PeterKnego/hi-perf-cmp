// network-rtt-tcp benchmark (Go).
//
// Measures synchronous request/response round-trip latency over TCP using a
// strict ping-pong loop (one request outstanding at a time). It supports three
// modes selected by RTT_MODE:
//
//   - loopback (default): an in-process echo server on an ephemeral 127.0.0.1
//     port plus a client. Emits the three result-contract JSON lines.
//   - server: bind a TCP echo responder on 0.0.0.0 at RTT_TCP_PORT and serve
//     until killed. Emits nothing to stdout (logs to stderr).
//   - client: connect to RTT_HOST:RTT_TCP_PORT, measure, and emit the three
//     result-contract JSON lines.
//
// All logs and errors go to stderr; only result lines go to stdout. See
// docs/result-contract.md and the experiment-dimension design spec for details.
package main

import (
	"bytes"
	"fmt"
	"io"
	"net"
	"strconv"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "tcp"

func main() {
	cfg, err := bench.LoadConfig()
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}

	switch cfg.Mode {
	case bench.ModeLoopback:
		runLoopback(cfg)
	case bench.ModeServer:
		runServer(cfg)
	case bench.ModeClient:
		runClient(cfg)
	default:
		bench.Fatalf(prog(), "unknown mode %q", cfg.Mode)
	}
}

func prog() string { return "network-rtt-" + experiment }

// runLoopback starts an in-process TCP echo server on an ephemeral 127.0.0.1
// port, measures against it, and emits the three result lines.
func runLoopback(cfg bench.Config) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "tcp: listen: %v", err)
	}
	defer ln.Close()
	go func() { _ = serve(ln, cfg.PayloadBytes) }()

	samples, err := client(ln.Addr().String(), cfg)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRTT(experiment, samples)
}

// runServer binds a TCP echo responder on 0.0.0.0 at RTT_TCP_PORT and serves
// until the process is killed. It emits nothing to stdout.
func runServer(cfg bench.Config) {
	addr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.TCPPort))
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "tcp: listen: %v", err)
	}
	defer ln.Close()

	bench.Logf(prog(), "serving: tcp %s", addr)
	if err := serve(ln, cfg.PayloadBytes); err != nil {
		bench.Fatalf(prog(), "server: %v", err)
	}
}

// runClient connects to RTT_HOST:RTT_TCP_PORT, measures, and emits the three
// result lines.
func runClient(cfg bench.Config) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.TCPPort))
	samples, err := client(addr, cfg)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRTT(experiment, samples)
}

// client connects to addr with TCP_NODELAY set and runs warmup then
// cfg.Iterations strict ping-pong round trips, returning per-round-trip
// elapsed nanoseconds. One request is outstanding at a time. The echoed bytes
// are asserted equal to the sent bytes.
func client(addr string, cfg bench.Config) ([]int64, error) {
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("tcp: dial: %w", err)
	}
	defer conn.Close()

	tcpConn, ok := conn.(*net.TCPConn)
	if !ok {
		return nil, fmt.Errorf("tcp: expected *net.TCPConn, got %T", conn)
	}
	if err := tcpConn.SetNoDelay(true); err != nil {
		return nil, fmt.Errorf("tcp: set nodelay: %w", err)
	}

	send := cfg.Payload()
	recv := make([]byte, cfg.PayloadBytes)

	roundTrip := func() error {
		if _, err := tcpConn.Write(send); err != nil {
			return fmt.Errorf("tcp: write: %w", err)
		}
		if _, err := io.ReadFull(tcpConn, recv); err != nil {
			return fmt.Errorf("tcp: read: %w", err)
		}
		if !bytes.Equal(recv, send) {
			return fmt.Errorf("tcp: echo mismatch")
		}
		return nil
	}

	return bench.Measure(cfg, roundTrip)
}

// serve accepts connections on ln and echoes fixed-size payloads back to every
// client, serving until ln is closed. Each accepted connection is handled in
// its own goroutine.
func serve(ln net.Listener, payloadBytes int) error {
	for {
		conn, err := ln.Accept()
		if err != nil {
			return err
		}
		go echoConn(conn, payloadBytes)
	}
}

// echoConn echoes fixed-size payloads back on conn until the client
// disconnects or an error occurs. Errors are logged to stderr.
func echoConn(conn net.Conn, payloadBytes int) {
	defer conn.Close()

	if tcpConn, ok := conn.(*net.TCPConn); ok {
		_ = tcpConn.SetNoDelay(true)
	}

	buf := make([]byte, payloadBytes)
	for {
		if _, err := io.ReadFull(conn, buf); err != nil {
			if err == io.EOF || err == io.ErrUnexpectedEOF {
				return // client disconnected; normal
			}
			bench.Logf(prog(), "tcp echo: read: %v", err)
			return
		}
		if _, err := conn.Write(buf); err != nil {
			bench.Logf(prog(), "tcp echo: write: %v", err)
			return
		}
	}
}
