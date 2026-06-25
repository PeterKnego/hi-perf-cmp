package main

import (
	"bytes"
	"fmt"
	"net"
	"time"
)

// udpReadTimeout is the per-recv deadline. Loopback UDP is effectively
// lossless; a timeout is treated as a hard error, not a retransmit.
const udpReadTimeout = time.Second

// measureUDP starts an in-process UDP echo server on loopback, dials one
// connected client socket, runs warmup then cfg.Iterations strict ping-pong
// round trips, and returns the per-round-trip elapsed nanoseconds.
func measureUDP(cfg Config) ([]int64, error) {
	srvAddr, err := net.ResolveUDPAddr("udp", "127.0.0.1:0")
	if err != nil {
		return nil, fmt.Errorf("udp: resolve: %w", err)
	}
	srvConn, err := net.ListenUDP("udp", srvAddr)
	if err != nil {
		return nil, fmt.Errorf("udp: listen: %w", err)
	}
	defer srvConn.Close()

	srvErr := make(chan error, 1)
	stop := make(chan struct{})
	go udpEchoServer(srvConn, cfg.PayloadBytes, stop, srvErr)

	// DialUDP connects the socket so plain Read/Write work.
	cliConn, err := net.DialUDP("udp", nil, srvConn.LocalAddr().(*net.UDPAddr))
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

	// Stop the server and surface any server-side error.
	close(stop)
	srvConn.Close()
	if err := <-srvErr; err != nil {
		return nil, err
	}
	return samples, nil
}

// udpEchoServer reads datagrams and echoes them back to the sender until
// stopped (signalled by closing stop, then closing the conn).
func udpEchoServer(conn *net.UDPConn, payloadBytes int, stop <-chan struct{}, errc chan<- error) {
	buf := make([]byte, payloadBytes)
	for {
		n, addr, err := conn.ReadFromUDP(buf)
		if err != nil {
			select {
			case <-stop:
				errc <- nil // expected shutdown
			default:
				errc <- fmt.Errorf("udp echo: read: %w", err)
			}
			return
		}
		if _, err := conn.WriteToUDP(buf[:n], addr); err != nil {
			select {
			case <-stop:
				errc <- nil
			default:
				errc <- fmt.Errorf("udp echo: write: %w", err)
			}
			return
		}
	}
}
