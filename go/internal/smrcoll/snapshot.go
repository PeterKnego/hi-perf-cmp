package smrcoll

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"hash/crc32"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll/booksnap"
)

var crc32cTable = crc32.MakeTable(crc32.Castagnoli)

// Snapshotter holds the reusable marshaller + buffer + message scratch so that
// repeated Encode calls avoid re-allocating the SBE machinery.
type Snapshotter struct {
	m   *booksnap.SbeGoMarshaller
	buf *bytes.Buffer
	msg booksnap.BookSnapshot
}

func NewSnapshotter() *Snapshotter {
	return &Snapshotter{m: booksnap.NewSbeGoMarshaller(), buf: new(bytes.Buffer)}
}

func sideEnum(side uint8) booksnap.SideEnum {
	if side == 0 {
		return booksnap.Side.BID
	}
	return booksnap.Side.ASK
}

func sideU8(s booksnap.SideEnum) uint8 {
	if s == booksnap.Side.ASK {
		return 1
	}
	return 0
}

// Encode serializes the book (header + body + 4-byte crc32c) into the reused
// buffer and returns the bytes (valid until the next Encode call).
func (s *Snapshotter) Encode(b *Book) []byte {
	s.buf.Reset()
	msg := &s.msg
	msg.PriceMin = b.PriceMin
	msg.TickSize = b.Tick
	msg.NLevels = b.NLevels
	msg.Capacity = uint32(len(b.Pool))
	msg.Hwm = b.Hwm
	msg.BestBid = b.BestBid
	msg.BestAsk = b.BestAsk

	msg.Levels = msg.Levels[:0]
	for side, lane := range [2][]Level{b.Bids, b.Asks} {
		for t := range lane {
			lvl := lane[t]
			if lvl.Head == NIL {
				continue
			}
			msg.Levels = append(msg.Levels, booksnap.BookSnapshotLevels{
				Side: sideEnum(uint8(side)), LevelTick: uint32(t),
				QtyTotal: lvl.QtyTotal, OrderCount: lvl.Count, Head: lvl.Head, Tail: lvl.Tail,
			})
		}
	}
	msg.Orders = msg.Orders[:0]
	for slot := uint32(0); slot < b.Hwm; slot++ {
		o := b.Pool[slot]
		msg.Orders = append(msg.Orders, booksnap.BookSnapshotOrders{
			Slot: slot, OrderId: o.OrderID, Price: o.Price, Qty: o.Qty, Filled: o.Filled,
			Side: sideEnum(o.Side), Next: o.Next, Prev: o.Prev,
		})
	}

	hdr := booksnap.MessageHeader{
		BlockLength: msg.SbeBlockLength(), TemplateId: msg.SbeTemplateId(),
		SchemaId: msg.SbeSchemaId(), Version: msg.SbeSchemaVersion(),
	}
	_ = hdr.Encode(s.m, s.buf)
	_ = msg.Encode(s.m, s.buf, false)

	crc := crc32.Checksum(s.buf.Bytes(), crc32cTable)
	var tmp [4]byte
	binary.LittleEndian.PutUint32(tmp[:], crc)
	s.buf.Write(tmp[:])
	return s.buf.Bytes()
}

// Restore rebuilds a fresh book from an encoded image, verifying the crc32c.
func Restore(data []byte, cfg bench.SmrConfig) (*Book, error) {
	if len(data) < 4 {
		return nil, fmt.Errorf("snapshot too short")
	}
	sbeLen := len(data) - 4
	want := binary.LittleEndian.Uint32(data[sbeLen:])
	if crc32.Checksum(data[:sbeLen], crc32cTable) != want {
		return nil, fmt.Errorf("crc32c mismatch")
	}
	r := bytes.NewReader(data[:sbeLen])
	m := booksnap.NewSbeGoMarshaller()
	var msg booksnap.BookSnapshot
	var hdr booksnap.MessageHeader
	if err := hdr.Decode(m, r, msg.SbeSchemaVersion()); err != nil {
		return nil, err
	}
	if err := msg.Decode(m, r, hdr.Version, hdr.BlockLength, false); err != nil {
		return nil, err
	}

	b := NewBook(cfg)
	b.PriceMin = msg.PriceMin
	b.Tick = msg.TickSize
	b.NLevels = msg.NLevels
	b.Hwm = msg.Hwm
	b.BestBid = msg.BestBid
	b.BestAsk = msg.BestAsk
	for i := range msg.Levels {
		lv := &msg.Levels[i]
		lane := b.Bids
		if sideU8(lv.Side) == 1 {
			lane = b.Asks
		}
		lane[lv.LevelTick] = Level{Head: lv.Head, Tail: lv.Tail, QtyTotal: lv.QtyTotal, Count: lv.OrderCount}
	}
	for i := range msg.Orders {
		o := &msg.Orders[i]
		b.Pool[o.Slot] = Order{
			OrderID: o.OrderId, Price: o.Price, Qty: o.Qty, Filled: o.Filled,
			Next: o.Next, Prev: o.Prev, Side: sideU8(o.Side),
		}
	}
	b.rebuildIDs()
	return b, nil
}
