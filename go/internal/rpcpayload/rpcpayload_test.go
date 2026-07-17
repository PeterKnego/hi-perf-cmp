package rpcpayload

import "testing"

// Golden values generated from the Rust rpc_roundtrip_common builder on 2026-07-17.
var golden = []struct {
	index uint64
	want  uint64
}{
	{0, 0x51694f16fd7829b6},
	{1, 0x42bd19ed5deb1079},
	{42, 0x2a8920402906b171},
	{99999, 0x97ca10ed0ba917b7},
}

func TestGoldenChecksumsMatchRust(t *testing.T) {
	for _, g := range golden {
		r := BuildRecord(g.index)
		if got := ChecksumRecord(&r); got != g.want {
			t.Errorf("build(%d): got %#016x, want %#016x", g.index, got, g.want)
		}
	}
}

func TestBuildRecordDeterministicAndSized(t *testing.T) {
	a, b := BuildRecord(7), BuildRecord(7)
	if ChecksumRecord(&a) != ChecksumRecord(&b) {
		t.Fatal("same index produced different records")
	}
	if len(a.Signature) != 32 || len(a.Context) != 152 {
		t.Fatalf("blob sizes: sig=%d ctx=%d", len(a.Signature), len(a.Context))
	}
}
