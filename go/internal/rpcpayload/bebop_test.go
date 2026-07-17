package rpcpayload

import "testing"

func TestBebopRoundTripAndSize(t *testing.T) {
	r := BuildRecord(0)
	scratch := make([]byte, 64*1024)
	n := EncodeBebop(ToBebop(&r), scratch)
	if n < 200 || n > 300 {
		t.Fatalf("encoded size %d outside [200,300]", n)
	}
	d, err := DecodeBebop(scratch[:n])
	if err != nil {
		t.Fatal(err)
	}
	if d.Hop != r.Hop || d.Seq != r.Seq || d.RecordFlags != r.Flags ||
		len(d.Signature) != 32 || len(d.Context) != 152 {
		t.Fatalf("field mismatch: %+v", d)
	}
}
