package serjournal

import (
	"unsafe"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalsbe"
)

// SBECodec is the zero-copy SBE (flyweight) adapter. It reuses the flyweight
// message/header structs and a command scratch buffer so DecodeChecksum
// allocates nothing on the timed path.
type SBECodec struct {
	enc journalsbe.JournalRecord
	hdr journalsbe.MessageHeader
	dec journalsbe.JournalRecord
	cmd []byte
}

// NewSBECodec allocates the reusable flyweight state once.
func NewSBECodec() *SBECodec {
	return &SBECodec{cmd: make([]byte, 64*1024)}
}

// Encode writes a full framed message (header + body) into scratch and returns
// its length. Zero-copy: the flyweight writes fields directly at wire offsets.
func (c *SBECodec) Encode(r Record, scratch []byte) int {
	m := &c.enc
	m.WrapAndApplyHeader(scratch, 0, uint64(len(scratch)))
	m.SetLeadershipTermId(r.LeadershipTermID).
		SetLogPosition(r.LogPosition).
		SetTimestamp(r.Timestamp).
		SetClusterSessionId(r.ClusterSessionID).
		SetCorrelationId(r.CorrelationID).
		SetLeaderMemberId(r.LeaderMemberID).
		SetServiceId(r.ServiceID).
		SetEventType(journalsbe.EventType(r.EventType)).
		SetFlags(r.Flags)
	g := m.EntriesCount(uint16(len(r.Entries)))
	for i := range r.Entries {
		e := &r.Entries[i]
		g.Next()
		g.SetEntryTermId(e.EntryTermID).
			SetEntryIndex(e.EntryIndex).
			SetEntryTimestamp(e.EntryTimestamp).
			SetCommandKey(e.CommandKey)
		// PutCommand takes a string but copies the bytes immediately and never
		// retains the header, so an unsafe.String view avoids a per-entry alloc.
		if len(e.Command) > 0 {
			g.PutCommand(unsafe.String(&e.Command[0], len(e.Command)))
		} else {
			g.PutCommand("")
		}
	}
	return int(journalsbe.MessageHeaderEncodedLength) + int(m.EncodedLength())
}

// DecodeChecksum decodes the framed message in place and folds every field in
// the canonical ChecksumRecord order (full materialization). Zero allocation:
// scalars are read in place, the command is copied into the reused buffer.
func (c *SBECodec) DecodeChecksum(frame []byte) uint64 {
	c.hdr.Wrap(frame, 0, 0, uint64(len(frame)))
	c.dec.WrapForDecode(frame, uint64(journalsbe.MessageHeaderEncodedLength),
		uint64(c.hdr.BlockLength()), uint64(c.hdr.Version()), uint64(len(frame)))
	ck := NewChecksum()
	ck.AddI64(c.dec.LeadershipTermId())
	ck.AddI64(c.dec.LogPosition())
	ck.AddI64(c.dec.Timestamp())
	ck.AddI64(c.dec.ClusterSessionId())
	ck.AddI64(c.dec.CorrelationId())
	ck.AddI32(c.dec.LeaderMemberId())
	ck.AddI32(c.dec.ServiceId())
	ck.AddU8(uint8(c.dec.EventType()))
	ck.AddU8(c.dec.Flags())
	e := c.dec.Entries()
	for e.HasNext() {
		e.Next()
		ck.AddI64(e.EntryTermId())
		ck.AddI64(e.EntryIndex())
		ck.AddI64(e.EntryTimestamp())
		ck.AddI32(e.CommandKey())
		n := e.GetCommand(c.cmd)
		ck.AddBytes(c.cmd[:n])
	}
	return ck.Finish()
}
