package bench

import "testing"

func TestLoadRpcConfigDefaults(t *testing.T) {
	cfg, err := LoadRpcConfig()
	if err != nil {
		t.Fatalf("defaults errored: %v", err)
	}
	if cfg.Mode != ModeLoopback || cfg.UDPPort != 9200 || cfg.TCPPort != 9201 ||
		cfg.GRPCPort != 9202 || cfg.Warmup != 10000 || cfg.Iterations != 100000 {
		t.Fatalf("unexpected defaults: %+v", cfg)
	}
}

func TestLoadRpcConfigClientRequiresHost(t *testing.T) {
	t.Setenv("RPC_MODE", "client")
	if _, err := LoadRpcConfig(); err == nil {
		t.Fatal("client mode without RPC_HOST did not error")
	}
}

func TestLoadRpcConfigRejectsMalformed(t *testing.T) {
	t.Setenv("RPC_ITERATIONS", "nope")
	if _, err := LoadRpcConfig(); err == nil {
		t.Fatal("malformed RPC_ITERATIONS did not error")
	}
}
