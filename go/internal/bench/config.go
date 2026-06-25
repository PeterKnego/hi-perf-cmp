package bench

import (
	"fmt"
	"os"
	"strconv"
)

// Mode selects the role this process plays.
type Mode string

const (
	// ModeLoopback runs an in-process echo server on an ephemeral 127.0.0.1
	// port plus a client, emitting the three result lines. This is the default.
	ModeLoopback Mode = "loopback"
	// ModeServer binds an echo responder on 0.0.0.0 and serves until killed,
	// emitting nothing to stdout.
	ModeServer Mode = "server"
	// ModeClient connects to RTT_HOST on this experiment's port, measures, and
	// emits the three result lines.
	ModeClient Mode = "client"
)

// Config holds the benchmark parameters, sourced from env vars with defaults.
// All transport ports are parsed so the same Config serves every experiment;
// each experiment binary selects the port it needs.
type Config struct {
	Mode         Mode
	Host         string
	TCPPort      int
	UDPPort      int
	QUICPort     int
	PayloadBytes int
	Warmup       int
	Iterations   int
}

// LoadConfig reads RTT_MODE, RTT_HOST, RTT_TCP_PORT, RTT_UDP_PORT,
// RTT_QUIC_PORT, RTT_PAYLOAD_BYTES, RTT_WARMUP and RTT_ITERATIONS from the
// environment, applying defaults. Invalid values, or a missing RTT_HOST in
// client mode, yield an error.
func LoadConfig() (Config, error) {
	mode, err := loadMode("RTT_MODE", ModeLoopback)
	if err != nil {
		return Config{}, err
	}

	tcpPort, err := positiveEnv("RTT_TCP_PORT", 9100)
	if err != nil {
		return Config{}, err
	}
	udpPort, err := positiveEnv("RTT_UDP_PORT", 9101)
	if err != nil {
		return Config{}, err
	}
	quicPort, err := positiveEnv("RTT_QUIC_PORT", 9102)
	if err != nil {
		return Config{}, err
	}
	payload, err := positiveEnv("RTT_PAYLOAD_BYTES", 64)
	if err != nil {
		return Config{}, err
	}
	warmup, err := positiveEnv("RTT_WARMUP", 10000)
	if err != nil {
		return Config{}, err
	}
	iterations, err := positiveEnv("RTT_ITERATIONS", 100000)
	if err != nil {
		return Config{}, err
	}

	host := os.Getenv("RTT_HOST")
	if mode == ModeClient && host == "" {
		return Config{}, fmt.Errorf("RTT_HOST: required in client mode")
	}

	return Config{
		Mode:         mode,
		Host:         host,
		TCPPort:      tcpPort,
		UDPPort:      udpPort,
		QUICPort:     quicPort,
		PayloadBytes: payload,
		Warmup:       warmup,
		Iterations:   iterations,
	}, nil
}

// loadMode parses env var name as a Mode, returning def when unset/empty.
// Returns an error for any unrecognized value.
func loadMode(name string, def Mode) (Mode, error) {
	s := os.Getenv(name)
	if s == "" {
		return def, nil
	}
	switch Mode(s) {
	case ModeLoopback, ModeServer, ModeClient:
		return Mode(s), nil
	default:
		return "", fmt.Errorf("%s: %q is not a valid mode (want loopback, server or client)", name, s)
	}
}

// positiveEnv parses env var name as a positive integer, returning def when
// unset/empty. Returns an error for non-integer or non-positive values.
func positiveEnv(name string, def int) (int, error) {
	s := os.Getenv(name)
	if s == "" {
		return def, nil
	}
	v, err := strconv.Atoi(s)
	if err != nil {
		return 0, fmt.Errorf("%s: %q is not a valid integer", name, s)
	}
	if v <= 0 {
		return 0, fmt.Errorf("%s: must be a positive integer, got %d", name, v)
	}
	return v, nil
}

// Payload builds the fixed request payload of PayloadBytes, filled with a
// deterministic pattern so the echo can be asserted equal on the client.
func (c Config) Payload() []byte {
	send := make([]byte, c.PayloadBytes)
	for i := range send {
		send[i] = byte(i)
	}
	return send
}
