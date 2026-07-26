package smrcoll

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"hash/crc32"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
	"github.com/peterknego/hi-perf-cmp/go/internal/smrcoll/booksnap"
)

// EncodeRoot serializes a frozen CowRoot (header + body + crc32c) into the
// reused buffer; byte-identical to Encode for the same logical state.
func (s *Snapshotter) EncodeRoot(r *CowRoot) []byte {
	s.buf.Reset()
	msg := &s.msg
	msg.PriceMin = r.PriceMin
	msg.TickSize = r.Tick
	msg.NLevels = r.NLevels
	msg.Capacity = r.Capacity
	msg.Hwm = r.Hwm
	msg.BestBid = r.BestBid
	msg.BestAsk = r.BestAsk

	msg.Levels = msg.Levels[:0]
	for side := uint8(0); side < 2; side++ {
		for t := uint32(0); t < r.NLevels; t++ {
			lvl := r.LevelAt(side, t)
			if lvl.Head == NIL {
				continue
			}
			msg.Levels = append(msg.Levels, booksnap.BookSnapshotLevels{
				Side: sideEnum(side), LevelTick: t,
				QtyTotal: lvl.QtyTotal, OrderCount: lvl.Count, Head: lvl.Head, Tail: lvl.Tail,
			})
		}
	}
	msg.Orders = msg.Orders[:0]
	for slot := uint32(0); slot < r.Hwm; slot++ {
		o := r.OrderAt(slot)
		msg.Orders = append(msg.Orders, booksnap.BookSnapshotOrders{
			Slot: slot, OrderId: o.OrderID, Price: o.Price, Qty: o.Qty, Filled: o.Filled,
			Side: sideEnum(o.Side), NextSlot: o.Next, Prev: o.Prev,
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

// RestoreCow rebuilds a fresh CowBook from an encoded image, verifying crc32c.
func RestoreCow(data []byte, cfg bench.SmrConfig) (*CowBook, error) {
	if len(data) < 4 {
		return nil, fmt.Errorf("snapshot too short")
	}
	sbeLen := len(data) - 4
	want := binary.LittleEndian.Uint32(data[sbeLen:])
	if crc32.Checksum(data[:sbeLen], crc32cTable) != want {
		return nil, fmt.Errorf("crc32c mismatch")
	}
	rd := bytes.NewReader(data[:sbeLen])
	m := booksnap.NewSbeGoMarshaller()
	var msg booksnap.BookSnapshot
	var hdr booksnap.MessageHeader
	if err := hdr.Decode(m, rd, msg.SbeSchemaVersion()); err != nil {
		return nil, err
	}
	if err := msg.Decode(m, rd, hdr.Version, hdr.BlockLength, false); err != nil {
		return nil, err
	}

	b := NewCowBook(cfg)
	b.PriceMin = msg.PriceMin
	b.Tick = msg.TickSize
	b.NLevels = msg.NLevels
	b.Hwm = msg.Hwm
	b.BestBid = msg.BestBid
	b.BestAsk = msg.BestAsk
	for i := range msg.Levels {
		lv := &msg.Levels[i]
		lvl := b.levelMut(sideU8(lv.Side), lv.LevelTick)
		lvl.Head = lv.Head
		lvl.Tail = lv.Tail
		lvl.QtyTotal = lv.QtyTotal
		lvl.Count = lv.OrderCount
	}
	for i := range msg.Orders {
		o := &msg.Orders[i]
		*b.orderMut(o.Slot) = Order{
			OrderID: o.OrderId, Price: o.Price, Qty: o.Qty, Filled: o.Filled,
			Next: o.NextSlot, Prev: o.Prev, Side: sideU8(o.Side),
		}
	}
	b.rebuildIDs()
	return b, nil
}
