// rpc-roundtrip-bebop_tcp: TCP transport + bebop safe-API codec. The responder
// deserializes each request, increments Hop, and re-serializes the reply; the
// client verifies resp.Hop == req.Hop+1 and resp.Seq == req.Seq. Framing is a
// 4-byte big-endian length prefix + body. One request outstanding at a time.
package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"strconv"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload"
)

const experiment = "bebop_tcp"

func prog() string { return "rpc-roundtrip-" + experiment }

func main() {
	cfg, err := bench.LoadRpcConfig()
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

func runLoopback(cfg bench.RpcConfig) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	defer ln.Close()
	go func() { _ = serve(ln) }()
	measureAndEmit(ln.Addr().String(), cfg)
}

func runServer(cfg bench.RpcConfig) {
	addr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.TCPPort))
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	defer ln.Close()
	bench.Logf(prog(), "serving tcp %s", addr)
	if err := serve(ln); err != nil {
		bench.Fatalf(prog(), "serve: %v", err)
	}
}

func runClient(cfg bench.RpcConfig) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.TCPPort))
	measureAndEmit(addr, cfg)
}

func measureAndEmit(addr string, cfg bench.RpcConfig) {
	req := rpcpayload.BuildRecord(0)
	bebopReq := rpcpayload.ToBebop(&req)

	conn, err := net.Dial("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "dial: %v", err)
	}
	defer conn.Close()
	if tc, ok := conn.(*net.TCPConn); ok {
		_ = tc.SetNoDelay(true)
	}

	sendBody := make([]byte, 64*1024)
	sendFrame := make([]byte, 4+64*1024)
	recvHdr := make([]byte, 4)
	recvBody := make([]byte, 64*1024)

	roundTrip := func() error {
		n := rpcpayload.EncodeBebop(bebopReq, sendBody)
		binary.BigEndian.PutUint32(sendFrame, uint32(n))
		copy(sendFrame[4:], sendBody[:n])
		if _, err := conn.Write(sendFrame[:4+n]); err != nil {
			return fmt.Errorf("write: %w", err)
		}
		if _, err := io.ReadFull(conn, recvHdr); err != nil {
			return fmt.Errorf("read hdr: %w", err)
		}
		m := int(binary.BigEndian.Uint32(recvHdr))
		if m > len(recvBody) {
			return fmt.Errorf("reply too large: %d", m)
		}
		if _, err := io.ReadFull(conn, recvBody[:m]); err != nil {
			return fmt.Errorf("read body: %w", err)
		}
		resp, err := rpcpayload.DecodeBebop(recvBody[:m])
		if err != nil {
			return fmt.Errorf("decode: %w", err)
		}
		if resp.Hop != req.Hop+1 || resp.Seq != req.Seq {
			return fmt.Errorf("verification failed: hop=%d seq=%d", resp.Hop, resp.Seq)
		}
		return nil
	}

	samples, err := bench.MeasureN(cfg.Warmup, cfg.Iterations, roundTrip)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRoundtrip(experiment, samples)
	encoded := rpcpayload.EncodeBebop(bebopReq, sendBody)
	bench.EmitRoundtripInt(experiment, "encoded_bytes", int64(encoded), "bytes", 1)
}

func serve(ln net.Listener) error {
	for {
		conn, err := ln.Accept()
		if err != nil {
			return err
		}
		go handle(conn)
	}
}

// handle reads length-prefixed requests, increments Hop, and writes the reply.
func handle(conn net.Conn) {
	defer conn.Close()
	if tc, ok := conn.(*net.TCPConn); ok {
		_ = tc.SetNoDelay(true)
	}
	hdr := make([]byte, 4)
	body := make([]byte, 64*1024)
	out := make([]byte, 64*1024)
	frame := make([]byte, 4+64*1024)
	for {
		if _, err := io.ReadFull(conn, hdr); err != nil {
			return // client closed; normal
		}
		n := int(binary.BigEndian.Uint32(hdr))
		if n > len(body) {
			bench.Logf(prog(), "request too large: %d", n)
			return
		}
		if _, err := io.ReadFull(conn, body[:n]); err != nil {
			return
		}
		d, err := rpcpayload.DecodeBebop(body[:n])
		if err != nil {
			bench.Logf(prog(), "decode: %v", err)
			return
		}
		d.Hop++ // mutate
		m := rpcpayload.EncodeBebop(d, out)
		binary.BigEndian.PutUint32(frame, uint32(m))
		copy(frame[4:], out[:m])
		if _, err := conn.Write(frame[:4+m]); err != nil {
			bench.Logf(prog(), "write: %v", err)
			return
		}
	}
}
