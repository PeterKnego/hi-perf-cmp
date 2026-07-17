// Package rpcpayload holds the shared logical model for the rpc-roundtrip
// focus area's Go cells: one flat ~250-byte payload, a deterministic
// index-seeded builder, and the canonical checksum that anchors the Go and
// Rust builders to identical logical payloads (golden test).
package rpcpayload

// Record is the flat request/response payload (~250 bytes encoded). Hop is the
// mutated field (responder returns Hop+1); Seq is echoed unchanged (verified).
type Record struct {
	Hop       uint32
	Seq       uint64
	Timestamp int64
	OrderID   uint64
	Price     int64
	Qty       int64
	SymbolID  uint32
	AccountID uint64
	VenueID   uint16
	Side      uint8
	Flags     uint8
	Signature []byte // 32 bytes
	Context   []byte // 152 bytes
}

func mix(x uint64) uint64 {
	z := x + 0x9E3779B97F4A7C15
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// BuildRecord builds payload index deterministically (no RNG, no wall clock).
func BuildRecord(index uint64) Record {
	h := mix(index)
	sig := make([]byte, 32)
	s := mix(h ^ 0x05)
	for i := range sig {
		sig[i] = byte(s>>(i%8*8)) ^ byte(i)
	}
	ctx := make([]byte, 152)
	c := mix(h ^ 0x06)
	for i := range ctx {
		ctx[i] = byte(c>>(i%8*8)) ^ byte(i)
	}
	return Record{
		Hop:       uint32(index),
		Seq:       index,
		Timestamp: int64(mix(h)),
		OrderID:   mix(h ^ 0x01),
		Price:     int64(mix(h ^ 0x02)),
		Qty:       int64(mix(h ^ 0x03)),
		SymbolID:  uint32(h >> 16),
		AccountID: mix(h ^ 0x04),
		VenueID:   uint16(h >> 8),
		Side:      uint8(h & 1),
		Flags:     uint8(h >> 1),
		Signature: sig,
		Context:   ctx,
	}
}

// Checksum is the order-sensitive FNV fold both languages reproduce.
type Checksum uint64

func NewChecksum() Checksum { return 0xcbf29ce484222325 }

func (c *Checksum) step(v uint64) { *c = Checksum((uint64(*c) ^ v) * 0x100000001B3) }

func (c *Checksum) AddU64(v uint64) { c.step(v) }
func (c *Checksum) AddU32(v uint32) { c.step(uint64(v)) }
func (c *Checksum) AddU16(v uint16) { c.step(uint64(v)) }
func (c *Checksum) AddU8(v uint8)   { c.step(uint64(v)) }
func (c *Checksum) AddI64(v int64)  { c.step(uint64(v)) }
func (c *Checksum) AddBytes(b []byte) {
	c.step(uint64(len(b)))
	for _, x := range b {
		c.step(uint64(x))
	}
}
func (c Checksum) Finish() uint64 { return uint64(c) }

// ChecksumRecord folds every field in the canonical order.
func ChecksumRecord(r *Record) uint64 {
	c := NewChecksum()
	c.AddU32(r.Hop)
	c.AddU64(r.Seq)
	c.AddI64(r.Timestamp)
	c.AddU64(r.OrderID)
	c.AddI64(r.Price)
	c.AddI64(r.Qty)
	c.AddU32(r.SymbolID)
	c.AddU64(r.AccountID)
	c.AddU16(r.VenueID)
	c.AddU8(r.Side)
	c.AddU8(r.Flags)
	c.AddBytes(r.Signature)
	c.AddBytes(r.Context)
	return c.Finish()
}
