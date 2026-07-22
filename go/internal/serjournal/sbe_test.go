package serjournal

import (
	"bytes"
	"os"
	"testing"
)

func TestSBEFlyweightRoundTrip(t *testing.T) {
	codec := NewSBECodec()
	scratch := make([]byte, 64*1024)
	for _, cfg := range [][2]int{{4, 78}, {2, 8}, {6, 40}} {
		for _, idx := range []uint64{0, 1, 42} {
			r := BuildRecord(idx, cfg[0], cfg[1])
			n := codec.Encode(r, scratch)
			if got, want := codec.DecodeChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
				t.Errorf("cfg%v idx%d: decode checksum %#x != fold %#x", cfg, idx, got, want)
			}
		}
	}
}

func TestSBEFlyweightByteIdentity(t *testing.T) {
	golden, err := os.ReadFile("testdata/journal_sbe_golden.bin")
	if err != nil {
		t.Fatal(err)
	}
	if len(golden) != 570 {
		t.Fatalf("golden size %d, want 570", len(golden))
	}
	codec := NewSBECodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(7, 4, 78)
	n := codec.Encode(r, scratch)
	if !bytes.Equal(scratch[:n], golden) {
		t.Fatalf("flyweight frame (%d bytes) not byte-identical to Rust golden", n)
	}
}

func TestSBEFlyweightDecodeZeroAlloc(t *testing.T) {
	codec := NewSBECodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(0, 4, 78)
	n := codec.Encode(r, scratch)
	frame := scratch[:n]
	avg := testing.AllocsPerRun(1000, func() { _ = codec.DecodeChecksum(frame) })
	if avg != 0 {
		t.Errorf("flyweight decode allocs/op = %v, want 0", avg)
	}
}

func TestSBEStructRoundTrip(t *testing.T) {
	codec := NewSBEStructCodec()
	scratch := make([]byte, 64*1024)
	for _, cfg := range [][2]int{{4, 78}, {2, 8}, {6, 40}} {
		for _, idx := range []uint64{0, 1, 42} {
			r := BuildRecord(idx, cfg[0], cfg[1])
			n := codec.Encode(ToSBEStruct(&r), scratch)
			if got, want := codec.DecodeChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
				t.Errorf("cfg%v idx%d: decode checksum %#x != fold %#x", cfg, idx, got, want)
			}
		}
	}
}

func TestSBEStructByteIdentity(t *testing.T) {
	golden, err := os.ReadFile("testdata/journal_sbe_golden.bin")
	if err != nil {
		t.Fatal(err)
	}
	codec := NewSBEStructCodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(7, 4, 78)
	n := codec.Encode(ToSBEStruct(&r), scratch)
	if !bytes.Equal(scratch[:n], golden) {
		t.Fatalf("struct frame (%d bytes) not byte-identical to Rust golden", n)
	}
}
