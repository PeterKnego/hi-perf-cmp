package serjournal

import (
	"google.golang.org/protobuf/proto"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalpb"
)

// ToProto converts the logical record to the generated protobuf
// representation. CmdText is a shared string, not copied — encode only reads
// it; the other command fields (CmdQty, CmdPrice, CmdFlag) are scalars copied
// by value. Conversion happens in the harness's untimed pre-build phase.
func ToProto(r *Record) *journalpb.JournalRecord {
	entries := make([]*journalpb.Entry, len(r.Entries))
	for i := range r.Entries {
		e := &r.Entries[i]
		entries[i] = &journalpb.Entry{
			EntryTermId:    e.EntryTermID,
			EntryIndex:     e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp,
			CommandKey:     e.CommandKey,
			CmdQty:         e.CmdQty,
			CmdPrice:       e.CmdPrice,
			CmdFlag:        e.CmdFlag,
			CmdText:        e.CmdText,
		}
	}
	return &journalpb.JournalRecord{
		LeadershipTermId: r.LeadershipTermID,
		LogPosition:      r.LogPosition,
		Timestamp:        r.Timestamp,
		ClusterSessionId: r.ClusterSessionID,
		CorrelationId:    r.CorrelationID,
		LeaderMemberId:   r.LeaderMemberID,
		ServiceId:        r.ServiceID,
		EventType:        uint32(r.EventType),
		Flags:            uint32(r.Flags),
		Entries:          entries,
	}
}

var protoMarshalOpts = proto.MarshalOptions{}

// EncodeProto serializes into the reused scratch buffer via MarshalAppend.
// The record (~516 B) never outgrows the 64 KiB scratch, so no reallocation
// happens inside the timed region; the guard makes a violation loud instead
// of silently corrupting the journal buffer.
func EncodeProto(r *journalpb.JournalRecord, scratch []byte) int {
	out, err := protoMarshalOpts.MarshalAppend(scratch[:0], r)
	if err != nil {
		panic("serjournal: proto encode failed: " + err.Error())
	}
	if len(out) > 0 && &out[0] != &scratch[0] {
		panic("serjournal: scratch buffer too small for encoded record")
	}
	return len(out)
}

// DecodeProtoChecksum decodes (owned, allocating — the story this cell tells)
// and folds every field in the canonical checksum order.
func DecodeProtoChecksum(buf []byte) uint64 {
	var d journalpb.JournalRecord
	if err := proto.Unmarshal(buf, &d); err != nil {
		panic("serjournal: proto decode failed on harness-encoded bytes: " + err.Error())
	}
	c := NewChecksum()
	c.AddI64(d.LeadershipTermId)
	c.AddI64(d.LogPosition)
	c.AddI64(d.Timestamp)
	c.AddI64(d.ClusterSessionId)
	c.AddI64(d.CorrelationId)
	c.AddI32(d.LeaderMemberId)
	c.AddI32(d.ServiceId)
	c.AddU8(uint8(d.EventType))
	c.AddU8(uint8(d.Flags))
	for _, e := range d.Entries {
		c.AddI64(e.EntryTermId)
		c.AddI64(e.EntryIndex)
		c.AddI64(e.EntryTimestamp)
		c.AddI32(e.CommandKey)
		c.AddI64(e.CmdQty)
		c.AddF64(e.CmdPrice)
		c.AddBool(e.CmdFlag)
		c.AddString(e.CmdText)
	}
	return c.Finish()
}
