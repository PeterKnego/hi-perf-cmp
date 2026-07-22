package serjournal

import "testing"

func TestFlatBuffersRoundTrip(t *testing.T) {
	codec := NewFBCodec()
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

func TestFlatBuffersEncodedSizeBand(t *testing.T) {
	codec := NewFBCodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(0, 4, 78)
	n := codec.Encode(r, scratch)
	// FlatBuffers carries vtables + offsets; ~616 B at the default config.
	if n < 550 || n > 700 {
		t.Fatalf("encoded size %d outside [550,700]", n)
	}
}

func TestFlatBuffersDecodeZeroAlloc(t *testing.T) {
	codec := NewFBCodec()
	scratch := make([]byte, 64*1024)
	r := BuildRecord(0, 4, 78)
	n := codec.Encode(r, scratch)
	frame := scratch[:n]
	avg := testing.AllocsPerRun(1000, func() { _ = codec.DecodeChecksum(frame) })
	if avg != 0 {
		t.Errorf("flatbuffers decode allocs/op = %v, want 0", avg)
	}
}
