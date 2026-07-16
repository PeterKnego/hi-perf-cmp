package serjournal

import "testing"

func TestProtoRoundTripChecksum(t *testing.T) {
	scratch := make([]byte, 64*1024)
	for _, index := range []uint64{0, 1, 42} {
		r := BuildRecord(index, 4, 78)
		n := EncodeProto(ToProto(&r), scratch)
		if got, want := DecodeProtoChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
			t.Errorf("index %d: decode checksum %#x, direct fold %#x", index, got, want)
		}
	}
}

func TestProtoEncodedSizeBand(t *testing.T) {
	r := BuildRecord(0, 4, 78)
	scratch := make([]byte, 64*1024)
	n := EncodeProto(ToProto(&r), scratch)
	// ~500-byte target; loose band allows per-codec framing differences.
	if n < 450 || n > 570 {
		t.Fatalf("encoded size %d outside [450, 570]", n)
	}
}
