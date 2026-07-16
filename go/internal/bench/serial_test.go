package bench

import "testing"

func TestLoadSerialConfigDefaults(t *testing.T) {
	cfg, err := LoadSerialConfig()
	if err != nil {
		t.Fatalf("defaults errored: %v", err)
	}
	want := SerialConfig{Warmup: 1000, Iters: 100000, Entries: 4, CmdBytes: 78}
	if cfg != want {
		t.Fatalf("got %+v, want %+v", cfg, want)
	}
}

func TestLoadSerialConfigOverrides(t *testing.T) {
	t.Setenv("SER_WARMUP", "10")
	t.Setenv("SER_ITERS", "200")
	t.Setenv("SER_ENTRIES", "2")
	t.Setenv("SER_CMD_BYTES", "8")
	cfg, err := LoadSerialConfig()
	if err != nil {
		t.Fatalf("overrides errored: %v", err)
	}
	want := SerialConfig{Warmup: 10, Iters: 200, Entries: 2, CmdBytes: 8}
	if cfg != want {
		t.Fatalf("got %+v, want %+v", cfg, want)
	}
}

func TestLoadSerialConfigRejectsMalformed(t *testing.T) {
	t.Setenv("SER_ITERS", "not-a-number")
	if _, err := LoadSerialConfig(); err == nil {
		t.Fatal("malformed SER_ITERS did not error")
	}
}
