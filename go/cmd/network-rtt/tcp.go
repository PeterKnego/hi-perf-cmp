package main

import (
	"bytes"
	"fmt"
	"io"
	"net"
	"time"
)

// measureTCP starts an in-process TCP echo server on loopback, connects one
// client with TCP_NODELAY set, runs warmup then cfg.Iterations strict
// ping-pong round trips, and returns the per-round-trip elapsed nanoseconds.
func measureTCP(cfg Config) ([]int64, error) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return nil, fmt.Errorf("tcp: listen: %w", err)
	}
	defer ln.Close()

	srvErr := make(chan error, 1)
	go tcpEchoServer(ln, cfg.PayloadBytes, srvErr)

	conn, err := net.Dial("tcp", ln.Addr().String())
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

	// Close the client so the server's echo loop sees EOF and exits cleanly,
	// then surface any server-side error that isn't a normal close.
	conn.Close()
	ln.Close()
	if err := <-srvErr; err != nil {
		return nil, err
	}
	return samples, nil
}

// tcpEchoServer accepts one connection and echoes fixed-size payloads back
// until the client disconnects.
func tcpEchoServer(ln net.Listener, payloadBytes int, errc chan<- error) {
	conn, err := ln.Accept()
	if err != nil {
		errc <- nil // listener closed; not an error worth reporting
		return
	}
	defer conn.Close()

	buf := make([]byte, payloadBytes)
	for {
		if _, err := io.ReadFull(conn, buf); err != nil {
			if err == io.EOF || err == io.ErrUnexpectedEOF {
				errc <- nil
				return
			}
			errc <- fmt.Errorf("tcp echo: read: %w", err)
			return
		}
		if _, err := conn.Write(buf); err != nil {
			errc <- fmt.Errorf("tcp echo: write: %w", err)
			return
		}
	}
}
