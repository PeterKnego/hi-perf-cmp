// Package serjournal holds the shared logical model for the serialization
// focus area's Go cells: one ~500-byte SMR journal record, a deterministic
// index-seeded builder, and the canonical checksum every codec's decode must
// reproduce (the full-materialization proof). Ports rust/serialization/common;
// the golden test anchors the two implementations to identical records.
package serjournal

import "math"

// Entry is one replicated command in the record's repeating group.
type Entry struct {
	EntryTermID    int64
	EntryIndex     int64
	EntryTimestamp int64
	CommandKey     int32
	CmdQty         int64
	CmdPrice       float64
	CmdFlag        bool
	CmdText        string
}

// Record mirrors Rust serialization-common's JournalRecord.
type Record struct {
	LeadershipTermID int64
	LogPosition      int64
	Timestamp        int64
	ClusterSessionID int64
	CorrelationID    int64
	LeaderMemberID   int32
	ServiceID        int32
	EventType        uint8
	Flags            uint8
	Entries          []Entry
}

// mix is one splitmix64 step — spreads field values from the record index so a
// record is byte-reproducible without RNG state or wall-clock input.
func mix(x uint64) uint64 {
	z := x + 0x9E3779B97F4A7C15
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// BuildRecord builds one journal record deterministically from index, with
// entries group members each carrying a textLen-long command text payload.
// Defaults of entries=4, textLen=78 encode to ~500 bytes.
func BuildRecord(index uint64, entries, textLen int) Record {
	h := mix(index)
	group := make([]Entry, 0, entries)
	for k := uint64(0); k < uint64(entries); k++ {
		e := mix(h ^ k*0x100000001B3)
		t := mix(e ^ 0xAA)
		text := make([]byte, textLen)
		for i := range text {
			text[i] = 0x20 + byte(t>>(i%8*8))%95
		}
		group = append(group, Entry{
			EntryTermID:    int64(e),
			EntryIndex:     int64(index*uint64(entries) + k),
			EntryTimestamp: int64(mix(e)),
			CommandKey:     int32(e >> 32),
			CmdQty:         int64(mix(e)),
			CmdPrice:       float64(mix(e^0xF0)>>11) * 3.0517578125e-5,
			CmdFlag:        mix(e^0x0F)&1 == 1,
			CmdText:        string(text),
		})
	}
	return Record{
		LeadershipTermID: int64(h),
		LogPosition:      int64(index) << 8,
		Timestamp:        int64(mix(h)),
		ClusterSessionID: int64(h >> 16),
		CorrelationID:    int64(mix(h ^ 0xABCD)),
		LeaderMemberID:   int32(h >> 8),
		ServiceID:        int32(h >> 24),
		EventType:        uint8(h & 1), // 0 = APPEND, 1 = SNAPSHOT
		Flags:            uint8(h >> 1),
		Entries:          group,
	}
}

// Checksum is the order-sensitive FNV-style accumulator every codec folds the
// decoded fields into, in the same order; equal outputs prove identical
// materialization.
type Checksum uint64

// NewChecksum starts at the FNV-1a offset basis.
func NewChecksum() Checksum { return 0xcbf29ce484222325 }

func (c *Checksum) step(v uint64) { *c = Checksum((uint64(*c) ^ v) * 0x100000001B3) }

func (c *Checksum) AddI64(v int64) { c.step(uint64(v)) }

func (c *Checksum) AddI32(v int32) { c.step(uint64(uint32(v))) }

func (c *Checksum) AddU8(v uint8) { c.step(uint64(v)) }

func (c *Checksum) AddBytes(b []byte) {
	c.step(uint64(len(b)))
	for _, x := range b {
		c.step(uint64(x))
	}
}

func (c *Checksum) AddF64(v float64) { c.step(math.Float64bits(v)) }
func (c *Checksum) AddBool(v bool) {
	if v {
		c.step(1)
	} else {
		c.step(0)
	}
}
func (c *Checksum) AddString(s string) {
	c.step(uint64(len(s)))
	for i := 0; i < len(s); i++ {
		c.step(uint64(s[i]))
	}
}

// AddStringBytes folds a UTF-8 byte view identically to AddString, so the
// zero-copy decoders (flyweight, flatbuffers) can fold cmdText without a
// string() allocation.
func (c *Checksum) AddStringBytes(b []byte) {
	c.step(uint64(len(b)))
	for _, x := range b {
		c.step(uint64(x))
	}
}

func (c Checksum) Finish() uint64 { return uint64(c) }

// ChecksumRecord is the canonical fold over a fully-owned record. Codec decode
// paths fold the same order from their decoded representations.
func ChecksumRecord(r *Record) uint64 {
	c := NewChecksum()
	c.AddI64(r.LeadershipTermID)
	c.AddI64(r.LogPosition)
	c.AddI64(r.Timestamp)
	c.AddI64(r.ClusterSessionID)
	c.AddI64(r.CorrelationID)
	c.AddI32(r.LeaderMemberID)
	c.AddI32(r.ServiceID)
	c.AddU8(r.EventType)
	c.AddU8(r.Flags)
	for i := range r.Entries {
		e := &r.Entries[i]
		c.AddI64(e.EntryTermID)
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
