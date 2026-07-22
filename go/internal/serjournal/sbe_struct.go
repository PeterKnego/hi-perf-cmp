package serjournal

import (
	"bytes"
	"io"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalsbestruct"
)

// sliceWriter is a zero-alloc io.Writer over a caller buffer (the encode
// scratch), so the streaming SbeGoMarshaller writes without an intermediate
// bytes.Buffer.
type sliceWriter struct {
	b []byte
	n int
}

func (w *sliceWriter) Write(p []byte) (int, error) {
	c := copy(w.b[w.n:], p)
	w.n += c
	if c < len(p) {
		return c, io.ErrShortWrite
	}
	return c, nil
}

// ToSBEStruct converts the logical record to the owned SBE message struct
// (untimed pre-build, like ToBebop). Command slices are shared, not copied.
func ToSBEStruct(r *Record) journalsbestruct.JournalRecord {
	entries := make([]journalsbestruct.JournalRecordEntries, len(r.Entries))
	for i := range r.Entries {
		e := &r.Entries[i]
		entries[i] = journalsbestruct.JournalRecordEntries{
			EntryTermId: e.EntryTermID, EntryIndex: e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp, CommandKey: e.CommandKey,
			CmdQty: e.CmdQty, CmdPrice: e.CmdPrice,
			CmdFlag: boolU8(e.CmdFlag), CmdText: []uint8(e.CmdText),
		}
	}
	return journalsbestruct.JournalRecord{
		LeadershipTermId: r.LeadershipTermID, LogPosition: r.LogPosition, Timestamp: r.Timestamp,
		ClusterSessionId: r.ClusterSessionID, CorrelationId: r.CorrelationID,
		LeaderMemberId: r.LeaderMemberID, ServiceId: r.ServiceID,
		EventType: journalsbestruct.EventTypeEnum(r.EventType), Flags: r.Flags, Entries: entries,
	}
}

// SBEStructCodec is the owned (struct-mode) SBE adapter. The marshaller,
// slice-writer, and bytes.Reader are reused; decode still materializes a fresh
// owned JournalRecord per call (the honest owned-decode cost).
type SBEStructCodec struct {
	m  *journalsbestruct.SbeGoMarshaller
	w  sliceWriter
	rd bytes.Reader
}

func NewSBEStructCodec() *SBEStructCodec {
	return &SBEStructCodec{m: journalsbestruct.NewSbeGoMarshaller()}
}

// Encode writes header + body into scratch through the reused slice-writer.
func (c *SBEStructCodec) Encode(msg journalsbestruct.JournalRecord, scratch []byte) int {
	c.w.b = scratch
	c.w.n = 0
	hdr := journalsbestruct.MessageHeader{
		BlockLength: msg.SbeBlockLength(), TemplateId: msg.SbeTemplateId(),
		SchemaId: msg.SbeSchemaId(), Version: msg.SbeSchemaVersion(),
	}
	_ = hdr.Encode(c.m, &c.w)
	_ = msg.Encode(c.m, &c.w, false)
	return c.w.n
}

// DecodeChecksum decodes into a fresh owned struct and folds every field.
func (c *SBEStructCodec) DecodeChecksum(frame []byte) uint64 {
	c.rd.Reset(frame)
	var msg journalsbestruct.JournalRecord
	var hdr journalsbestruct.MessageHeader
	_ = hdr.Decode(c.m, &c.rd, msg.SbeSchemaVersion())
	_ = msg.Decode(c.m, &c.rd, hdr.Version, hdr.BlockLength, false)
	ck := NewChecksum()
	ck.AddI64(msg.LeadershipTermId)
	ck.AddI64(msg.LogPosition)
	ck.AddI64(msg.Timestamp)
	ck.AddI64(msg.ClusterSessionId)
	ck.AddI64(msg.CorrelationId)
	ck.AddI32(msg.LeaderMemberId)
	ck.AddI32(msg.ServiceId)
	ck.AddU8(uint8(msg.EventType))
	ck.AddU8(msg.Flags)
	for i := range msg.Entries {
		e := &msg.Entries[i]
		ck.AddI64(e.EntryTermId)
		ck.AddI64(e.EntryIndex)
		ck.AddI64(e.EntryTimestamp)
		ck.AddI32(e.CommandKey)
		ck.AddI64(e.CmdQty)
		ck.AddF64(e.CmdPrice)
		ck.AddBool(e.CmdFlag != 0)
		ck.AddStringBytes(e.CmdText)
	}
	return ck.Finish()
}
