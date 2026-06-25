// network-rtt-quic benchmark (Go).
//
// Measures synchronous request/response round-trip latency over QUIC, mirroring
// the TCP methodology for comparability: one connection, one long-lived
// bidirectional stream, strict ping-pong (write payload, read the full echo
// back), one request outstanding at a time. It supports three modes selected by
// RTT_MODE:
//
//   - loopback (default): an in-process echo server on an ephemeral 127.0.0.1
//     port plus a client. Emits the three result-contract JSON lines.
//   - server: bind a QUIC echo responder on 0.0.0.0 at RTT_QUIC_PORT and serve
//     until killed. Emits nothing to stdout (logs to stderr).
//   - client: connect to RTT_HOST:RTT_QUIC_PORT, measure, and emit the three
//     result-contract JSON lines.
//
// QUIC requires TLS; for a loopback/private-network latency benchmark the server
// generates an in-memory self-signed certificate at startup and the client skips
// verification (insecure is acceptable here — we measure latency, not security).
// A fixed ALPN ("hperf-rtt") is used on both ends.
//
// All logs and errors go to stderr; only result lines go to stdout. See
// docs/result-contract.md and the experiment-dimension design spec for details.
package main

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"fmt"
	"io"
	"math/big"
	"net"
	"strconv"
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/quic-go/quic-go"
)

const experiment = "quic"

// alpn is the fixed application-layer protocol negotiation identifier used by
// both the server and client.
const alpn = "hperf-rtt"

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

// runLoopback starts an in-process QUIC echo server on an ephemeral 127.0.0.1
// port, measures against it, and emits the three result lines.
func runLoopback(cfg bench.Config) {
	ln, err := listen("127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	defer ln.Close()
	go func() { _ = serve(ln, cfg.PayloadBytes) }()

	samples, err := client(ln.Addr().String(), cfg)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRTT(experiment, samples)
}

// runServer binds a QUIC echo responder on 0.0.0.0 at RTT_QUIC_PORT and serves
// until the process is killed. It emits nothing to stdout.
func runServer(cfg bench.Config) {
	addr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.QUICPort))
	ln, err := listen(addr)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	defer ln.Close()

	bench.Logf(prog(), "serving: quic %s", addr)
	if err := serve(ln, cfg.PayloadBytes); err != nil {
		bench.Fatalf(prog(), "server: %v", err)
	}
}

// runClient connects to RTT_HOST:RTT_QUIC_PORT, measures, and emits the three
// result lines.
func runClient(cfg bench.Config) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.QUICPort))
	samples, err := client(addr, cfg)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRTT(experiment, samples)
}

// listen opens a QUIC listener at addr with a fresh in-memory self-signed
// certificate and the fixed ALPN.
func listen(addr string) (*quic.Listener, error) {
	tlsConf, err := serverTLSConfig()
	if err != nil {
		return nil, fmt.Errorf("quic: tls config: %w", err)
	}
	ln, err := quic.ListenAddr(addr, tlsConf, nil)
	if err != nil {
		return nil, fmt.Errorf("quic: listen: %w", err)
	}
	return ln, nil
}

// client dials a QUIC connection to addr, opens one long-lived bidirectional
// stream, and runs warmup then cfg.Iterations strict ping-pong round trips,
// returning per-round-trip elapsed nanoseconds. One request is outstanding at a
// time. The echoed bytes are asserted equal to the sent bytes.
func client(addr string, cfg bench.Config) ([]int64, error) {
	tlsConf := &tls.Config{
		InsecureSkipVerify: true, //nolint:gosec // latency benchmark, not security
		NextProtos:         []string{alpn},
	}

	conn, err := quic.DialAddr(context.Background(), addr, tlsConf, nil)
	if err != nil {
		return nil, fmt.Errorf("quic: dial: %w", err)
	}
	defer conn.CloseWithError(0, "done")

	// One long-lived bidirectional stream; the server starts echoing only once
	// the first bytes arrive, so we must write before AcceptStream completes on
	// the peer.
	stream, err := conn.OpenStreamSync(context.Background())
	if err != nil {
		return nil, fmt.Errorf("quic: open stream: %w", err)
	}
	defer stream.Close()

	send := cfg.Payload()
	recv := make([]byte, cfg.PayloadBytes)

	roundTrip := func() error {
		if _, err := stream.Write(send); err != nil {
			return fmt.Errorf("quic: write: %w", err)
		}
		if _, err := io.ReadFull(stream, recv); err != nil {
			return fmt.Errorf("quic: read: %w", err)
		}
		if !bytes.Equal(recv, send) {
			return fmt.Errorf("quic: echo mismatch")
		}
		return nil
	}

	return bench.Measure(cfg, roundTrip)
}

// serve accepts QUIC connections on ln and echoes the bytes of each accepted
// bidirectional stream back, serving until ln is closed. Each connection is
// handled in its own goroutine.
func serve(ln *quic.Listener, payloadBytes int) error {
	for {
		conn, err := ln.Accept(context.Background())
		if err != nil {
			return err
		}
		go echoConn(conn, payloadBytes)
	}
}

// echoConn accepts the client's long-lived bidirectional stream and echoes
// fixed-size payloads back until the stream ends or an error occurs. Errors are
// logged to stderr.
func echoConn(conn quic.Connection, payloadBytes int) {
	stream, err := conn.AcceptStream(context.Background())
	if err != nil {
		bench.Logf(prog(), "quic echo: accept stream: %v", err)
		return
	}
	defer stream.Close()

	buf := make([]byte, payloadBytes)
	for {
		if _, err := io.ReadFull(stream, buf); err != nil {
			if err == io.EOF || err == io.ErrUnexpectedEOF {
				return // client closed the stream; normal
			}
			bench.Logf(prog(), "quic echo: read: %v", err)
			return
		}
		if _, err := stream.Write(buf); err != nil {
			bench.Logf(prog(), "quic echo: write: %v", err)
			return
		}
	}
}

// serverTLSConfig builds a tls.Config with a fresh in-memory self-signed
// certificate and the fixed ALPN, suitable for the QUIC echo server.
func serverTLSConfig() (*tls.Config, error) {
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return nil, fmt.Errorf("generate key: %w", err)
	}

	serial, err := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 128))
	if err != nil {
		return nil, fmt.Errorf("serial: %w", err)
	}

	tmpl := x509.Certificate{
		SerialNumber: serial,
		Subject:      pkix.Name{CommonName: "hperf-rtt"},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(24 * time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature | x509.KeyUsageKeyEncipherment,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		DNSNames:     []string{"localhost"},
		IPAddresses:  []net.IP{net.IPv4(127, 0, 0, 1), net.IPv6loopback},
	}

	der, err := x509.CreateCertificate(rand.Reader, &tmpl, &tmpl, &key.PublicKey, key)
	if err != nil {
		return nil, fmt.Errorf("create certificate: %w", err)
	}

	cert := tls.Certificate{
		Certificate: [][]byte{der},
		PrivateKey:  key,
	}
	return &tls.Config{
		Certificates: []tls.Certificate{cert},
		NextProtos:   []string{alpn},
	}, nil
}
