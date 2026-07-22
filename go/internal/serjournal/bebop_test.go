package serjournal

import "testing"

func TestBebopRoundTripChecksum(t *testing.T) {
	scratch := make([]byte, 64*1024)
	for _, index := range []uint64{0, 1, 42} {
		r := BuildRecord(index, 4, 78)
		n := EncodeBebop(ToBebop(&r), scratch)
		if got, want := DecodeBebopChecksum(scratch[:n]), ChecksumRecord(&r); got != want {
			t.Errorf("index %d: decode checksum %#x, direct fold %#x", index, got, want)
		}
	}
}

func TestBebopEncodedSizeBand(t *testing.T) {
	r := BuildRecord(0, 4, 78)
	scratch := make([]byte, 64*1024)
	n := EncodeBebop(ToBebop(&r), scratch)
	// ~560-byte target (typed command fields); loose band allows per-codec
	// framing differences.
	if n < 530 || n > 600 {
		t.Fatalf("encoded size %d outside [530, 600]", n)
	}
}
