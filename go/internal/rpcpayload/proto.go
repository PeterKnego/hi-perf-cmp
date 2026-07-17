package rpcpayload

import "github.com/peterknego/hi-perf-cmp/go/internal/rpcpayload/payloadpb"

// ToProto converts the logical record to the generated protobuf representation.
// Blob slices are shared, not copied.
func ToProto(r *Record) *payloadpb.RpcPayload {
	return &payloadpb.RpcPayload{
		Hop:       r.Hop,
		Seq:       r.Seq,
		Timestamp: r.Timestamp,
		OrderId:   r.OrderID,
		Price:     r.Price,
		Qty:       r.Qty,
		SymbolId:  r.SymbolID,
		AccountId: r.AccountID,
		VenueId:   uint32(r.VenueID),
		Side:      uint32(r.Side),
		Flags:     uint32(r.Flags),
		Signature: r.Signature,
		Context:   r.Context,
	}
}
