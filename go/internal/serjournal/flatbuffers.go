package serjournal

import (
	flatbuffers "github.com/google/flatbuffers/go"

	"github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalfb"
)

// FBCodec is the FlatBuffers adapter. It reuses the Builder, an offsets slice,
// and an Entry accessor so encode/decode allocate nothing on the timed path.
// Decode uses the default zero-copy accessors (not the object API).
type FBCodec struct {
	b    *flatbuffers.Builder
	offs []flatbuffers.UOffsetT
	ent  journalfb.Entry
}

// NewFBCodec allocates the reusable builder + offsets slice once.
func NewFBCodec() *FBCodec {
	return &FBCodec{b: flatbuffers.NewBuilder(4096), offs: make([]flatbuffers.UOffsetT, 0, 16)}
}

// Encode builds the record bottom-up into the reused Builder, then copies the
// finished bytes into scratch and returns the length.
func (c *FBCodec) Encode(r Record, scratch []byte) int {
	b := c.b
	b.Reset()

	// Nested objects (command vectors, Entry tables) must be built before the
	// containing entries vector / root table.
	if cap(c.offs) < len(r.Entries) {
		c.offs = make([]flatbuffers.UOffsetT, len(r.Entries))
	}
	c.offs = c.offs[:len(r.Entries)]
	for i := range r.Entries {
		e := &r.Entries[i]
		cmdOff := b.CreateByteVector(e.Command)
		journalfb.EntryStart(b)
		journalfb.EntryAddEntryTermId(b, e.EntryTermID)
		journalfb.EntryAddEntryIndex(b, e.EntryIndex)
		journalfb.EntryAddEntryTimestamp(b, e.EntryTimestamp)
		journalfb.EntryAddCommandKey(b, e.CommandKey)
		journalfb.EntryAddCommand(b, cmdOff)
		c.offs[i] = journalfb.EntryEnd(b)
	}

	journalfb.JournalRecordStartEntriesVector(b, len(r.Entries))
	for i := len(r.Entries) - 1; i >= 0; i-- {
		b.PrependUOffsetT(c.offs[i])
	}
	entriesVec := b.EndVector(len(r.Entries))

	journalfb.JournalRecordStart(b)
	journalfb.JournalRecordAddLeadershipTermId(b, r.LeadershipTermID)
	journalfb.JournalRecordAddLogPosition(b, r.LogPosition)
	journalfb.JournalRecordAddTimestamp(b, r.Timestamp)
	journalfb.JournalRecordAddClusterSessionId(b, r.ClusterSessionID)
	journalfb.JournalRecordAddCorrelationId(b, r.CorrelationID)
	journalfb.JournalRecordAddLeaderMemberId(b, r.LeaderMemberID)
	journalfb.JournalRecordAddServiceId(b, r.ServiceID)
	journalfb.JournalRecordAddEventType(b, r.EventType)
	journalfb.JournalRecordAddFlags(b, r.Flags)
	journalfb.JournalRecordAddEntries(b, entriesVec)
	root := journalfb.JournalRecordEnd(b)
	b.Finish(root)

	return copy(scratch, b.FinishedBytes())
}

// DecodeChecksum reads via zero-copy accessors and folds every field in the
// canonical ChecksumRecord order. Zero allocation: scalars read in place, the
// Entry accessor is reused, CommandBytes returns a view into the buffer.
func (c *FBCodec) DecodeChecksum(frame []byte) uint64 {
	rec := journalfb.GetRootAsJournalRecord(frame, 0)
	ck := NewChecksum()
	ck.AddI64(rec.LeadershipTermId())
	ck.AddI64(rec.LogPosition())
	ck.AddI64(rec.Timestamp())
	ck.AddI64(rec.ClusterSessionId())
	ck.AddI64(rec.CorrelationId())
	ck.AddI32(rec.LeaderMemberId())
	ck.AddI32(rec.ServiceId())
	ck.AddU8(rec.EventType())
	ck.AddU8(rec.Flags())
	n := rec.EntriesLength()
	for i := 0; i < n; i++ {
		rec.Entries(&c.ent, i)
		ck.AddI64(c.ent.EntryTermId())
		ck.AddI64(c.ent.EntryIndex())
		ck.AddI64(c.ent.EntryTimestamp())
		ck.AddI32(c.ent.CommandKey())
		ck.AddBytes(c.ent.CommandBytes())
	}
	return ck.Finish()
}
