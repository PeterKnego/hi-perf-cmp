// rpc-roundtrip-grpc: gRPC (HTTP/2 + protobuf) transport. One unary Roundtrip
// call is the round trip; the server handler increments Hop and returns. The
// client verifies resp.Hop == req.Hop+1 and resp.Seq == req.Seq.
package main

import (
	"context"
	"fmt"
	"net"
	"strconv"
	"time"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload"
	"github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadpb"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/protobuf/proto"
)

const experiment = "grpc"

func prog() string { return "rpc-roundtrip-" + experiment }

type server struct {
	payloadpb.UnimplementedRpcRoundtripServer
}

// Roundtrip mutates the request (Hop+1) and returns it — the deserialize +
// mutate + reserialize the focus area measures (gRPC owns the codec + framing).
func (server) Roundtrip(_ context.Context, in *payloadpb.RpcPayload) (*payloadpb.RpcPayload, error) {
	in.Hop++
	return in, nil
}

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
	lis, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	s := grpc.NewServer()
	payloadpb.RegisterRpcRoundtripServer(s, server{})
	go func() { _ = s.Serve(lis) }()
	defer s.Stop()
	measureAndEmit(lis.Addr().String(), cfg)
}

func runServer(cfg bench.RpcConfig) {
	addr := net.JoinHostPort("0.0.0.0", strconv.Itoa(cfg.GRPCPort))
	lis, err := net.Listen("tcp", addr)
	if err != nil {
		bench.Fatalf(prog(), "listen: %v", err)
	}
	s := grpc.NewServer()
	payloadpb.RegisterRpcRoundtripServer(s, server{})
	bench.Logf(prog(), "serving grpc %s", addr)
	if err := s.Serve(lis); err != nil {
		bench.Fatalf(prog(), "serve: %v", err)
	}
}

func runClient(cfg bench.RpcConfig) {
	addr := net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.GRPCPort))
	measureAndEmit(addr, cfg)
}

func measureAndEmit(addr string, cfg bench.RpcConfig) {
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		bench.Fatalf(prog(), "dial: %v", err)
	}
	defer conn.Close()
	client := payloadpb.NewRpcRoundtripClient(conn)

	rec := rpcpayload.BuildRecord(0)
	req := rpcpayload.ToProto(&rec)
	ctx := context.Background()

	roundTrip := func() error {
		callCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
		resp, err := client.Roundtrip(callCtx, req)
		cancel()
		if err != nil {
			return fmt.Errorf("roundtrip: %w", err)
		}
		if resp.Hop != rec.Hop+1 || resp.Seq != rec.Seq {
			return fmt.Errorf("verification failed: hop=%d seq=%d", resp.Hop, resp.Seq)
		}
		return nil
	}

	samples, err := bench.MeasureN(cfg.Warmup, cfg.Iterations, roundTrip)
	if err != nil {
		bench.Fatalf(prog(), "%v", err)
	}
	bench.EmitRoundtrip(experiment, samples)
	encoded, _ := protoSize(req)
	bench.EmitRoundtripInt(experiment, "encoded_bytes", int64(encoded), "bytes", 1)
}

func protoSize(m *payloadpb.RpcPayload) (int, error) {
	out, err := proto.Marshal(m)
	return len(out), err
}
