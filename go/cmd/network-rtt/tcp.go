package main

import (
	"bytes"
	"fmt"
	"io"
	"net"
	"time"
)

// tcpClient connects to addr with TCP_NODELAY set, runs warmup then
// cfg.Iterations strict ping-pong round trips, and returns the per-round-trip
// elapsed nanoseconds. One request is outstanding at a time. The echoed bytes
// are asserted equal to the sent bytes.
func tcpClient(addr string, cfg Config) ([]int64, error) {
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

	send := make([]byte, cfg.PayloadBytes)
	for i := range send {
		send[i] = byte(i)
	}
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

	// Warmup (discarded).
	for i := 0; i < cfg.Warmup; i++ {
		if err := roundTrip(); err != nil {
			return nil, err
		}
	}

	// Measured.
	samples := make([]int64, cfg.Iterations)
	for i := 0; i < cfg.Iterations; i++ {
		start := time.Now()
		if err := roundTrip(); err != nil {
			return nil, err
		}
		samples[i] = time.Since(start).Nanoseconds()
	}

	return samples, nil
}

// tcpServe binds a TCP listener at addr and echoes fixed-size payloads back to
// every client connection, serving until ln is closed. Each accepted
// connection is handled in its own goroutine. It returns only when the
// listener stops accepting (e.g. it was closed).
func tcpServe(ln net.Listener, payloadBytes int) error {
	for {
		conn, err := ln.Accept()
		if err != nil {
			return err
		}
		go tcpEchoConn(conn, payloadBytes)
	}
}

// tcpEchoConn echoes fixed-size payloads back on conn until the client
// disconnects or an error occurs. Errors are logged to stderr.
func tcpEchoConn(conn net.Conn, payloadBytes int) {
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
			logf("tcp echo: read: %v", err)
			return
		}
		if _, err := conn.Write(buf); err != nil {
			logf("tcp echo: write: %v", err)
			return
		}
	}
}
