package rpcpayload

import (
	"testing"

	"google.golang.org/protobuf/proto"
)

func TestProtoRoundTripAndSize(t *testing.T) {
	r := BuildRecord(0)
	out, err := proto.Marshal(ToProto(&r))
	if err != nil {
		t.Fatal(err)
	}
	if len(out) < 200 || len(out) > 300 {
		t.Fatalf("encoded size %d outside [200,300]", len(out))
	}
}
