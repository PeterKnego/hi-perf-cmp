package main

import (
	"bytes"
	"fmt"
	"net"
	"time"
)

// udpReadTimeout is the per-recv deadline. The link under test is expected to
// be effectively lossless; a timeout is treated as a hard error, not a
// retransmit.
const udpReadTimeout = time.Second

// udpClient dials a connected UDP socket to addr, runs warmup then
// cfg.Iterations strict ping-pong round trips, and returns the per-round-trip
// elapsed nanoseconds. Each recv carries a 1s deadline; a timeout is a hard
// error. The echoed bytes are asserted equal to the sent bytes.
func udpClient(addr string, cfg Config) ([]int64, error) {
	raddr, err := net.ResolveUDPAddr("udp", addr)
	if err != nil {
		return nil, fmt.Errorf("udp: resolve: %w", err)
	}

	// DialUDP connects the socket so plain Read/Write work.
	cliConn, err := net.DialUDP("udp", nil, raddr)
	if err != nil {
		return nil, fmt.Errorf("udp: dial: %w", err)
	}
	defer cliConn.Close()

	send := make([]byte, cfg.PayloadBytes)
	for i := range send {
		send[i] = byte(i)
	}
	recv := make([]byte, cfg.PayloadBytes)

	roundTrip := func() error {
		if _, err := cliConn.Write(send); err != nil {
			return fmt.Errorf("udp: write: %w", err)
		}
		if err := cliConn.SetReadDeadline(time.Now().Add(udpReadTimeout)); err != nil {
			return fmt.Errorf("udp: set deadline: %w", err)
		}
		n, err := cliConn.Read(recv)
		if err != nil {
			return fmt.Errorf("udp: read: %w", err)
		}
		if n != len(send) || !bytes.Equal(recv[:n], send) {
			return fmt.Errorf("udp: echo mismatch")
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

// udpServe reads datagrams on conn and echoes them back to the sender,
// serving until conn is closed. It returns only when a read fails (e.g. the
// conn was closed).
func udpServe(conn *net.UDPConn, payloadBytes int) error {
	buf := make([]byte, payloadBytes)
	for {
		n, addr, err := conn.ReadFromUDP(buf)
		if err != nil {
			return err
		}
		if _, err := conn.WriteToUDP(buf[:n], addr); err != nil {
			logf("udp echo: write: %v", err)
		}
	}
}
