// Package serjournal holds the shared logical model for the serialization
// focus area's Go cells: one ~500-byte SMR journal record, a deterministic
// index-seeded builder, and the canonical checksum every codec's decode must
// reproduce (the full-materialization proof). Ports rust/serialization/common;
// the golden test anchors the two implementations to identical records.
package serjournal

// Entry is one replicated command in the record's repeating group.
type Entry struct {
	EntryTermID    int64
	EntryIndex     int64
	EntryTimestamp int64
	CommandKey     int32
	Command        []byte
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
// entries group members each carrying a cmdBytes-long command payload.
// Defaults of entries=4, cmdBytes=78 encode to ~500 bytes.
func BuildRecord(index uint64, entries, cmdBytes int) Record {
	h := mix(index)
	group := make([]Entry, 0, entries)
	for k := uint64(0); k < uint64(entries); k++ {
		e := mix(h ^ k*0x100000001B3)
		command := make([]byte, cmdBytes)
		for i := range command {
			command[i] = byte(e>>(i%8*8)) ^ byte(i)
		}
		group = append(group, Entry{
			EntryTermID:    int64(e),
			EntryIndex:     int64(index*uint64(entries) + k),
			EntryTimestamp: int64(mix(e)),
			CommandKey:     int32(e >> 32),
			Command:        command,
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
		c.AddBytes(e.Command)
	}
	return c.Finish()
}
