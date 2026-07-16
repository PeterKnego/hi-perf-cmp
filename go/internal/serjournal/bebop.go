package serjournal

import "github.com/peterknego/hi-perf-cmp/go/internal/serjournal/journalbop"

// ToBebop converts the logical record to the generated bebop representation.
// Command slices are shared, not copied — encode only reads them. Conversion
// happens in the harness's untimed pre-build phase.
func ToBebop(r *Record) journalbop.JournalRecord {
	entries := make([]journalbop.Entry, len(r.Entries))
	for i := range r.Entries {
		e := &r.Entries[i]
		entries[i] = journalbop.Entry{
			EntryTermId:    e.EntryTermID,
			EntryIndex:     e.EntryIndex,
			EntryTimestamp: e.EntryTimestamp,
			CommandKey:     e.CommandKey,
			Command:        e.Command,
		}
	}
	return journalbop.JournalRecord{
		LeadershipTermId: r.LeadershipTermID,
		LogPosition:      r.LogPosition,
		Timestamp:        r.Timestamp,
		ClusterSessionId: r.ClusterSessionID,
		CorrelationId:    r.CorrelationID,
		LeaderMemberId:   r.LeaderMemberID,
		ServiceId:        r.ServiceID,
		EventType:        r.EventType,
		RecordFlags:      r.Flags,
		Entries:          entries,
	}
}

// EncodeBebop serializes via the safe MarshalBebopTo into the reused scratch
// buffer (the unsafe fast path is deliberately not benchmarked).
func EncodeBebop(r journalbop.JournalRecord, scratch []byte) int {
	return r.MarshalBebopTo(scratch)
}

// DecodeBebopChecksum decodes (owned, allocating — the story this cell tells)
// and folds every field in the canonical checksum order.
func DecodeBebopChecksum(buf []byte) uint64 {
	var d journalbop.JournalRecord
	if err := d.UnmarshalBebop(buf); err != nil {
		panic("serjournal: bebop decode failed on harness-encoded bytes: " + err.Error())
	}
	c := NewChecksum()
	c.AddI64(d.LeadershipTermId)
	c.AddI64(d.LogPosition)
	c.AddI64(d.Timestamp)
	c.AddI64(d.ClusterSessionId)
	c.AddI64(d.CorrelationId)
	c.AddI32(d.LeaderMemberId)
	c.AddI32(d.ServiceId)
	c.AddU8(d.EventType)
	c.AddU8(d.RecordFlags)
	for i := range d.Entries {
		e := &d.Entries[i]
		c.AddI64(e.EntryTermId)
		c.AddI64(e.EntryIndex)
		c.AddI64(e.EntryTimestamp)
		c.AddI32(e.CommandKey)
		c.AddBytes(e.Command)
	}
	return c.Finish()
}
