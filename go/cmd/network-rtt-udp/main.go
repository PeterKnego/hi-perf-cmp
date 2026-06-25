// network-rtt-udp benchmark (Go).
//
// Measures synchronous request/response round-trip latency over UDP using a
// strict ping-pong loop (one request outstanding at a time). It supports three
// modes selected by RTT_MODE:
//
//   - loopback (default): an in-process echo server on an ephemeral 127.0.0.1
//     port plus a client. Emits the three result-contract JSON lines.
//   - server: bind a UDP echo responder on 0.0.0.0 at RTT_UDP_PORT and serve
//     until killed. Emits nothing to stdout (logs to stderr).
//   - client: connect to RTT_HOST:RTT_UDP_PORT, measure, and emit the three
//     result-contract JSON lines.
//
// The link under test is expected to be effectively lossless; each recv carries
// a 1s deadline and a timeout is treated as a hard error, not a retransmit.
//
// All logs and errors go to stderr; only result lines go to stdout. See
// docs/result-contract.md and the experiment-dimension design spec for details.
package main

import (
	"bytes"
	"errors"
	"fmt"
	"net"
	"strconv"
	"syscall"
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "udp"

// udpReadTimeout is the per-recv deadline. A timeout is treated as a hard
// error, not a retransmit.
const udpReadTimeout = time.Second

// serveSpinBudget is the maximum number of EAGAIN spins in the raw-fd read
// loop before yielding back to the netpoller.
const serveSpinBudget = 2048

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

// runLoopback starts an in-process UDP echo server on an ephemeral 127.0.0.1
// port, measures against it, and emits the three result lines.
func runLoopback(cfg bench.Config) {
	udpAddr, err := net.ResolveUDPAddr("udp", "127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "udp: resolve: %v", err)
	}
	conn, err := net.ListenUDP("udp", udpAddr)
	if err != nil {
		bench.Fatalf(prog(), "udp: listen: %v", err)
	}
	defer conn.Close()
	go func() { _ = serve(conn, cfg.PayloadBytes) }()

	samples, err := client(conn.LocalAddr().String(), cfg)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRTT(experiment, samples)
}

// runServer binds a UDP echo responder on 0.0.0.0 at RTT_UDP_PORT and serves
// until the process is killed. It emits nothing to stdout.
func runServer(cfg bench.Config) {
	udpAddr, err := net.ResolveUDPAddr("udp", net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.UDPPort)))
	if err != nil {
		bench.Fatalf(prog(), "udp: resolve: %v", err)
	}
	conn, err := net.ListenUDP("udp", udpAddr)
	if err != nil {
		bench.Fatalf(prog(), "udp: listen: %v", err)
	}
	defer conn.Close()

	bench.Logf(prog(), "serving: udp %s", udpAddr)
	if err := serve(conn, cfg.PayloadBytes); err != nil {
		bench.Fatalf(prog(), "server: %v", err)
	}
}

// runClient connects to RTT_HOST:RTT_UDP_PORT, measures, and emits the three
// result lines.
func runClient(cfg bench.Config) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.UDPPort))
	samples, err := client(addr, cfg)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRTT(experiment, samples)
}

// client dials a connected UDP socket to addr and runs warmup then
// cfg.Iterations strict ping-pong round trips, returning per-round-trip
// elapsed nanoseconds. Each recv carries a 1s deadline; a timeout is a hard
// error. The echoed bytes are asserted equal to the sent bytes.
func client(addr string, cfg bench.Config) ([]int64, error) {
	raddr, err := net.ResolveUDPAddr("udp", addr)
	if err != nil {
		return nil, fmt.Errorf("udp: resolve: %w", err)
	}

	// DialUDP connects the socket so plain Read/Write work.
	conn, err := net.DialUDP("udp", nil, raddr)
	if err != nil {
		return nil, fmt.Errorf("udp: dial: %w", err)
	}
	defer conn.Close()

	rc, err := conn.SyscallConn()
	if err != nil {
		return nil, fmt.Errorf("udp: rawconn: %w", err)
	}
	errRecvTimeout := errors.New("udp recv timed out (datagram loss)")

	send := cfg.Payload()
	recv := make([]byte, cfg.PayloadBytes)

	roundTrip := func() error {
		if _, err := conn.Write(send); err != nil {
			return fmt.Errorf("udp: write: %w", err)
		}
		deadline := time.Now().Add(udpReadTimeout)
		var n int
		var rerr error
		cberr := rc.Read(func(fd uintptr) bool {
			for spins := 0; ; spins++ {
				nn, e := syscall.Read(int(fd), recv)
				if e == syscall.EINTR {
					continue
				}
				if e == syscall.EAGAIN {
					if spins&255 == 0 && time.Now().After(deadline) {
						rerr = errRecvTimeout
						return true // never park: return from rc.Read
					}
					continue
				}
				n, rerr = nn, e
				return true
			}
		})
		if cberr != nil {
			return fmt.Errorf("udp: rawread: %w", cberr)
		}
		if rerr != nil {
			return fmt.Errorf("udp: read: %w", rerr)
		}
		if n != len(send) || !bytes.Equal(recv[:n], send) {
			return fmt.Errorf("udp: echo mismatch")
		}
		return nil
	}

	return bench.Measure(cfg, roundTrip)
}

// serve reads datagrams on conn and echoes them back to the sender, serving
// until conn is closed. It returns only when a read fails (e.g. the conn was
// closed).
func serve(conn *net.UDPConn, payloadBytes int) error {
	buf := make([]byte, payloadBytes)
	rc, err := conn.SyscallConn()
	if err != nil {
		return err
	}
	for {
		var rerr error
		err := rc.Read(func(fd uintptr) bool {
			for spins := 0; ; spins++ {
				n, from, e := syscall.Recvfrom(int(fd), buf, 0)
				if e == syscall.EINTR {
					continue
				}
				if e == syscall.EAGAIN {
					if spins >= serveSpinBudget {
						return false // park on netpoller; re-invoked when readable
					}
					continue
				}
				if e != nil {
					rerr = e
					return true
				}
				// echo straight back to sender
				if se := syscall.Sendto(int(fd), buf[:n], 0, from); se != nil {
					bench.Logf(prog(), "udp echo: write: %v", se)
				}
				return true
			}
		})
		if err != nil {
			return err
		}
		if rerr != nil {
			return rerr
		}
	}
}
