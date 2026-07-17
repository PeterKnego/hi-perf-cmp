package rpcpayload

import "github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadbop"

// ToBebop converts the logical record to the generated bebop representation.
// Blob slices are shared, not copied (encode only reads them).
func ToBebop(r *Record) payloadbop.RpcPayload {
	return payloadbop.RpcPayload{
		Hop:         r.Hop,
		Seq:         r.Seq,
		Timestamp:   r.Timestamp,
		OrderId:     r.OrderID,
		Price:       r.Price,
		Qty:         r.Qty,
		SymbolId:    r.SymbolID,
		AccountId:   r.AccountID,
		VenueId:     r.VenueID,
		Side:        r.Side,
		RecordFlags: r.Flags,
		Signature:   r.Signature,
		Context:     r.Context,
	}
}

// EncodeBebop serializes via the safe MarshalBebopTo into the reused scratch
// buffer, returning the encoded length.
func EncodeBebop(r payloadbop.RpcPayload, scratch []byte) int {
	return r.MarshalBebopTo(scratch)
}

// DecodeBebop deserializes a framed bebop message.
func DecodeBebop(buf []byte) (payloadbop.RpcPayload, error) {
	var d payloadbop.RpcPayload
	err := d.UnmarshalBebop(buf)
	return d, err
}
